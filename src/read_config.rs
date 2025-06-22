//! reads the config.toml file that configures rr3

use std::time::Duration;

#[derive(Debug, serde::Deserialize)]
#[serde(default)] // if any field is missing, use the value specified in the default
pub struct Config {
    /// The folder that stores the stations
    pub stations_directory: String, // eg stations_directory = "/boot/playlists2"

    #[serde(with = "humantime_serde")] // this allows us to enter the time for example as          input_timeout = "3s"
    /// The timeout when entering two digit station indices
    pub input_timeout: Duration, // the duration of the keyboard timeout eg input_timeout = "3s"

    // The change in volume when the user increments or decrements the volume
    pub volume_offset: i32,

    pub initial_volume: i32,

    #[serde(with = "humantime_serde")]
    pub buffering_duration: Option<Duration>,

    #[serde(with = "humantime_serde")]
    pub pause_before_playing_increment: Duration,

    #[serde(with = "humantime_serde")]
    pub max_pause_before_playing: Duration,

    #[serde(with = "humantime_serde")]
    pub smart_goto_previous_track_duration: Duration,

    pub maximum_error_recovery_attempts: usize,

    #[serde(with = "humantime_serde")]
    pub error_recovery_attempt_count_reset_time: Option<Duration>,

    pub scroll: Scroll,

    /// Notification sounds
    pub aural_notifications: AuralNotifications,

    pub cd_channel_number: Option<u8>, // in the range 0 to 99 inclusive

    pub usb: Option<Usb>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Usb {
    /// 2 digit channel number
    pub channel_number: u8, // in the range 0 to 99 inclusive
    /// eg device = "/dev/sda1"
    pub device: String,
    /// Folder where the USB drive is mounted
    pub mount_folder: String,
}

/// the paramaters used by the scroll function
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Scroll {
    pub max_scroll: usize,
    pub min_scroll: usize,
    pub scroll_period_ms: u64,
}

/// Notifications allow rradio to play sounds to notify the user of events
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct AuralNotifications {
    /// Ready for input ie the  ding played when the program starts
    pub filename_startup: Option<String>,

    /// Played before the station track, ie after you have entered 2 digits and before a track is played
    pub playlist_prefix: Option<String>,

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
            pause_before_playing_increment: Duration::from_secs(1),
            max_pause_before_playing: Duration::from_secs(5),
            smart_goto_previous_track_duration: Duration::from_secs(2),
            maximum_error_recovery_attempts: 5,
            error_recovery_attempt_count_reset_time: Some(Duration::from_secs(30)),
            scroll: Scroll {
                max_scroll: 14,
                min_scroll: 6,
                scroll_period_ms: 1600,
            },
            aural_notifications: AuralNotifications::default(),
            cd_channel_number: None,
            usb: None,
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
                    "{} could'nt read {config_file_path:?} Got {toml_file_read_error}",
                    env!("CARGO_PKG_NAME")
                )
            })?;

        let return_value_as_result: Result<Config, String> = toml::from_str(&config_as_string)
            .map_err(|toml_file_parse_error| {
                format!(
                    "{} could'nt parse {config_file_path:?} Got     {toml_file_parse_error}",
                    env!("CARGO_PKG_NAME")
                )
            });
        //now verify that the specified files exist
        if let Ok(return_value) = &return_value_as_result {
            if let Some(filename_startup) = &return_value.aural_notifications.filename_startup {
                if !std::path::Path::new(filename_startup).exists() {
                    return Err(format!(
                        "Startup file {} specified in TOML file but not found",
                        filename_startup
                    ));
                }
            }
        }
        if let Ok(return_value) = &return_value_as_result {
            if let Some(playlist_prefix) = &return_value.aural_notifications.playlist_prefix {
                if !std::path::Path::new(playlist_prefix).exists() {
                    return Err(format!(
                        "playlist prefix file {} specified in TOML file but not found",
                        playlist_prefix
                    ));
                }
            }
        }
        if let Ok(return_value) = &return_value_as_result {
            if let Some(playlistfilename_sound_at_end_of_playlist) = &return_value
                .aural_notifications
                .filename_sound_at_end_of_playlist
            {
                if !std::path::Path::new(playlistfilename_sound_at_end_of_playlist).exists() {
                    return Err(format!(
                        "filename_sound_at_end_of_playlist file {} specified in TOML file but not found",
                        playlistfilename_sound_at_end_of_playlist
                    ));
                }
            }
        }

        return_value_as_result
    }
}

/* sample config file
#this file is read at startup
# first log entry affect all modules, except those that explicity have their own level. The levels are in the README

stations_directory = "/boot/playlists3"
input_timeout = "3s"            # input timeout on the keyboard
volume_offset = 5               # the ammount the volume changes when going up & down
initial_volume = 75
buffering_duration = "20s"
pause_before_playing_increment = "1s"           # the increment in the pauses before playing when an infinite stream terminates
max_pause_before_playing  = "10s"                       # maximum value of the pause

smart_goto_previous_track_duration = "4s"

[scroll]
max_scroll = 14         #  maximum ammount of a scroll in charactters
min_scroll = 6          # minimuum ammount of a scroll
scroll_period_ms = 1600 # the time between scrollsin misli-seconds


[log_level]
"rradio::audio_pipeline::controller::buffering" = "trace"

[ping]
remote_ping_count = 20                                  # number of times the remote server is pinged

[aural_notifications]
filename_startup =  "/boot/sounds/KDE-Sys-App-Message.mp3"                      # sound played at startup
filename_error =    "/boot/sounds/KDE-Sys-App-Message.mp3"                      # sound played if there is an error
filename_sound_at_end_of_playlist =  "/boot/sounds/KDE-Sys-App-Message.mp3"     # beep at end of playlist


cd_channel_number = 0

[usb]
channel_number = 99
device = "/dev/sda1"
mount_folder = "//home//pi//mount_folder"

*/
