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
    pub ping_time: String,                            // the time the last ping took
}
impl PingData {
    pub fn new() -> Self {
        Self {
            last_ping_time_of_day: chrono::Utc::now(),
            ping_status: PingStatus::PingNotSent,
            destination_to_ping: PingWhat::Local,
            destination_of_last_ping: PingWhat::Nothing,
            ping_time: String::new(),
        }
    }
}
/// sends a ping to the local or remote address as required and sets the flag "destination_of_last_ping accordingly".
/// panics if it cannot ping.
pub fn send_ping(status_of_rradio: &mut player_status::PlayerStatus) -> std::process::Child {
    status_of_rradio.ping_data.destination_of_last_ping =
        status_of_rradio.ping_data.destination_to_ping.clone();
    status_of_rradio.ping_data.ping_status = PingStatus::PingSent;
    status_of_rradio.ping_data.last_ping_time_of_day = chrono::Utc::now();
    let address = if status_of_rradio.ping_data.destination_to_ping == PingWhat::Local {
        status_of_rradio.network_data.gateway_ip_address.to_string()
    } else {
        status_of_rradio.network_data.remote_address.clone()
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

/// return
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
                if let Some(position_decimal_point) = ping_time.find('.') {
                    let _ = ping_time.split_off(position_decimal_point + 2);
                    status_of_rradio.ping_data.ping_time = ping_time;
                    Ok(())
                } else {
                    Err("Could not find the decimal point when looking for a ping".to_string())
                }
            } else {
                Err("could not parse the time returned from ping".to_string())
            }
        }
        Err(error) => Err(error.to_string()),
    }
}
