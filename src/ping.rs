use chrono::Utc;
//use tokio_stream::Elapsed;
//use futures::future::ok;
use std::process::{Command, Stdio};

use crate::{
    get_channel_details,
    lcd::{self, Lc, RunningStatus},
    player_status::{self, PlayerStatus, NUMBER_OF_POSSIBLE_CHANNELS},
};
#[derive(Debug, PartialEq)]

pub enum PingStatus {
    /// a ping has not been sent
    PingNotSent,
    /// A ping response has been received, but the next one has not yet been sent
    PingResponseReceived,
    /// The last ping sent times out
    TimedOut,
    /// A ping has been sent, but nothing has come back yet, not even a timeout
    PingSent,
}

#[derive(Debug, PartialEq, Clone)]
/// stores the address being pinged, either local, remote or nothing
pub enum PingWhat {
    Local,
    Remote,
    Nothing,
}

#[derive(Debug, PartialEq)]
/// Used to store the data about the pings
pub struct PingData {
    pub ping_status: PingStatus,
    pub last_ping_time_of_day: chrono::DateTime<Utc>, // the time the last ping was sent; used so we do not ping too often
    pub destination_to_ping: PingWhat,                // the address the next ping has to be sent to
    pub destination_of_last_ping: PingWhat,           // the address we sent the last ping to
    pub ping_time: f32,                               // the time the last ping took
    pub number_of_remote_pings_to_this_station: u32,
    pub reached_number_of_remote_pings: bool, // specifies if we have reached the number of pings specified in config
}
impl PingData {
    pub fn new() -> Self {
        Self {
            last_ping_time_of_day: chrono::Utc::now(),
            ping_status: PingStatus::PingNotSent,
            destination_to_ping: PingWhat::Local,
            destination_of_last_ping: PingWhat::Nothing,
            ping_time: 0.0,
            number_of_remote_pings_to_this_station: 0,
            reached_number_of_remote_pings: false,
        }
    }
    /// Toggles between specifying pinging (Local or Remote) & (Local or Nothing)
    pub fn toggle_ping_destination(&mut self) -> () {
        self.destination_to_ping = match self.destination_to_ping {
            PingWhat::Remote => PingWhat::Local,
            PingWhat::Local => {
                if self.reached_number_of_remote_pings {
                    PingWhat::Nothing
                } else {
                    PingWhat::Remote
                }
            }
            PingWhat::Nothing => PingWhat::Local,
        }
    }
}
/// Sends a ping to the local or remote address as required and sets the flag "destination_of_last_ping accordingly".
/// Panics if it cannot ping.
/// Assumes that if destination_to_ping is not Remote, it must ping the local address
pub fn send_ping(status_of_rradio: &mut player_status::PlayerStatus) -> std::process::Child {
    status_of_rradio.ping_data.destination_of_last_ping =
        status_of_rradio.ping_data.destination_to_ping.clone();
    status_of_rradio.ping_data.ping_status = PingStatus::PingSent;
    status_of_rradio
        .ping_data
        .number_of_remote_pings_to_this_station += 1; // will take > 100 years to overflow; so no concern
    status_of_rradio.ping_data.last_ping_time_of_day = chrono::Utc::now();
    let address = if status_of_rradio.ping_data.destination_to_ping == PingWhat::Remote {
        status_of_rradio.network_data.remote_address.clone()
    } else {
        status_of_rradio.network_data.gateway_ip_address.to_string()
    };
    Command::new("/bin/ping")
        .args([
            address,
            "-c".to_string(),
            "1".to_string(),
            "-W".to_string(),
            "3".to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute child process when trying to ping")
}

/// Updates status_of_rradio.ping_data giving either PingResponseReceived or TimedOut if a response is received,
/// but not too recently so we do not ping too often
/// Otherwise does nothing
pub fn see_if_there_is_a_ping_response(
    child: &mut std::process::Child,
    status_of_rradio: &mut player_status::PlayerStatus,
) {
    // must not ping too frequently; so return without doing anything if the previous ping is recent or we should not be sending pings
    if ((chrono::Utc::now() - status_of_rradio.ping_data.last_ping_time_of_day).num_milliseconds()
        > 2000)
        && (status_of_rradio.channel_number <= NUMBER_OF_POSSIBLE_CHANNELS)
        && ((status_of_rradio.position_and_duration[status_of_rradio.channel_number]
            .channel_data
            .source_type
            == get_channel_details::SourceType::UrlList)
            || (status_of_rradio.running_status == RunningStatus::Startingup))
    {
        if let Ok(exit_status) = child.wait() {
            status_of_rradio.ping_data.ping_status = if exit_status.success() {
                PingStatus::PingResponseReceived
            } else {
                PingStatus::TimedOut
            };
        }
    }
}

/// return if it worked as an Output & stores the ping time in status_of_rradio.ping_data.ping_time
/// Can only usefully be called after checking that a ping reponse has been received (which can be done by using see_if_there_is_a_ping_response)
pub fn get_ping_time(
    ping_output: Result<std::process::Output, std::io::Error>,
    status_of_rradio: &mut player_status::PlayerStatus,
) -> Result<(), String> {
    if status_of_rradio.ping_data.ping_status != PingStatus::PingResponseReceived {
        return Err("Cannot get ping time if a valid ping has not been returned".to_string());
    }
    match ping_output {
        Ok(output) => {
            let mut output_as_ascii = unsafe { String::from_utf8_unchecked(output.stdout) }; // convert the output, which is a series of bytes, to a string
            let split_text = "mdev = ";
            if let Some(position_mdev) = output_as_ascii.find(split_text) {
                let mut ping_time = output_as_ascii.split_off(position_mdev + split_text.len()); // at this point, the string contains too much trailing text.
                if let Some(position_slash) = ping_time.find('/') {
                    let _ = ping_time.split_off(position_slash);
                    match ping_time.parse::<f32>() {
                        Ok(time) => {
                            status_of_rradio.ping_data.ping_time = time;
                        }
                        Err(error_message) => {
                            status_of_rradio.ping_data.ping_time = 0.0;

                            return Err(format!(
                                "Could not convert the ping time \" {} to a float; got {}\r",
                                ping_time, error_message
                            ));
                        }
                    }

                    Ok(())
                } else {
                    Err("Could not find the terminating slash when looking for a ping".to_string())
                }
            } else {
                Err("could not parse the time returned from ping".to_string())
            }
        }
        Err(error) => Err(error.to_string()),
    }
}
