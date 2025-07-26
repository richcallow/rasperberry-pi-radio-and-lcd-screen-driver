use chrono::Duration;

use crate::{get_channel_details, lcd, read_config};

#[derive(Debug, Clone, Copy)]
/// stores the position of the tracks, ie the time since starting to play it
/// &, it it is a streaming channel, the duration of the channel.
pub struct PositionAndDuration {
    pub position: Duration,
    pub duration_ms: Option<u64>,
}
/// The maximum possible as the channel number is 2 decimal digits
const HIGHEST_POSSIBLE_CHANNEL_NUMBER: usize = 100;
#[derive(Debug)] // neither Copy nor clone are implmented as the player can only have a single status
/// A struct listing all information needed to dispaly the status of rradio.
pub struct PlayerStatus {
    pub toml_error: Option<String>,
    /// Specifies if we are starting up, in which case we want to see the startup message, shutting down or running normally.
    /// or there is a bad error
    pub running_status: lcd::RunningStatus,
    /// Derived from gstreamer tags, & thus applies only to the track currently being played
    pub artist: String,
    pub position_and_duration: [PositionAndDuration; HIGHEST_POSSIBLE_CHANNEL_NUMBER],
    /// in the range 00 to 99
    pub channel_number: u8,
    pub previous_channel_number: u8, // in the range 00 to 99
    /// This specifies which of the tracks we are currently playing on a USB stick/CD or which station if there are multiple stations with the same channel number, starting at zero
    pub index_to_current_track: usize,
    pub current_volume: i32,
    pub gstreamer_state: gstreamer::State,
    pub all_4lines: lcd::ScrollData,
    pub line_2_data: lcd::ScrollData,
    pub line_34_data: lcd::ScrollData,
    pub channel_file_data: get_channel_details::ChannelFileDataDecoded,
    pub buffering_percent: i32,
    /// true if the USB is mounted locally
    pub usb_is_mounted: bool,
}
impl PlayerStatus {
    pub fn new(config: &read_config::Config) -> PlayerStatus {
        use crate::SourceType;
        PlayerStatus {
            toml_error: None,
            running_status: lcd::RunningStatus::Startingup,
            artist: String::new(),
            channel_number: 101, // an invalid value that cannot match
            previous_channel_number: 101,
            position_and_duration: [PositionAndDuration {
                position: Duration::seconds(0),
                duration_ms: None,
            }; HIGHEST_POSSIBLE_CHANNEL_NUMBER],
            all_4lines: lcd::ScrollData::new("", 4),
            line_2_data: lcd::ScrollData::new("", 1),
            line_34_data: lcd::ScrollData::new("", 2),
            channel_file_data: get_channel_details::ChannelFileDataDecoded {
                organisation: String::new(),
                station_url: vec![],
                source_type: SourceType::Unknown,
                last_track_is_a_ding: false,
            },
            index_to_current_track: 0,
            current_volume: config.initial_volume,
            gstreamer_state: gstreamer::State::Null,
            buffering_percent: 0,
            usb_is_mounted: false,
        }
    }
}
