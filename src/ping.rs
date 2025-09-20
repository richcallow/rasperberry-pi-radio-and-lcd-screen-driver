use chrono::Utc;
//use tokio_stream::Elapsed;
//use futures::future::ok;
use std::process::{Command, Stdio};

use crate::player_status;
#[derive(Debug, PartialEq)]
pub enum PingStatus {
    PingNotSent,
    PingResponseReceived,
    TimedOut,
    PingSent,
}

#[derive(Debug, PartialEq)]
pub enum PingWhat {
    Local,
    Remote,
    Nothing,
}

#[derive(Debug, PartialEq)]
pub struct PingData {
    pub ping_status: PingStatus,
    pub last_ping_time: chrono::DateTime<Utc>,
    pub ping_destination: PingWhat,
}
impl PingData {
    pub fn new() -> Self {
        Self {
            last_ping_time: chrono::Utc::now(),
            ping_status: PingStatus::PingNotSent,
            ping_destination: PingWhat::Local,
        }
    }
}

pub fn send_ping(status_of_rradio: &mut player_status::PlayerStatus) -> std::process::Child {
    status_of_rradio.ping_data.ping_status = PingStatus::PingSent;
    status_of_rradio.ping_data.last_ping_time = chrono::Utc::now();
    let addressn = status_of_rradio.network_data.gateway_ip_address.to_string();
    Command::new("/bin/ping")
        .args([
            addressn,
            "-c".to_string(),
            "1".to_string(),
            "-W".to_string(),
            "3".to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute child")
}

/// Updates status_of_rradio.ping_data giving either PingResponseReceived or TimedOut if a response is received,
/// but not too recently so we do not ping too often
/// Otherwise does nothing
pub fn get_ping_response(
    child: &mut std::process::Child,
    status_of_rradio: &mut player_status::PlayerStatus,
) {
    // must not ping too frequently; so return without doing anything if the previous ping is recent
    if (chrono::Utc::now() - status_of_rradio.ping_data.last_ping_time).num_milliseconds() > 2000 {
        if let Ok(exit_status) = child.wait() {
            status_of_rradio.ping_data.ping_status = if exit_status.success() {
                PingStatus::PingResponseReceived
            } else {
                PingStatus::TimedOut
            };
        }
    }
}

pub fn get_ping_time(
    ping_output: Result<std::process::Output, std::io::Error>,
    status_of_rradio: &mut player_status::PlayerStatus,
) -> Result<String, String> {
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
                    Ok(ping_time)
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
