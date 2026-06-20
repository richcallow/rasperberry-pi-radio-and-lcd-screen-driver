//! reads the config.toml file that configures rrr
use std::time::Duration;

use chrono::DateTime;
use gstreamer::ClockTime;
use string_replace_all::StringReplaceAll;

use crate::player_status::NUMBER_OF_POSSIBLE_CHANNELS;

/// used to convert a TOML string to clock time
fn deserialize_clocktime<'de, D: serde::Deserializer<'de>>(
    // "de" is, by convention, the name of the lifetime of the input.
    // this function is needed by #[derive(serde::Deserialize)] (called by toml::from_str),
    // which requires #[serde(deserialize_with = "deserialize_clocktime")]
    deserializer: D,
) -> Result<ClockTime, D::Error> {
    humantime_serde::deserialize(deserializer).and_then(|duration: std::time::Duration| {
        ClockTime::try_from(duration).map_err(serde::de::Error::custom)
    })
}

use serde::{Deserialize, Serialize};
#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct StartTime {
    pub time: String,
    pub channel: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(default)] // if any field is missing, use the value specified in the default
/// Holds all the configuration information read from the TOML configuration file
pub struct Config {
    /// The folder that stores the stations
    pub stations_directory: String, // eg stations_directory = "/boot/playlists2"

    /// The timeout when entering two digit station indices
    #[serde(with = "humantime_serde")]
    // this allows us to enter the time for example as          input_timeout = "3s"
    pub input_timeout: Duration, // the duration of the keyboard timeout eg input_timeout = "3s"

    /// The change in volume when the user increments or decrements the volume
    pub volume_offset: i32,

    /// The inital volum ewhen the program starts
    pub initial_volume: i32,

    ///buffer-duration is a configuration property for the playbin element that defines the
    /// maximum amount of media data to buffer in time (measured in nanoseconds) when streaming content over a network
    #[serde(with = "humantime_serde")]
    pub buffer_duration: Option<Duration>,

    /// if the user preses "previous track", within this time, program goes to start of the current track
    /// if longer, to the previous track
    #[serde(deserialize_with = "deserialize_clocktime")]
    pub goto_previous_track_time_delta: ClockTime,

    #[serde(deserialize_with = "deserialize_clocktime")]
    pub time_initial_message_displayed_after_channel_change: ClockTime,

    pub max_number_of_remote_pings: u32,

    /// the parameters that specify how the scroll reacts
    pub scroll: Scroll,

    /// Notification sounds
    pub aural_notifications: AuralNotifications,

    /// list of times when the program automatically starts to play a channel
    pub start_times: Vec<StartTime>,

    ///details on the local memory stick
    //pub usb: Option<UsbConfig>, //details on the local memory stick

    /// the time that the positon will advance (or goback) when the short advance
    /// (or short goback) button is pressed on the web page
    #[serde(skip, default = "short_advance_time_default")]
    pub short_advance_time: i32,

    /// the time that the positon will advance (or goback) when the long advance
    /// (or long goback) button is pressed on the web page
    #[serde(skip, default = "long_advance_time_default")]
    pub long_advance_time: i32,
}

/// the default value for short_advance_time
fn short_advance_time_default() -> i32 {
    10
}

/// the default value for long_advance_time
fn long_advance_time_default() -> i32 {
    60
}

#[derive(Debug, Default, PartialEq, Clone, serde::Deserialize)]
/// Authneticaton data for a Samba share is stored here
pub struct AuthenticationData {
    pub username: String,
    pub password: String,
}

#[derive(Debug, PartialEq, Clone, serde::Deserialize)]
/// needs to start with the following so TOML expects the media details.
pub struct MediaDetails {
    //details of a local memory stick or a Samba device
    /// eg  device = "//192.168.0.2/volume(sda1)" or ""//192.168.0.2" if disk_identifier is specified
    pub device: String,

    /// Name of a file or folder that is on the device to be searched for.
    /// if this is specified, the program will use smbclient to enumerate all the top level
    /// files or folders on the sambra share looking for a match
    pub disk_identifier: Option<String>,
    /// contains username & password
    pub authentication_data: Option<AuthenticationData>,
    /// eg version = "3.0"
    #[serde(alias = "Version")] // allows version to start with upper or Lower V.
    pub version: Option<String>,
    /// Folder where the remote drive will be mounted;
    /// Must not be the same as the folder where the local USB drive is mounted

    #[serde(default = "empty_string")]
    pub mount_folder: String,
    /// specifies if the device is mounted
    #[serde(skip, default = "is_mounted_default")]
    // skip means that even if the users specify it as true,
    // the deserializer will skip what they have entered and it will be false.
    pub is_mounted: bool, // the user should not specify this & it must be false on startup
}
/// the default value for is_mounted
fn is_mounted_default() -> bool {
    false
}

fn empty_string() -> String {
    String::new()
}
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
/// the paramaters used by the scroll function
pub struct Scroll {
    pub max_scroll: usize,
    pub min_scroll: usize,
    pub scroll_period_ms: u64,
}

#[derive(Debug, Default, serde::Deserialize)] // the parameters that specify how the scroll reacts
#[serde(default)]
/// Notifications allows rradio to play sounds to notify the user of events
pub struct AuralNotifications {
    /// Ready for input ie the  ding played when the program starts
    pub filename_startup: Option<String>,

    /// Name of the file played at the end of the list of tracks, ie another ding
    pub filename_sound_at_end_of_playlist: Option<String>,

    /// Name of the file played if there is an error ie the error ding.
    pub filename_error: Option<String>,
}

/// Used when the program cannot find the config.toml file.
impl Default for Config {
    /// Used when the program cannot find the config.toml file.
    fn default() -> Self {
        Self {
            stations_directory: "/home/pi/playlists".to_string(),
            input_timeout: Duration::from_secs(3),
            volume_offset: 5,   // step the volum in 5 dB intervals
            initial_volume: 70, // initial volume is 70 dB
            buffer_duration: None,
            goto_previous_track_time_delta: ClockTime::from_mseconds(2000),
            time_initial_message_displayed_after_channel_change: ClockTime::from_mseconds(3000),
            scroll: Scroll {
                max_scroll: 14,         // we want to advance at most that many characters
                min_scroll: 6,          //minimum ammount of a scroll
                scroll_period_ms: 1600, //  the time between scrolls in milli-seconds
            },
            aural_notifications: AuralNotifications::default(),
            max_number_of_remote_pings: 15,
            short_advance_time: 10,
            long_advance_time: 60,
            start_times: vec![],
        }
    }
}

impl Config {
    /// Given the path to the TOML file used to give the config information returns the configuration information.
    /// returns an error string if it cannot parse the TOML file or
    /// if a file is specified to be played to the user, eg at startup or at the end of a CD or USB stick AND the file is missing.
    pub fn from_file(config_file_path: &str) -> Result<Self, String> {
        let config_as_string =
            std::fs::read_to_string(config_file_path).map_err(|toml_file_read_error| {
                format!(
                    "{} couldn't read {config_file_path:?} Got {toml_file_read_error}",
                    env!("CARGO_PKG_NAME")
                )
            })?;

        let return_value_as_result: Result<Config, String> = toml::from_str(&config_as_string)
            .map_err(|toml_file_parse_error| {
                let error = toml_file_parse_error
                    .to_string()
                    .replace("\n", " ") // cannot handle new lines, so turn into spaces
                    .replace("|", " ") // "|"" are not very meaningful, so turn into spaces
                    .replace("^", " ") // "^" not very meaningful, so turn into spaces
                    .replace_all("  ", " ") // get rid of multiple double spaces
                    .replace_all("  ", " ")
                    .replace_all("  ", " ");

                format!("Using file {config_file_path:?} got {error}\n")
            });

        //now verify that the specified files exist & start times are OK
        if let Ok(return_value) = &return_value_as_result {
            if let Some(filename_startup) = &return_value.aural_notifications.filename_startup
                && !std::path::Path::new(filename_startup).exists()
            {
                return Err(format!(
                    "Startup file {} specified in TOML file but not found",
                    filename_startup
                ));
            }

            if let Some(playlistfilename_sound_at_end_of_playlist) = &return_value
                .aural_notifications
                .filename_sound_at_end_of_playlist
                && !std::path::Path::new(playlistfilename_sound_at_end_of_playlist).exists()
            {
                return Err(format!(
                    "filename_sound_at_end_of_playlist file {} specified in TOML file but not found",
                    playlistfilename_sound_at_end_of_playlist
                ));
            }

            for start_time in &return_value.start_times {
                if let Err(error) =
                    format!("2023-09-19T{}Z", start_time.time).parse::<DateTime<chrono::Utc>>()
                {
                    // the date is arbitrary
                    return Err(format!(
                        "When parsing the start time {} got error {}",
                        start_time.time, error
                    ));
                }

                if let Ok(channel_number) = start_time.channel.parse::<usize>()
                    && channel_number < NUMBER_OF_POSSIBLE_CHANNELS
                {
                } else {
                    return Err(format!("Start channel {} is invalid", start_time.channel));
                }
            }
        }

        return_value_as_result
    }
}
