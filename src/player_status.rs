use chrono::{Duration, Utc};

use crate::{
    get_channel_details::{self, ChannelFileDataDecoded},
    lcd::{
        self,
        get_local_ip_address::{self, NetworkData},
        get_mute_state, RunningStatus,
    },
    ping,
    read_config::{self, Config},
};

#[derive(Debug, Clone)]
/// stores the decoded channel file data, the position of the tracks, ie the time since starting to play it
/// &, if it is a streaming channel, the duration of the channel.
pub struct RealTimeDataOnOneChannel {
    pub artist: String,
    pub index_to_current_track: usize,
    pub position: Duration,
    /// address_to_ping is derived from the first station in the list
    /// after stripping off the prefix & suffix
    pub address_to_ping: String,
    pub duration_ms: Option<u64>,
    pub channel_data: ChannelFileDataDecoded,
}
impl RealTimeDataOnOneChannel {
    pub fn new() -> Self {
        Self {
            channel_data: get_channel_details::ChannelFileDataDecoded::new(),
            artist: String::new(),
            index_to_current_track: 0,
            position: Duration::zero(),
            duration_ms: None,
            address_to_ping: "8.8.8.8".to_string(), // a default value in case we do not find a valid address
        }
    }
}
impl Default for RealTimeDataOnOneChannel {
    fn default() -> Self {
        RealTimeDataOnOneChannel::new()
    }
}

/// The maximum possible as the channel number is 2 decimal digits. (The ding channel 100, so the user cannot enter it.)
pub const NUMBER_OF_POSSIBLE_CHANNELS: usize = 100;
pub const START_UP_DING_CHANNEL_NUMBER: usize = NUMBER_OF_POSSIBLE_CHANNELS;
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

    /// specifies what is mounted, eg local USB, remote USB (eg on server) or nothing
    pub item_mounted: get_channel_details::ItemMounted,
    pub ping_data: ping::PingData,
    pub all_4lines: lcd::ScrollData,
    pub line_1_data: lcd::ScrollData,
    pub line_2_data: lcd::ScrollData,
    pub line_34_data: lcd::ScrollData,
    pub time_started_playing_current_station: chrono::DateTime<Utc>,

    /// Stores channel_file_data, organisation, a vec of startion URLs & whether or not the last track is a ding
    pub position_and_duration: [RealTimeDataOnOneChannel; NUMBER_OF_POSSIBLE_CHANNELS + 1], // +1 so there is a channel to play the startup ding
}

impl PlayerStatus {
    pub fn new(config: &read_config::Config) -> PlayerStatus {
        //let a = core::array::from_fn(|i| i);
        PlayerStatus {
            toml_error: None,
            running_status: lcd::RunningStatus::Startingup,
            channel_number: NUMBER_OF_POSSIBLE_CHANNELS,
            position_and_duration: std::array::from_fn(|_index| RealTimeDataOnOneChannel::new()),
            all_4lines: lcd::ScrollData::new("", 4),
            line_1_data: lcd::ScrollData::new("", 1),
            line_2_data: lcd::ScrollData::new("", 1),
            line_34_data: lcd::ScrollData::new("", 2),
            current_volume: config.initial_volume,
            gstreamer_state: gstreamer::State::Null,
            buffering_percent: 0,
            item_mounted: get_channel_details::ItemMounted::Nothing,
            network_data: NetworkData::new(),
            ping_data: ping::PingData::new(),
            time_started_playing_current_station: chrono::Utc::now(),
        }
    }
    /// initialises for a new station, sets time_started_playing_current_station, RunningStatus::RunningNormally,
    /// number_of_pings_to_this_channel = 0
    pub fn initialise_for_new_station(&mut self) {
        self.time_started_playing_current_station = chrono::Utc::now();
        self.running_status = RunningStatus::RunningNormally;
        self.ping_data.number_of_pings_to_this_channel = 0;
    }

    /// outputs the config file
    pub fn output_config_information(&self, config: &Config) {
        println!(
            "\r\nconfigdata\r\naural_notifications\t\t{:?}\r",
            config.aural_notifications
        );
        println!("buffering_duration\t\t{:?}\r", config.buffering_duration);
        println!("cd_channel_number\t\t{:?}\r", config.cd_channel_number);
        /*println!(
            "error_recovery_attempt_count_reset_time\t{:?}\r",
            config.error_recovery_attempt_count_reset_time
        );*/
        println!("initial_volume\t\t\t{}\r", config.initial_volume);
        println!("input_timeout\t\t\t{:?}\r", config.input_timeout);
        println!(
            "max_number_of_pings_to_a_remote_destination\t{}\r",
            config.max_number_of_remote_pings
        );

        println!("scroll\t\t\t\t{:?}\r", config.scroll);
        println!(
            "goto_previous_track_time_delta\t{:?}\r",
            config.goto_previous_track_time_delta
        );
        println!("stations_directory\t\t{}\r", config.stations_directory);
        println!(
            "time_initial_message_displayed_after_channel_change_as_ms\t{:?}\r",
            config.time_initial_message_displayed_after_channel_change_as_ms
        );
        println!("usb\t\t\t\t{:?}\r", config.usb);
        println!("samba_details\t\t\t{:?}\r", config.samba_details);
        println!("mount_data\t\t\t{:?}\r", config.mount_data);

        println!("volume_offset\t\t\t{}\r", config.volume_offset);
    }

    /// outputs whether or not the amplifier is muted & the status information
    pub fn output_debug_info(&self) {
        println!("\nstatus of rradio follows\r");
        println!(
            "Throttled_status\t{:?}\r",
            lcd::get_throttled::is_throttled()
        );
        println!("mute state is \t\t{}\r", get_mute_state::get_mute_state());
        println!("toml_error\t\t{:?}\r", self.toml_error);
        println!("running_status\t\t{:?}\r", self.running_status);
        println!("channel_number\t\t{}\r", self.channel_number);
        println!("current_volume\t\t{}\r", self.current_volume);
        println!("gstreamer_state\t\t{:?}\r", self.gstreamer_state);
        println!("buffering_percent\t{}\r", self.buffering_percent);
        println!("network_data\t\t{:?}\r", self.network_data);
        println!("item\t\t\t{:?}\r", self.item_mounted);
        println!("ping_data\t\t{:?}\r", self.ping_data);
        println!("all_4lines\t\t{:?}\r", self.all_4lines);
        println!("line_1_data\t\t{:?}\r", self.line_1_data);
        println!("line_2_data\t\t{:?}\r", self.line_2_data);
        println!("line_34_data\t\t{:?}\r", self.line_34_data);
        println!(
            "time_started_playing_current_station\t{}\r",
            self.time_started_playing_current_station
        );

        println!("position_and_duration follow if there are any\r");
        for channel_count in 0..self.position_and_duration.len() {
            if (!self.position_and_duration[channel_count]
                .channel_data
                .station_urls
                .is_empty())
                | (channel_count == self.channel_number)
            {
                println!("channel_count {}\r", channel_count);

                println!(
                    "\tchannel_data.organisation\t\t{:?}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .organisation
                );
                println!(
                    "\tchannel_data.source_type\t\t{:?}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .source_type
                );
                println!(
                    "\tchannel_data.last_track_is_a_ding\t{}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .last_track_is_a_ding
                );
                println!(
                    "\tchannel_data.pause_before_playing_ms\t{:?}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .pause_before_playing_ms
                );
                println!(
                    "\tchannel_data.samba_details\t\t{:?}\r",
                    self.position_and_duration[channel_count]
                        .channel_data
                        .samba_details
                );

                println!(
                    "\tartist\t\t\t{}\r",
                    self.position_and_duration[channel_count].artist
                );

                println!(
                    "\tindex_to_current_track\t{}\r",
                    self.position_and_duration[channel_count].index_to_current_track
                );

                println!(
                    "\taddress_to_ping\t\t{}\r",
                    self.position_and_duration[channel_count].address_to_ping
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
