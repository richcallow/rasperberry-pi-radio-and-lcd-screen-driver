use crate::{get_channel_details, lcd, read_config};

#[derive(Debug, Clone, Copy)]
pub struct PositionAndDuration {
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
}

const HIGHEST_POSSIBLE_CHANNEL_NUMBER: usize = 100; // the maximum possible as the channel number is 2 decimal digits
#[derive(Debug)] // neither Copy nor clone are implmented as the player can only have a single status
pub struct PlayerStatus {
    pub toml_error: Option<String>,
    pub running_status: lcd::RunningStatus,
    pub artist: String,
    pub album: String, // title of the song
    pub position_and_duration: [PositionAndDuration; HIGHEST_POSSIBLE_CHANNEL_NUMBER],
    pub channel_number: u8,            // in the range 00 to 99
    pub previous_channel_number: u8,   // in the range 00 to 99
    pub index_to_current_track: usize, //this specifies which of the tracks we are currently playing on a USB stick/CD or which station if there are multiple stations with the same channel number
    pub current_volume: i32,
    pub gstreamer_state: gstreamer::State,
    pub all_4lines: lcd::ScrollData,
    pub line_2_data: lcd::ScrollData,
    pub line_34_data: lcd::ScrollData,
    pub channel_file_data: get_channel_details::ChannelFileDataDecoded,
    pub buffering_percent: i32,
    pub usb_is_mounted: bool,
}
impl PlayerStatus {
    /*/// Initialises organisation, album, channel_number, artist, current_station
    pub fn initialise_for_new_station(&mut self) {
        self.channel_file_data.organisation = String::new();
        self.album = String::new();
        self.channel_number = 0;
        self.artist = String::new();
        self.current_station = 0;
    }*/
    pub fn new(config: &read_config::Config) -> PlayerStatus {
        use crate::SourceType;
        PlayerStatus {
            toml_error: None,
            running_status: lcd::RunningStatus::Startingup,
            artist: String::new(),
            channel_number: 101, // an invalid value that cannot match
            previous_channel_number: 101,
            album: String::new(),
            position_and_duration: [PositionAndDuration {
                position_ms: 0,
                duration_ms: None,
            }; HIGHEST_POSSIBLE_CHANNEL_NUMBER],
            all_4lines: lcd::ScrollData::new("", 4),
            line_2_data: lcd::ScrollData::new("", 1),
            line_34_data: lcd::ScrollData::new("", 2),
            channel_file_data: get_channel_details::ChannelFileDataDecoded {
                organisation: String::new(),
                station_url: vec![],
                source_type: SourceType::Unknown,
            },
            index_to_current_track: 0,
            current_volume: config.initial_volume,
            gstreamer_state: gstreamer::State::Null,
            buffering_percent: 0,
            usb_is_mounted: false,
        }
    }
}
