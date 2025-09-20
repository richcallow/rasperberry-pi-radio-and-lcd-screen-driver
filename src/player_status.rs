use chrono::Duration;

use crate::{
    get_channel_details,
    lcd::{
        self,
        get_local_ip_address::{self, NetworkData},
        RunningStatus,
    },
    ping, read_config,
};

#[derive(Debug, Clone, Copy)]
/// stores the position of the tracks, ie the time since starting to play it
/// &, it it is a streaming channel, the duration of the channel.
pub struct PositionAndDuration {
    pub position: Duration,
    pub duration_ms: Option<u64>,
}
/// The maximum possible as the channel number is 2 decimal digits
const HIGHEST_POSSIBLE_CHANNEL_NUMBER: usize = 99;
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
    pub line_1_data: lcd::ScrollData,
    pub line_2_data: lcd::ScrollData,
    pub line_34_data: lcd::ScrollData,
    /// Stores organisation, a vec of startion URLs & whether or not the last track is a ding
    pub channel_file_data: get_channel_details::ChannelFileDataDecoded,
    pub buffering_percent: i32,
    /// stores SSID, local IP address & gateway address
    pub network_data: get_local_ip_address::NetworkData,
    /// true if the USB is mounted locally
    pub usb_is_mounted: bool,
    pub ping_data: ping::PingData,
}
impl PlayerStatus {
    pub fn new(config: &read_config::Config) -> PlayerStatus {
        use crate::SourceType;
        PlayerStatus {
            toml_error: None,
            running_status: lcd::RunningStatus::Startingup,
            artist: String::new(),
            channel_number: 101, // an invalid value that cannot match as must be in the range 0 to 99 inclusive
            previous_channel_number: 102,
            position_and_duration: [PositionAndDuration {
                position: Duration::seconds(0),
                duration_ms: None,
            }; HIGHEST_POSSIBLE_CHANNEL_NUMBER],
            all_4lines: lcd::ScrollData::new("", 4),
            line_1_data: lcd::ScrollData::new("", 1),
            line_2_data: lcd::ScrollData::new("", 1),
            line_34_data: lcd::ScrollData::new("", 2),
            channel_file_data: get_channel_details::ChannelFileDataDecoded {
                organisation: String::new(),
                station_urls: vec![],
                source_type: SourceType::Unknown,
                last_track_is_a_ding: false,
            },
            index_to_current_track: 0,
            current_volume: config.initial_volume,
            gstreamer_state: gstreamer::State::Null,
            buffering_percent: 0,
            usb_is_mounted: false,
            network_data: NetworkData::new(),
            ping_data: ping::PingData::new(),
        }
    }

    /// Tries multiple times to get the WiFi data & store it in self.network_data.
    /// Sets self.running_status to RunningStatus::LongMessageOnAll4Lines so that its attempts can be seen on the LCD screen
    /// Sets self.running_status to RunningStatus::Startingup if successful
    pub fn update_network_data(
        &mut self,
        lcd: &mut crate::lcd::Lc,
        config: &crate::read_config::Config,
    ) {
        self.running_status = RunningStatus::LongMessageOnAll4Lines;
        for count in 0..40 {
            // go round the loop multiple times looking for the IP address
            self.all_4lines.update_if_changed(
                format!("Looking for IP address. Attempt number {count}").as_str(),
            );
            lcd.write_rradio_status_to_lcd(self, config);

            match crate::get_local_ip_address::try_once_to_get_wifi_network_data() {
                Ok(network_data) => {
                    self.network_data = network_data;
                    self.running_status = crate::RunningStatus::Startingup;
                    return;
                }
                Err(error) => self
                    .all_4lines
                    .update_if_changed(format!("Got error {error}  on count {count}").as_str()),
            }
        }
    }
}
