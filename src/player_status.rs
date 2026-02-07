use std::fs;

use super::PodcastDataAllStations;
use chrono::Utc;
use gstreamer::ClockTime;

use crate::{
    get_channel_details::{self, ChannelFileDataDecoded},
    lcd::{
        self, RunningStatus,
        get_local_ip_address::{self, NetworkData},
        get_mute_state,
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
    pub position: ClockTime,
    /// address_to_ping is derived from the first station in the list
    /// after stripping off the prefix & suffix
    pub address_to_ping: String,
    pub duration: Option<ClockTime>,
    pub channel_data: ChannelFileDataDecoded,
}
impl RealTimeDataOnOneChannel {
    pub fn new() -> Self {
        Self {
            channel_data: get_channel_details::ChannelFileDataDecoded::new(),
            artist: String::new(),
            index_to_current_track: 0,
            position: ClockTime::ZERO,
            duration: None,
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
/// PODCAST_CHANNEL_NUMBER must be less than START_UP_DING_CHANNEL_NUMBER or else we do not get position & duration
pub const PODCAST_CHANNEL_NUMBER: usize = NUMBER_OF_POSSIBLE_CHANNELS;
pub const START_UP_DING_CHANNEL_NUMBER: usize = NUMBER_OF_POSSIBLE_CHANNELS + 1;
#[derive(Debug)] // neither Copy nor clone are implmented as the player can only have a single status
/// A struct listing all information needed to display the status of rradio.
pub struct PlayerStatus {
    pub toml_error: Option<String>,
    /// Specifies if we are starting up, in which case we want to see the startup message, shutting down or running normally.
    /// or there is a bad error
    pub running_status: lcd::RunningStatus,
    /// in the range 00 to 99, normally, but the ding channel is 100
    pub startup_folder: String,
    pub channel_number: usize,
    pub current_volume: i32,
    pub gstreamer_state: gstreamer::State,
    pub buffering_percent: i32,
    pub podcast_data_from_toml: PodcastDataAllStations,
    pub latest_podcast_string: Option<String>,
    /// index_of_podcast, as in which podcast has been selected
    pub podcast_index: i32,
    /// stores SSID, local IP address & gateway address
    pub network_data: get_local_ip_address::NetworkData,
    pub ping_data: ping::PingData,
    pub all_4lines: lcd::ScrollData,
    pub line_1_data: lcd::ScrollData,
    pub line_2_data: lcd::ScrollData,
    pub line_34_data: lcd::ScrollData,
    pub time_started_playing_current_station: chrono::DateTime<Utc>,
    /// Stores channel_file_data, organisation, a vec of startion URLs & whether or not the last track is a ding
    pub position_and_duration: [RealTimeDataOnOneChannel; NUMBER_OF_POSSIBLE_CHANNELS + 2], // +1 so there is a channel to play the startup ding
}

impl PlayerStatus {
    pub fn new(config: &read_config::Config) -> PlayerStatus {
        //let a = core::array::from_fn(|i| i);
        PlayerStatus {
            toml_error: None,
            running_status: lcd::RunningStatus::Startingup,
            startup_folder: String::new(),
            channel_number: NUMBER_OF_POSSIBLE_CHANNELS,
            current_volume: config.initial_volume,
            gstreamer_state: gstreamer::State::Null,
            buffering_percent: 0,
            podcast_data_from_toml: PodcastDataAllStations {
                podcast_data_for_all_stations: Vec::new(),
            },
            latest_podcast_string: None,
            podcast_index: 0, // 0 is the index value of the not-selected value
            network_data: NetworkData::new(),
            ping_data: ping::PingData::new(),
            all_4lines: lcd::ScrollData::new("", 4),
            line_1_data: lcd::ScrollData::new("", 1),
            line_2_data: lcd::ScrollData::new("", 1),
            line_34_data: lcd::ScrollData::new("", 2),
            time_started_playing_current_station: chrono::Utc::now(),
            position_and_duration: std::array::from_fn(|_index| RealTimeDataOnOneChannel::new()),
        }
    }
    /// Initialises for a new station, sets time_started_playing_current_station, RunningStatus::RunningNormally,
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
            "time_initial_message_displayed_after_channel_change\t{}\r",
            config.time_initial_message_displayed_after_channel_change
        );
        println!("usb\t\t\t\t{:?}\r", config.usb);
        println!("samba\t\t\t\t{:?}\r", config.samba);
        println!("volume_offset\t\t\t{}\r", config.volume_offset);
    }

    /// reports whether or not the amplifier is muted & the status information
    pub fn generate_rradio_report(&self) -> Result<String, std::fmt::Error> {
        use std::fmt::Write;
        let mut report = String::new();

        writeln!(report, "\nstatus of rradio follows")?;
        writeln!(
            report,
            "Throttled_status\t{:?}",
            lcd::get_throttled::is_throttled()
        )?;
        writeln!(
            report,
            "Temperature & Wi-Fi\t{}",
            lcd::Lc::get_temperature_and_wifi_strength_text()
        )?;
        writeln!(
            report,
            "mute state is \t\t{}",
            get_mute_state::get_mute_state()
        )?;
        writeln!(report, "toml_error\t\t{:?}", self.toml_error)?;
        writeln!(report, "running_status\t\t{:?}", self.running_status)?;
        writeln!(report, "startup folder\t\t{}", self.startup_folder)?;
        writeln!(report, "channel_number\t\t{}", self.channel_number)?;
        writeln!(report, "current_volume\t\t{}", self.current_volume)?;
        writeln!(
            report,
            "podcast_data_from_toml\t{:?}",
            self.podcast_data_from_toml
        )?;
        writeln!(report, "self.podcast_index\t{}", self.podcast_index)?;
        writeln!(report, "gstreamer_state\t\t{:?}", self.gstreamer_state)?;
        writeln!(report, "buffering_percent\t{}", self.buffering_percent)?;
        writeln!(report, "network_data\t\t{:?}", self.network_data)?;
        writeln!(report, "ping_data\t\t{:?}", self.ping_data)?;
        writeln!(report, "all_4lines\t\t{:?}", self.all_4lines)?;
        writeln!(report, "line_1_data\t\t{:?}", self.line_1_data)?;
        writeln!(report, "line_2_data\t\t{:?}", self.line_2_data)?;
        writeln!(report, "line_34_data\t\t{:?}", self.line_34_data)?;
        writeln!(
            report,
            "time_started_playing_current_station\t{}",
            self.time_started_playing_current_station
        )?;

        writeln!(report, "position_and_duration follow if there are any")?;
        for (channel_count, channel_realtime_data) in self.position_and_duration.iter().enumerate()
        {
            if !channel_realtime_data.channel_data.station_urls.is_empty() {
                writeln!(report, "channel_count {}", channel_count)?;

                writeln!(report, "\tartist\t\t\t{}", channel_realtime_data.artist)?;

                writeln!(
                    report,
                    "\tindex_to_current_track\t{}",
                    channel_realtime_data.index_to_current_track
                )?;

                writeln!(
                    report,
                    "\taddress_to_ping\t\t{}",
                    channel_realtime_data.address_to_ping
                )?;

                writeln!(
                    report,
                    "\tposition\t\t{} s",
                    (channel_realtime_data.position.mseconds() as f32) / 1000.0
                )?;
                writeln!(
                    report,
                    "\tduration\t\t{:?} s",
                    channel_realtime_data
                        .duration
                        .map(|duration| (duration.mseconds() as f32) / 1000.0)
                )?;

                writeln!(
                    report,
                    "\tchannel_data.organisation\t\t{:?}",
                    channel_realtime_data.channel_data.organisation
                )?;
                writeln!(
                    report,
                    "\tchannel_data.source_type\t\t{:?}",
                    channel_realtime_data.channel_data.source_type
                )?;
                writeln!(
                    report,
                    "\tchannel_data.last_track_is_a_ding\t{}",
                    channel_realtime_data.channel_data.last_track_is_a_ding
                )?;
                writeln!(
                    report,
                    "\tchannel_data.pause_before_playing_ms\t{:?}",
                    channel_realtime_data.channel_data.pause_before_playing_ms
                )?;
                writeln!(
                    report,
                    "\tchannel_data.media_details\t\t{:?}",
                    channel_realtime_data.channel_data.media_details
                )?;

                writeln!(report, "\n\tTrack information follows")?;

                for (track_count, station_url) in channel_realtime_data
                    .channel_data
                    .station_urls
                    .iter()
                    .enumerate()
                {
                    writeln!(report, "\t{} {}", track_count, station_url)?;
                }
            }
        }

        Ok(report)
    }

    pub fn output_mount_folder_contents(&self, config: &Config) {
        if let Some(usb) = &config.usb {
            let mount_folder = &usb.mount_folder;
            match fs::read_dir(mount_folder) {
                Ok(audio_files) => {
                    println!("folder {:?}\r", audio_files);
                    for file_as_result in audio_files {
                        println!("file {:?}\r", file_as_result);
                    }
                }
                Err(message) => {
                    eprintln!("Failed to read folder and got {:?}\r", message)
                }
            }
        } else {
            println!("no USB data\r")
        }
        if let Some(samba) = &config.samba {
            let mount_folder = &samba.mount_folder;
            match fs::read_dir(mount_folder) {
                Ok(audio_files) => {
                    println!("folder {:?}\r", audio_files);
                    for file_as_result in audio_files {
                        println!("file {:?}\r", file_as_result);
                    }
                }
                Err(message) => {
                    eprintln!("Failed to read folder and got {:?}\r", message)
                }
            }
        } else {
            println!("no samba data\r")
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
