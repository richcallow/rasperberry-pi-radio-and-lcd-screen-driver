//! reads the config.toml file that configures rrr
use std::time::Duration;

use gstreamer::ClockTime;
use string_replace_all::StringReplaceAll;

use crate::{
    get_channel_details::ChannelFileDataDecoded,
    player_status::{PlayerStatus, RealTimeDataOnOneChannel},
};

/// used to convert a TOML string to clock time
///
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

#[derive(Debug, serde::Deserialize)]
#[serde(default)] // if any field is missing, use the value specified in the default
/// Holds all the configuration information read from the TOML configuration file
pub struct Config {
    /// The folder that stores the stations
    pub stations_directory: String, // eg stations_directory = "/boot/playlists2"

    #[serde(with = "humantime_serde")] // this allows us to enter the time for example as          input_timeout = "3s"
    /// The timeout when entering two digit station indices
    pub input_timeout: Duration, // the duration of the keyboard timeout eg input_timeout = "3s"

    /// The change in volume when the user increments or decrements the volume
    pub volume_offset: i32,

    pub initial_volume: i32,

    #[serde(with = "humantime_serde")]
    pub buffering_duration: Option<Duration>,

    #[serde(deserialize_with = "deserialize_clocktime")]
    pub goto_previous_track_time_delta: ClockTime,

    #[serde(deserialize_with = "deserialize_clocktime")]
    pub time_initial_message_displayed_after_channel_change: ClockTime,

    pub max_number_of_remote_pings: u32,

    pub scroll: Scroll, // the parameters that specify how the scroll reacts

    /// Notification sounds
    pub aural_notifications: AuralNotifications,

    /// channel number of the CD drive, eg 00
    pub cd_channel_number: Option<usize>, // in the range 0 to 99 inclusive

    ///details on the local memory stick
    pub usb: Option<MediaDetails>, //details on the local memory stick

    ///details of a memory stick on a Samba share
    pub samba: Option<MediaDetails>,
}

#[derive(Debug, Default, PartialEq, Clone, serde::Deserialize)]
pub struct AuthenticationData {
    pub username: String,
    pub password: String,
}

#[derive(Debug, PartialEq, Clone, serde::Deserialize)]
/// optionally specify in config.toml file if you want a local memory stick to work
/// needs to start with the following so TOML expects the media details.
pub struct MediaDetails {
    //details of a local memory stick or a Samba device
    /// eg channel_number = 88
    pub channel_number: usize,
    /// eg  device = "//192.168.0.2/volume(sda1)"
    pub device: String,
    /// contains username & password
    pub authentication_data: Option<AuthenticationData>,
    /// eg version = "3.0"
    #[serde(alias = "Version")] // allows version to start with upper or Lower V.
    pub version: Option<String>,
    /// Folder where the remote drive will be mounted;
    /// Must not be the same as the folder where the local USB drive is mounted
    pub mount_folder: String,
    /// specifies if the device is mounted
    #[serde(skip, default = "is_mounted_default")]
    // skip means that even if the users specify it as true,
    // the deserializer will skip what they have entered and it will be false.
    pub is_mounted: bool, // the user should not specify this & it must be false on startup
}
/// the default value for is_mounted;
fn is_mounted_default() -> bool {
    false
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
/// the paramaters used by the scroll function
pub struct Scroll {
    pub max_scroll: usize,
    pub min_scroll: usize,
    pub scroll_period_ms: u64,
}

#[derive(Debug, Default, serde::Deserialize)]
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

impl Default for Config {
    fn default() -> Self {
        Self {
            stations_directory: "/boot/playlists3".to_string(),
            input_timeout: Duration::from_secs(3),
            volume_offset: 5,   // step the volum in 5 dB intervals
            initial_volume: 70, // initial volume is 70 dB
            buffering_duration: None,
            //pause_before_playing_increment: Duration::from_secs(1),
            //max_pause_before_playing: Duration::from_secs(5),
            goto_previous_track_time_delta: ClockTime::from_mseconds(2000),
            //maximum_error_recovery_attempts: 5,
            //error_recovery_attempt_count_reset_time: Some(Duration::from_secs(30)),
            time_initial_message_displayed_after_channel_change: ClockTime::from_mseconds(3000),
            scroll: Scroll {
                max_scroll: 14,         // we want to advance at most that many characters
                min_scroll: 6,          //minimum ammount of a scroll
                scroll_period_ms: 1600, //  the time between scrolls in milli-seconds
            },
            aural_notifications: AuralNotifications::default(),
            cd_channel_number: None,
            usb: None,
            samba: None,
            max_number_of_remote_pings: 15,
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
                    .replace("|", " ") // not very meaningful, so turn into spaces
                    .replace("^", " ") // not very meaningful, so turn into spaces
                    .replace_all("  ", " ")
                    .replace_all("  ", " ")
                    .replace_all("  ", " ");

                format!("Using file {config_file_path:?} got {error}")
            });

        //now verify that the specified files exist
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

            if let Some(usb) = &return_value.usb
                && !std::path::Path::new(&usb.mount_folder).exists()
            {
                return Err(format!(
                    "local USB mount folder {} specified in TOML file but not found",
                    usb.mount_folder
                ));
            }
            if let Some(samba) = &return_value.samba {
                if let Some(usb) = &return_value.usb
                    && samba.mount_folder == usb.mount_folder
                {
                    return Err("Mount folder for local & remote USB must be different".to_string());
                }
                if !std::path::Path::new(&samba.mount_folder).exists() {
                    return Err(format!(
                        "Remote USB mount folder {} specified in TOML file but not found",
                        samba.mount_folder
                    ));
                }
            }
        }

        return_value_as_result
    }
}

/// inserts the SAMBA details in the Samaba part of status_of_rradio
pub fn insert_samba(config: &Config, status_of_rraadio: &mut PlayerStatus) {
    if let Some(samba) = &config.samba {
        let samba_clone = samba.clone();
        status_of_rraadio.position_and_duration[samba.channel_number] = RealTimeDataOnOneChannel {
            artist: String::new(),
            address_to_ping: String::new(),
            index_to_current_track: 0,
            duration: None,
            position: ClockTime::ZERO,
            channel_data: ChannelFileDataDecoded {
                organisation: String::new(),
                last_track_is_a_ding: true,
                pause_before_playing_ms: None,
                source_type: crate::get_channel_details::SourceType::Usb,
                station_urls: vec![],
                media_details: Some(MediaDetails {
                    channel_number: samba.channel_number,
                    authentication_data: samba_clone.authentication_data,
                    version: samba_clone.version,
                    device: samba_clone.device,
                    mount_folder: samba_clone.mount_folder,
                    is_mounted: false,
                }),
            },
        }
    }
}

/// inserts the USB details in the USB part of status_of_rradio
pub fn insert_usb(config: &Config, status_of_rraadio: &mut PlayerStatus) {
    if let Some(usb) = &config.usb {
        let usb_clone = usb.clone();
        status_of_rraadio.position_and_duration[usb.channel_number] = RealTimeDataOnOneChannel {
            artist: String::new(),
            address_to_ping: String::new(),
            index_to_current_track: 0,
            duration: None,
            position: ClockTime::ZERO,
            channel_data: ChannelFileDataDecoded {
                organisation: String::new(),
                last_track_is_a_ding: true,
                pause_before_playing_ms: None,
                source_type: crate::get_channel_details::SourceType::Usb,
                station_urls: vec![],
                media_details: Some(MediaDetails {
                    channel_number: usb.channel_number,
                    authentication_data: None,
                    version: None,
                    device: usb_clone.device,
                    mount_folder: usb_clone.mount_folder,
                    is_mounted: false,
                }),
            },
        }
    }
}

/* sample config file

#this file is read at startup
# first log entry affect all modules, except those that explicity have their own level. The levels are in the README

stations_directory = "/home/pi/playlists"
input_timeout = "3s"            # input timeout on the keyboard
volume_offset = 5               # the ammount the volume changes when going up & down
initial_volume = 75
buffering_duration = "20s"
#pause_before_playing_increment = "1s"          # the increment in the pauses before playing when an infinite stream terminates
#max_pause_before_playing  = "10s"                      # maximum value of the pause

goto_previous_track_time_delta = 3500

cd_channel_number = 0

max_number_of_remote_pings = 12

[scroll]
max_scroll = 14         #  maximum ammount of a scroll in charactters
min_scroll = 6          # minimuum ammount of a scroll
scroll_period_ms = 1600 # the time between scrollsin misli-seconds


#[log_level]
#"rradio::audio_pipeline::controller::buffering" = "trace"

[aural_notifications]
filename_startup =  "/home/pi/sounds/KDE-Sys-App-Message.mp3"                   # sound played at startup
filename_error =    "/home/pi/sounds/KDE-Sys-App-Message.mp3"                   # sound played if there is an error
filename_sound_at_end_of_playlist =  "/home/pi/sounds/KDE-Sys-App-Message.mp3"  # beep at end of playlist

[usb]
channel_number = 99
device= "/dev/sda1"
mount_folder = "/home/pi/local_mount_folder"

[samba]
channel_number = 88
device = "//192.168.0.2/volume(sda1)"
version = "1.0"
mount_folder = "/home/pi/88"
[samba.authentication_data]         # omit this entry if no  authentication data
username = "the username"
password = "the password"
*/

/*
simple channel is as follows

organisation = "the name "
station_url = [
"https://etc "
]

or with a pause before playing to fill the buffer

organisation = "thename2"
pause_before_playing_ms = 5000
station_url = [
"http://etc   "
]



playlist is as follows

organisation = "playlist name"
station_url = [
"artist name/disk name",
"artist name2/disk name2",
]

device = "/dev/sda1"

*/
