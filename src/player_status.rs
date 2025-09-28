use chrono::Duration;

use crate::{
    get_channel_details::{self, ChannelFileDataDecoded},
    lcd::{
        self,
        get_local_ip_address::{self, NetworkData},
        RunningStatus,
    },
    ping,
    read_config::{self, Config},
};

#[derive(Debug, Clone)]
/// stores the decoded channel file data, the position of the tracks, ie the time since starting to play it
/// &, if it is a streaming channel, the duration of the channel.
pub struct RealTimeDataOnOneChannel {
    pub channel_data: ChannelFileDataDecoded,
    pub artist: String,
    pub index_to_current_track: usize,
    pub position: Duration,
    pub duration_ms: Option<u64>,
}
impl RealTimeDataOnOneChannel {
    pub fn new() -> Self {
        Self {
            channel_data: get_channel_details::ChannelFileDataDecoded::new(),
            index_to_current_track: 0,
            position: Duration::seconds(0),
            duration_ms: None,
            artist: String::new(),
        }
    }
}

/// The maximum possible as the channel number is 2 decimal digits. (The ding channel 100, so the user cannot enter it.)
pub const NUMBER_OF_POSSIBLE_CHANNELS: usize = 100;
pub const START_UP_DING_CHANNEL_NUMER: usize = NUMBER_OF_POSSIBLE_CHANNELS;
#[derive(Debug)] // neither Copy nor clone are implmented as the player can only have a single status
/// A struct listing all information needed to dispaly the status of rradio.
pub struct PlayerStatus {
    pub toml_error: Option<String>,
    /// Specifies if we are starting up, in which case we want to see the startup message, shutting down or running normally.
    /// or there is a bad error
    pub running_status: lcd::RunningStatus,
    /// in the range 00 to 99, normally, but the ding channel is 100
    pub channel_number: usize,
    pub current_volume: i32,
    pub gstreamer_state: gstreamer::State,
    pub buffering_percent: i32,
    /// stores SSID, local IP address & gateway address
    pub network_data: get_local_ip_address::NetworkData,
    /// true if the USB is mounted locally
    pub usb_is_mounted: bool,
    pub ping_data: ping::PingData,
    pub all_4lines: lcd::ScrollData,
    pub line_1_data: lcd::ScrollData,
    pub line_2_data: lcd::ScrollData,
    pub line_34_data: lcd::ScrollData,
    /// Stores organisation, a vec of startion URLs & whether or not the last track is a ding
    ///pub channel_file_data: get_channel_details::ChannelFileDataDecoded,
    pub position_and_duration: [RealTimeDataOnOneChannel; NUMBER_OF_POSSIBLE_CHANNELS + 1], // +1 so there is a channel to play the startup ding
}

impl PlayerStatus {
    pub fn new(config: &read_config::Config) -> PlayerStatus {
        //let a = core::array::from_fn(|i| i);
        PlayerStatus {
            toml_error: None,
            running_status: lcd::RunningStatus::Startingup,
            channel_number: NUMBER_OF_POSSIBLE_CHANNELS + 2, // an invalid value that cannot match as must be in the range 0 to 100 inclusive (Ding channel is 100)
            position_and_duration: std::array::from_fn(|_index| RealTimeDataOnOneChannel::new()),
            all_4lines: lcd::ScrollData::new("", 4),
            line_1_data: lcd::ScrollData::new("", 1),
            line_2_data: lcd::ScrollData::new("", 1),
            line_34_data: lcd::ScrollData::new("", 2),
            current_volume: config.initial_volume,
            gstreamer_state: gstreamer::State::Null,
            buffering_percent: 0,
            usb_is_mounted: false,
            network_data: NetworkData::new(),
            ping_data: ping::PingData::new(),
        }
    }

    pub fn output_config_information(&self, config: &Config) {
        println!("aural_notifications\t\t{:?}\r", config.aural_notifications);
        println!("buffering_duration\t\t{:?}\r", config.buffering_duration);
        println!("cd_channel_number\t\t{:?}\r", config.cd_channel_number);
        println!(
            "error_recovery_attempt_count_reset_time\t{:?}\r",
            config.error_recovery_attempt_count_reset_time
        );
        println!("initial_volume\t\t\t{}\r", config.initial_volume);
        println!("input_timeout\t\t\t{:?}\r", config.input_timeout);
        println!(
            "max_pause_before_playing\t{:?}\r",
            config.max_pause_before_playing
        );
        println!(
            "maximum_error_recovery_attempts\t{}\r",
            config.maximum_error_recovery_attempts
        );
        println!(
            "aural_notifications\t\t{:?}\r",
            config.pause_before_playing_increment
        );
        println!("scroll\t\t\t\t{:?}\r", config.scroll);
        println!(
            "smart_goto_previous_track_duration\t{:?}\r",
            config.smart_goto_previous_track_duration
        );
        println!("stations_directory\t\t{}\r", config.stations_directory);
        println!(
            "time_initial_message_displayed_after_channel_change_as_ms\t{:?}\r",
            config.time_initial_message_displayed_after_channel_change_as_ms
        );
        println!("usb\t\t\t\t{:?}\r", config.usb);
        println!("volume_offset\t\t\t{}\r", config.volume_offset);
    }

    pub fn output_debug_info(&self) {
        println!("\nstatus of rradio follows\r");
        println!("toml_error\t\t{:?}\r", self.toml_error);
        println!("running_status\t\t{:?}\r", self.running_status);
        println!("channel_number\t\t{}\r", self.channel_number);
        println!("current_volume\t\t{}\r", self.current_volume);
        println!("gstreamer_state\t\t{:?}\r", self.gstreamer_state);
        println!("buffering_percent\t{}\r", self.buffering_percent);
        println!("network_data\t\t{:?}\r", self.network_data);
        println!("usb_is_mounted\t\t{}\r", self.usb_is_mounted);
        println!("ping_data\t\t{:?}\r", self.ping_data);
        println!("all_4lines\t\t{:?}\r", self.all_4lines);
        println!("line_1_data\t\t{:?}\r", self.line_1_data);
        println!("line_2_data\t\t{:?}\r", self.line_2_data);
        println!("line_34_data\t\t{:?}\r", self.line_34_data);

        println!("position_and_duration follow if there are any\r");
        for channel_count in 0..self.position_and_duration.len() {
            if self.position_and_duration[channel_count]
                .duration_ms
                .is_some()
                | (channel_count == self.channel_number)
            {
                println!("channel_count {}\r", channel_count);
                println!(
                    "\tindex_to_current_track\t{}\r",
                    self.position_and_duration[channel_count].index_to_current_track
                );
                println!(
                    "\tposition\t\t{} s\r",
                    self.position_and_duration[channel_count]
                        .position
                        .as_seconds_f32()
                );
                println!(
                    "\tduration_ms\t\t{:?}\r",
                    self.position_and_duration[channel_count].duration_ms
                );
                println!(
                    "\tartist\t\t\t{}\r",
                    self.position_and_duration[channel_count].artist
                );
                println!(
                    "\torganisation\t\t{}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .organisation
                );
                println!(
                    "\tsource_type\t\t{:?}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .source_type
                );

                println!(
                    "\tlast_track_is_a_ding\t{}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .last_track_is_a_ding
                );
                println!("\n\tTrack information follows\r");

                for track_count in 0..self.position_and_duration[channel_count]
                    .channel_data
                    .station_urls
                    .len()
                {
                    println!(
                        "\t{} {}\r",
                        track_count,
                        self.position_and_duration[channel_count]
                            .channel_data
                            .station_urls[track_count]
                    )
                }
            }
        }
    }

    pub fn update_channel_file_data(
        &mut self,
        channel_number: usize,
        new_channel_file_data: ChannelFileDataDecoded,
    ) {
        self.toml_error = None; // clear out the toml error if there is one

        self.running_status = lcd::RunningStatus::RunningNormally;
        self.ping_data.destination_to_ping = if self.position_and_duration[self.channel_number]
            .channel_data
            .source_type
            == get_channel_details::SourceType::UrlList
        {
            ping::PingWhat::Local
        } else {
            ping::PingWhat::Nothing
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
