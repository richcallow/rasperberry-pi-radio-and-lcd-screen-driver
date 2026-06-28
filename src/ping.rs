// sends & receives pings & gets the ping time
use std::process::{Command, Stdio};
//use pnet_packet::ip;

use crate::{
    get_channel_details,
    lcd::RunningStatus,
    player_status::{self, NUMBER_OF_POSSIBLE_CHANNELS},
};

#[derive(Debug)]
/// Stores the ping time returned as an option (timeout => none()
pub struct PingTimeAndDestination {
    /// If it times out, there is no time to include; it that case, it returns None
    pub time_in_ms: Option<f32>,
    pub destination: PingWhere,
}

#[derive(Debug, PartialEq)]
/// Stores the address being pinged, either local, remote or nothing
pub enum PingWhere {
    Local,
    Remote,
    Nothing,
}
impl PingWhere {
    /// converts to a long string, as used at startup
    pub fn to_long_string(&self) -> String {
        match self {
            PingWhere::Local => "Local ping ".to_string(),
            PingWhere::Remote => "Remote Ping ".to_string(),
            PingWhere::Nothing => "No destination ".to_string(),
        }
    }
    /// converts to a short string, as used on line1 when things work OK
    pub fn to_short_string(&self) -> String {
        match self {
            PingWhere::Local => "LocPing".to_string(),
            PingWhere::Remote => "RemPing".to_string(),
            PingWhere::Nothing => "No dest".to_string(),
        }
    }
    /// converts to a single character, when space is very much at a premium
    pub fn to_single_character(&self) -> String {
        match self {
            PingWhere::Local => "L".to_string(),
            PingWhere::Remote => "R".to_string(),
            PingWhere::Nothing => "N".to_string(),
        }
    }
}

#[derive(Debug)]
/// Used to store the data about the pings
pub struct PingData {
    /// true if we can send a ping
    pub can_send_ping: bool,
    /// time of day the last ping was sent; used to ensure we do not ping too often
    pub last_ping_time_of_day: chrono::DateTime<chrono::Utc>, // the time the last ping was sent; used so we do not ping too often
    /// the time the ping took & the destination, local, remote or nothing.
    pub ping_time_and_destination: PingTimeAndDestination,
    pub number_of_pings_to_this_channel: u32,
}

/// Sends a ping to the local or remote address as required.
/// Panics if it cannot ping.
/// Sets can_send_ping to false as cannot ping again until we have received a response.
/// When we have sent more than max_number_of_remote_pings, all the pings go to the router
/// so as not to cause the remote site to be concerned about the number of pings.
/// (The display routine displays the temperature instead of the remote ping)
pub fn send_ping(
    status_of_rradio: &mut player_status::PlayerStatus,
    config: &crate::read_config::Config,
) -> std::process::Child {
    status_of_rradio.ping_data.last_ping_time_of_day = chrono::Utc::now();

    let number_of_remote_pings_to_this_channel =
        status_of_rradio.ping_data.number_of_pings_to_this_channel;

    let address = if (number_of_remote_pings_to_this_channel & 1 != 0)
        || (number_of_remote_pings_to_this_channel > config.max_number_of_remote_pings)
    {
        &status_of_rradio.network_data.gateway_ip_address
    } else {
        &status_of_rradio.position_and_duration[status_of_rradio.channel_number].address_to_ping
    }
    .as_str();

    let return_value = Command::new("/bin/ping")
        .args([
            address, "-c", "1", // send one ping and then stop
            "-W", "3", // wait that number of seconds before timing out
        ])
        //.stdin(Stdio::piped())    // not needed as we do not send anything after the initial command
        .stdout(Stdio::piped()) // needed as we need to capture what is sent back
        .spawn()
        .expect("failed to execute child process when trying to ping");

    status_of_rradio.ping_data.can_send_ping = false;
    status_of_rradio.ping_data.number_of_pings_to_this_channel += 1; // will take > 100 years to overflow; so no concern

    return_value
}

/// status_of_rradio.ping_data.can_send_ping = true if a response is received, but not too recently so we do not ping too often
/// Otherwise does nothing
pub fn see_if_there_is_a_ping_response(status_of_rradio: &mut player_status::PlayerStatus) {
    if ((chrono::Utc::now() - status_of_rradio.ping_data.last_ping_time_of_day).num_milliseconds()
        > 3000)
        && (status_of_rradio.channel_number <= NUMBER_OF_POSSIBLE_CHANNELS)// only ping valid channels
        && ((status_of_rradio.position_and_duration[status_of_rradio.channel_number]
            .channel_data
            .source_type
            == get_channel_details::SourceType::UrlList)
            || (status_of_rradio.running_status == RunningStatus::Startingup))
    {
        status_of_rradio.ping_data.can_send_ping = true
    }
}

/// If it worked, stores the result in status_of_rradio.ping_data.ping_time_and_destination
/// Can only usefully be called after checking that a ping reponse has been received (which can be done by using see_if_there_is_a_ping_response)
pub fn get_ping_time(
    ping_output: Result<std::process::Output, std::io::Error>,
    status_of_rradio: &mut player_status::PlayerStatus,
) -> Result<(), String> {
    if !status_of_rradio.ping_data.can_send_ping {
        return Err("Cannot get ping time if a valid ping has not been returned".to_string());
    }
    match ping_output {
        Ok(output) => {
            // convert the bytes to a str
            let (ip_address_only, time_data) = std::str::from_utf8(&output.stdout)
                .unwrap_or_default()
                .strip_prefix("PING ")
                .unwrap_or("Error could not find prefix")
                .split_once(" ")
                .unwrap_or_default();
            let destination = if ip_address_only == status_of_rradio.network_data.gateway_ip_address
            {
                PingWhere::Local
            } else {
                PingWhere::Remote
            };

            let (_, time_data2) = time_data.split_once("mdev = ").unwrap_or_default();
            // the time is between the str "mdev = " & the next /
            let (time_as_str, _) = time_data2.split_once("/").unwrap_or_default();
            let time = time_as_str.parse::<f32>().unwrap_or_default();

            status_of_rradio.ping_data.ping_time_and_destination = PingTimeAndDestination {
                time_in_ms: Some(time),
                destination,
            };
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}
