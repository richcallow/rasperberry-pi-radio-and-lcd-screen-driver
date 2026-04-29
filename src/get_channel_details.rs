//! This file get the details of the channel we are about to play as ChannelFileData
//! It normally picks a random album & then plays all of that; however if a playlist is specified, it selects a random album and then plays it.

use crate::gstreamer_interfaces::unmount_usb;
use crate::read_config::AuralNotifications;
use crate::read_config::{self, MediaDetails};
use crate::{
    gstreamer_interfaces::PlaybinElement,
    player_status::{PlayerStatus, START_UP_DING_CHANNEL_NUMBER},
};

use crate::{lcd, my_dbg};
use std::{fs, os::fd::AsRawFd};
use substring::Substring;
use gstreamer::ClockTime;
use crate::mount_media::{self};

fn station_url_default() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, PartialEq, Clone, serde::Deserialize)]
/// enum of the possible media types
pub enum SourceType {
    /// will be unknown if the channel cannot be found.
    UnknownSource,
    /// a list of URLs to play
    UrlList,
    Cd,
    /// we will play random tracks on this local or remote USB device
    Usb,
}
impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SourceType::Cd => write!(f, "CD"),
            SourceType::Usb => write!(f, "USB"),
            SourceType::UrlList => write!(f, "URL"),
            Self::UnknownSource => write!(f, "Source type is unknown; programming error"),
        }
    }
}

pub const LIST_OF_SUPPORTED_FILE_TYPES: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a"];

fn is_supported_file_type(path: &std::path::Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| LIST_OF_SUPPORTED_FILE_TYPES.contains(&extension.as_str()))
}

#[derive(Debug, PartialEq, Clone, serde::Deserialize)]
/// Decoded data sucessfully read from the station channel file, ie organisaton, source_type,
/// if the last track is a ding, pause_before_playing_ms, media_details & station_urls as a Vec,
pub struct ChannelFileDataDecoded {
    /// The name of the organisation    eg       organisation = "Tradcan"
    #[serde(default = "organisation")]
    pub organisation: String,

    /// The type of the source, such as URL list, CD, USB or unknown
    #[serde(skip, default = "default_source_type")]
    pub source_type: SourceType,

    /// True if the last entry in URL list is a ding.
    #[serde(skip, default = "is_false")]
    pub last_track_is_a_ding: bool,
    pub pause_before_playing_ms: Option<u64>,

    /// True if the last entry in URL list is a ding.
    #[serde(default = "is_false")]
    pub random_tracks_wanted: bool,

    /// true if the channel data has been initialised
    #[serde(skip, default = "is_false")]
    pub data_is_initialised: bool,

    pub media_details: Option<read_config::MediaDetails>,
    /// What to play       eg       station_url = "https://dc1.serverse.com/proxy/wiupfvnu?mp=/TradCan\"
    #[serde(default = "station_url_default")]
    /// What to play    eg  station_url = "https://dc1.serverse.com/proxy/wiupfvnu?mp=/TradCan\"
    pub station_url: Vec<String>,
}
impl ChannelFileDataDecoded {
    pub fn new() -> Self {
        Self {
            organisation: String::new(),
            station_url: vec![],
            source_type: SourceType::UnknownSource,
            last_track_is_a_ding: false,
            pause_before_playing_ms: None,
            media_details: None,
            random_tracks_wanted: false,
            data_is_initialised: false,
        }
    }
}
/// the default value for organisation
fn organisation() -> String {
    String::new()
}

fn is_false() -> bool {
    false
}

// the default value for unknown sources
fn default_source_type() -> SourceType {
    SourceType::UnknownSource
}

impl Default for ChannelFileDataDecoded {
    fn default() -> Self {
        ChannelFileDataDecoded::new()
    }
}
/// an enum of errors returned by get_channel_details
#[derive(Debug)]
pub enum ChannelErrorEvents {
    /// The message returned if the user enters a channel number that does not exist
    CouldNotFindChannelFile,

    /// When enumerating the Samba files, could not find a folder or file with the specified name
    CouldNotFindSambaShareWithFolder(Option<String>),

    /// Could not read the channels folder (eg \boot\playlists\) that contains all the channel files
    CouldNotReadChannelsFolder {
        channels_folder: String,
        error_message: String,
    },

    /// Got an error reading the folder entry
    ErrorReadingFolderEntry { error_message: String },

    /// For some reason we found the channel file, but could not read it.
    CouldNotReadChannelFile {
        path_to_channel_file: String,
        error_message: String,
    },

    /// We read the channel file, but could not parse it
    CouldNotParseChannelFile {
        channel_number: usize,
        error_message: String,
    },

    /// Could not enumerate the Samba device
    CouldNotEnumerateSamba(String),

    /// Could not find the album specifed in the play list, possibly because the wrong memory stick is inserted
    CouldNotFindAlbum(String),

    /// No USBDevice
    NoUSBDevice,

    /// is the problem that the SAMBA device has the wrong letter paattern associated with it eg sdb1, not sda1
    NoSuchDeviceOrDirectory(String),

    /// USB mount error other than no USB device;
    /// the string contains the reason return by the Operating System
    UsbMountMountError(String),

    /// Error when trying to read a USB memory stick
    USBReadReadError(String),

    /// failed to open the CD drive, whe ndrying to get the file descriptor
    FailedToOpenCdDrive(Option<i32>),

    /// failed to get the drive or disk details
    FailedtoGetCDdriveOrDiskStatus(i32),

    /// could not get the number of tracks on the CD
    CouldNotGetNumberOfCDTracks(i32),

    /// probably a bug as there should be files
    NoFilesInArray,
}

impl ChannelErrorEvents {
    /// Given an error enum, returns a string to go on the LCD screen
    pub fn to_lcd_screen(&self) -> String {
        match &self {
            ChannelErrorEvents::CouldNotFindChannelFile => "CouldNotFindChannelFile".to_string(),
            ChannelErrorEvents::CouldNotFindSambaShareWithFolder(folder_name) => {
                if let Some(error_message) = folder_name {
                    format!(
                        "When enumerating the Samba shares could not find folder/file {}",
                        error_message
                    )
                } else {
                    "Software bug in error message".to_string()
                }
            }
            ChannelErrorEvents::CouldNotParseChannelFile {
                channel_number,
                error_message,
            } => {
                format!("{}, {}", channel_number, error_message)
            }
            ChannelErrorEvents::CouldNotFindAlbum(album_name) => {
                format!("Could not find {album_name}")
            }
            ChannelErrorEvents::CouldNotReadChannelFile {
                path_to_channel_file,
                error_message,
            } => {
                format!(
                    "Could not read channel file {}; got error {}",
                    path_to_channel_file, error_message
                )
            }
            ChannelErrorEvents::CouldNotEnumerateSamba(error_message) => {
                format!("Could not enumerate Samba {}", error_message)
            }
            ChannelErrorEvents::NoSuchDeviceOrDirectory(bad_path) => {
                format!("Could not find device on path{}", bad_path)
            }
            ChannelErrorEvents::CouldNotReadChannelsFolder {
                channels_folder,
                error_message,
            } => {
                format!(
                    "Could not read channels folder {}; got error {}",
                    channels_folder, error_message
                )
            }
            ChannelErrorEvents::ErrorReadingFolderEntry { error_message } => {
                format!("Error reading channel folder entry {}", error_message)
            }
            ChannelErrorEvents::NoUSBDevice => "No USB device found".to_string(),
            ChannelErrorEvents::UsbMountMountError(error_message) => {
                format!("When trying to mount a USB device got error {error_message}")
            }
            ChannelErrorEvents::USBReadReadError(error_message) => {
                format!(
                    "When trying to read USB memory stick got error {}",
                    error_message
                )
            }
            ChannelErrorEvents::FailedToOpenCdDrive(error_as_option) => {
                if let Some(error) = error_as_option {
                    match error {
                        2 => "No CD drive".to_string(),
                        123 => "No CD in drive".to_string(),
                        _ => format!("CD Open error {error}"),
                    }
                } else {
                    format!("CD open error {:?}", error_as_option)
                }
            }

            ChannelErrorEvents::FailedtoGetCDdriveOrDiskStatus(error) => match error {
                &0 => "No info on CD in drive".to_string(), //CDS_NO_INFO
                &1 => "no CD in drive.".to_string(),        // CDS_NO_DISC
                &2 => "CD drive tray open".to_string(),     //
                &3 => "CD drive not ready".to_string(),     //
                &101 | &102 | &103 | &104 => "Data CD no audio".to_string(), // CDS_DATA_1 = 101
                //&102 => "CdIsData2)".to_string(),                   // CDS_DATA_2
                //&103 => "CdIsXA21)".to_string(),                    // CDS_XA_2_1
                //&104 => "CdIsXA22)".to_string(),                    // CDS_XA_2_2
                //&105 => tracing::debug!("Mixed CD"),         // CDS_MIXED
                -1 => "bad CD error from OS".to_string(),
                _ => format!("unexpected CD error {}", error).to_string(),
            },
            ChannelErrorEvents::CouldNotGetNumberOfCDTracks(error) => {
                format!("When getting number of CD tracks, got error {}", error)
            }
            ChannelErrorEvents::NoFilesInArray => {
                "Probably hit a bug as there were no files in the array".to_string()
            }
        }
    }
}

/// Given the folder that contains the channel files & the channel number as a string.
/// If successful returns the details of the channel as the struct ChannelFileData
/// namely organisation, station_url & sets the source type to be SourceType::UrlList
/// works on both local USB devices & remotely mounted ones,
/// which are expected to have different mount folders
pub fn get_channel_details_from_mountable_media(
    aural_notifications: &AuralNotifications, // taken from config.toml
    channel_file_data_decoded: &mut ChannelFileDataDecoded,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    let mount_folder = mount_media::mount_media_for_current_channel(channel_file_data_decoded)?;
    if channel_file_data_decoded.random_tracks_wanted {
        return set_up_playlist_random_albums(
            mount_folder,
            &aural_notifications.filename_sound_at_end_of_playlist,
            channel_file_data_decoded,
        );
    }

    //get an empty list of all the audio CD images on the USB memory stick or Samba device
    let mut list_of_audio_album_images = Vec::new();

    match fs::read_dir(&mount_folder) {
        Ok(artists) => {
            for artist_as_result in artists {
                if let Ok(artist_dir_entry) = artist_as_result {
                    match fs::read_dir(artist_dir_entry.path()) {
                        Ok(albums) => {
                            for album_as_result in albums {
                                let album_dir_entry = album_as_result.map_err(|_error| {
                                    ChannelErrorEvents::USBReadReadError(
                                        "Read error When trying to read an album".to_string(),
                                    )
                                })?;

                                if !album_dir_entry.path().is_dir() {
                                    continue; /* do not execute the rest of the for loop this time round */
                                }
                                let files =
                                    fs::read_dir(album_dir_entry.path()).map_err(|error| {
                                        ChannelErrorEvents::USBReadReadError(format!(
                                            "While searching for music files, got error {}",
                                            error
                                        ))
                                    })?;
                                for dir_entry_as_result in files {
                                    let dir_entry = dir_entry_as_result.map_err(|_error| {
                                        ChannelErrorEvents::USBReadReadError(
                                            "Failed while searching for audio files in folder"
                                                .to_string(),
                                        )
                                    })?;

                                    if is_supported_file_type(dir_entry.file_name().as_ref()) {
                                        list_of_audio_album_images.push(
                                            album_dir_entry.path().to_string_lossy().to_string(),
                                        );
                                        break;
                                    }
                                }
                            }
                        }
                        Err(error_message) => {
                            const OS_ERROR_NOT_A_DIRECTORY: i32 = 20; // if the error is "not a directory" we skip it.
                            if error_message.raw_os_error() != Some(OS_ERROR_NOT_A_DIRECTORY) {
                                return Err(ChannelErrorEvents::USBReadReadError(format!(
                                    "When trying to get the folder containing the albums got error {}",
                                    error_message
                                )));
                            }
                        }
                    }
                } else {
                    return Err(ChannelErrorEvents::USBReadReadError(
                        "When trying to get the list of artists got error".to_string(),
                    ));
                }
            }
        }
        Err(error_message) => {
            return Err(ChannelErrorEvents::USBReadReadError(format!(
                "When trying to get the folder {} containing the artists got error {}",
                mount_folder, error_message
            )));
        }
    }

    if list_of_audio_album_images.is_empty() {
        return Err(ChannelErrorEvents::NoFilesInArray);
    }

    let chosen_album = list_of_audio_album_images
        [rand::random_range(0..=(list_of_audio_album_images.len() - 1))]
    .as_str();
    let mut list_of_wanted_tracks = vec![]; // list of the tracks that we will return
    match fs::read_dir(chosen_album) {
        Ok(audio_files) => {
            for file_as_result in audio_files {
                if let Ok(audio_or_other_type_of_file_dir_entry) = file_as_result {
                    if let Ok(file_type) = audio_or_other_type_of_file_dir_entry.file_type()
                        && file_type.is_file()
                        && let Some(one_audio_file) = audio_or_other_type_of_file_dir_entry
                            .path()
                            .as_os_str()
                            .to_str()
                    {
                        // got a file not a folder, in the audio files folder. but is it an audio file
                        if is_supported_file_type(
                            audio_or_other_type_of_file_dir_entry.file_name().as_ref(),
                        ) {
                            list_of_wanted_tracks.push(format!("file://{}", one_audio_file));
                            // we do not use {:?} in the format string as that adds unwanted quotes
                        }
                    }
                } else {
                    return Err(ChannelErrorEvents::USBReadReadError(
                        "Failed while geting audio file entries".to_string(),
                    ));
                }
            }
        }
        Err(error_message) => {
            if let Some(2) = error_message.raw_os_error() {
                return Err(ChannelErrorEvents::CouldNotFindAlbum(format!(
                    "whilst getting audio file names, could not find album {}",
                    chosen_album
                )));
            }

            return Err(ChannelErrorEvents::USBReadReadError(format!(
                "whilst getting audio file names got {:?}",
                error_message
            )));
        }
    }
    let last_track_is_a_ding;
    // if we get here everything has worked
    if let Some(filename_sound_at_end_of_playlist) =
        &aural_notifications.filename_sound_at_end_of_playlist
    {
        // add a ding if one has been specified at the end of the list of tracks
        list_of_wanted_tracks.push(format!("file://{}", filename_sound_at_end_of_playlist));
        last_track_is_a_ding = true;
    } else {
        last_track_is_a_ding = false;
    }

    Ok(ChannelFileDataDecoded {
        organisation: chosen_album // if we remove the first part, we get the singer's name
            // and the album name concatonated together
            .substring(mount_folder.len() + 1, chosen_album.len())
            .to_string(),
        station_url: list_of_wanted_tracks,
        source_type: channel_file_data_decoded.source_type.clone(),
        data_is_initialised: false,
        last_track_is_a_ding,
        random_tracks_wanted: channel_file_data_decoded.random_tracks_wanted,
        pause_before_playing_ms: channel_file_data_decoded.pause_before_playing_ms,
        media_details: channel_file_data_decoded.media_details.clone(),
    })
}

//#[repr(C)]
#[derive(Debug, Default)]
struct CdToc {
    first_cd_track: u8, // start track
    last_cd_track: u8,  // end track
}

// If successful returns the details of the channel as the struct ChannelFileData
/// namely organisation (=CD), station_url & sets the source type to be SourceType::CD
pub fn play_cd(
    media_details: &MediaDetails, // eg /dev/sr0 or /dev/cdrom
    filename_sound_at_end_of_playlist: &Option<String>,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    let device = std::fs::File::open(media_details.device.clone())
        .map_err(|err| ChannelErrorEvents::FailedToOpenCdDrive(err.raw_os_error()))?;

    const CDROM_DRIVE_STATUS: nix::sys::ioctl::ioctl_num_type = 0x5326; /* Get tray position, etc. */
    const CDROM_DISC_STATUS: u64 = 0x5327; /* Get disc type, etc. */
    const CDROMREADTOCHDR: u64 = 0x5305; /* Read TOC header
    (struct cdrom_tochdr) */

    // first see if the CD drive is working OK & has a disk it it
    match unsafe { libc::ioctl(device.as_raw_fd(), CDROM_DRIVE_STATUS) } {
        4 => {} // CDS_DISC_OK
        //0 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(0)), // CDS_NO_INFO
        //1 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(1)), // CDS_NO_DISC
        //2 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(2)), // CDS_TRAY_OPEN
        //3 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(3)), // CDS_DRIVE_NOT_READY
        n => return Err(ChannelErrorEvents::FailedtoGetCDdriveOrDiskStatus(n)),
    };

    // & having checked that there is a disk in a CD drive, check that it contains a audio tracks
    match unsafe { libc::ioctl(device.as_raw_fd(), CDROM_DISC_STATUS) } {
        100 => {} // CDS_AUDIO; the normal case
        // 0 => return Err(CdError::NoCdInfo),         // CDS_NO_INFO
        // 1 => return Err(CdError::NoCd),             // CDS_NO_DISC
        // 2 => return Err(CdError::CdTrayIsOpen),     // CDS_TRAY_OPEN
        // 3 => return Err(CdError::CdTrayIsNotReady), // CDS_DRIVE_NOT_READY
        // 101 => return Err(CdError::CdIsData1),      // CDS_DATA_1
        // 102 => return Err(CdError::CdIsData2),      // CDS_DATA_2
        // 103 => return Err(CdError::CdIsXA21),       // CDS_XA_2_1
        // 104 => return Err(CdError::CdIsXA22),       // CDS_XA_2_2
        105 => println!("Mixed CD\r"), // CDS_MIXED
        n => return Err(ChannelErrorEvents::FailedtoGetCDdriveOrDiskStatus(n)),
    }
    let mut toc = CdToc::default();
    let result = unsafe { libc::ioctl(device.as_raw_fd(), CDROMREADTOCHDR, &mut toc) };
    match result {
        0 => {} // 0 is the Ok result
        _ => return Err(ChannelErrorEvents::CouldNotGetNumberOfCDTracks(result)),
    };

    let mut station_url = Vec::new();

    for track_count in toc.first_cd_track..=toc.last_cd_track {
        // the = sign means use last_cd_track  & not stop just beforehand
        station_url.push(format!("cdda://{track_count}"));
    }
    // if we get here everything has worked, so work out if we need to add a ding if one has been specified at the end of the list of tracks.
    let last_track_is_a_ding;
    if let Some(filename_sound_at_end_of_playlist) = filename_sound_at_end_of_playlist {
        if !station_url.is_empty() {
            // only put a ding if we have found at least one track
            station_url.push(format!("file://{filename_sound_at_end_of_playlist}"));
            last_track_is_a_ding = true;
        } else {
            last_track_is_a_ding = false;
        }
    } else {
        last_track_is_a_ding = false;
    }

    Ok(ChannelFileDataDecoded {
        organisation: "CD".to_string(),
        station_url,
        source_type: SourceType::Cd,
        last_track_is_a_ding,
        pause_before_playing_ms: None,
        random_tracks_wanted: false,
        media_details: Some(MediaDetails {
            device: media_details.device.clone(),
            disk_identifier: None,
            authentication_data: None,
            version: None,
            mount_folder: media_details.mount_folder.clone(),
            is_mounted: true,
        }),
        data_is_initialised: false,
    })
}

/// Given a URL (starting with http) & optionally a port number it extracts the station address.
/// Given an IP address, it returns the IP address unchanged.
pub fn get_ip_address(url: String) -> String {
    let mut source_address = url.clone();
    if let Some(position_double_slash) = source_address.find("//") {
        let mut address_to_ping = source_address
            .split_off(position_double_slash + 2)
            .to_string(); // we add +2 to split after the //
        // next if there is a suffix, we must remove it
        if let Some(position_first_single_slash) = address_to_ping.find('/') {
            let _suffix = address_to_ping.split_off(position_first_single_slash);
        } // else there was no suffix so do nothing;

        // but there might be a port number that we have to remove too
        if let Some(position_of_colon) = address_to_ping.find(':') {
            let _ = address_to_ping.split_off(position_of_colon);
        }
        return address_to_ping;
    }
    url
}

/// Updates status_of_rradio with the new channel data,
/// if status_of_rradio.channel_number == previous_channel_number OR no data has been got yet for the channel
pub fn store_channel_details_and_implement_them(
    config: &crate::read_config::Config,
    status_of_rradio: &mut PlayerStatus,
    playbin: &PlaybinElement,
    previous_channel_number: usize,
    lcd: &mut lcd::Lc,
) -> Result<(), ChannelErrorEvents> {
    my_dbg!(&previous_channel_number);
    my_dbg!(&status_of_rradio.channel_number);

    if status_of_rradio.channel_number != previous_channel_number
        && status_of_rradio.position_and_duration[previous_channel_number]
            .channel_data
            .data_is_initialised
    {
        //no need to do anything as there is data & user wants to return to the previous settngs
        return Ok(());
    }
    // Either the user wants a new search, or this is the first time & there is no data.

    status_of_rradio.initialise_for_new_station();
    status_of_rradio.position_and_duration[status_of_rradio.channel_number].position =
        ClockTime::ZERO;

    if let Err(error) =
        unmount_usb(&mut status_of_rradio.position_and_duration[previous_channel_number])
    {
        status_of_rradio
            .all_4lines
            .update_if_changed(error.as_str());
    }

    match get_channel_details(config, status_of_rradio.channel_number) {
        Ok(new_channel_file_data) => {
            status_of_rradio.position_and_duration[status_of_rradio.channel_number].channel_data =
                new_channel_file_data.clone();
            status_of_rradio.toml_error = None;
            if !status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .station_url
                .is_empty()
            {
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .address_to_ping = get_ip_address(new_channel_file_data.station_url[0].clone());
            }

            if status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .source_type
                == SourceType::Usb
            {
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .channel_data = get_channel_details_from_mountable_media(
                    &config.aural_notifications,
                    &mut status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                        .channel_data,
                )?;
            }

            Ok(())
        }
        Err(get_channel_details_error) => {
            if let ChannelErrorEvents::CouldNotFindChannelFile = get_channel_details_error {
                if (status_of_rradio.running_status == lcd::RunningStatus::NoChannel)
                    && (status_of_rradio.channel_number == previous_channel_number)
                {
                    status_of_rradio.toml_error = None; // clear the TOML error out, the user must have seen it by now
                    status_of_rradio.running_status = lcd::RunningStatus::NoChannelRepeated;
                } else {
                    status_of_rradio.running_status = lcd::RunningStatus::NoChannel;
                }
                if let Some(ding_filename) = &config.aural_notifications.filename_error {
                    // play a ding if one has been specified
                    status_of_rradio.position_and_duration[START_UP_DING_CHANNEL_NUMBER]
                        .channel_data
                        .station_url = vec![format!("file://{ding_filename}")];
                    status_of_rradio.position_and_duration[START_UP_DING_CHANNEL_NUMBER]
                        .index_to_current_track = 0;
                    let _ignore_error_if_beep_fails =
                        playbin.play_track(status_of_rradio, config, lcd, false);
                    status_of_rradio.position_and_duration[START_UP_DING_CHANNEL_NUMBER]
                        .index_to_current_track = 0;
                }
            } else {
                status_of_rradio
                    .all_4lines
                    .update_if_changed(get_channel_details_error.to_lcd_screen().as_str());
                status_of_rradio.running_status = lcd::RunningStatus::LongMessageOnAll4Lines;
            };
            Err(get_channel_details_error)
        }
    }
}

/// Given the TOML data.
/// If successful returns the details of the channel as the struct ChannelFileData.
/// namely organisation, station_url (which is type SourceType::UrlList) & .
/// it uses status_of_rradio to know which channel file to look for
/// if it is a playlist, it returns a list of albums to play, not tracks
/// if it is a CD drive, it plays it
fn get_channel_details(
    config: &crate::read_config::Config, // the data read from rradio's config.toml
    status_of_rradio_channel_number: usize,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    // we need to see if there is channel file with this number
    match std::fs::read_dir(&config.stations_directory) {
        Ok(directory_entries_in_playlist_folder) => {
            for directory_entry_in_playlist_folder_as_result in directory_entries_in_playlist_folder
            {
                match directory_entry_in_playlist_folder_as_result {
                    Ok(directory_entry_in_playlist_folder) => {
                        // As OK, enumerate all the files in the folder

                        if directory_entry_in_playlist_folder
                            .file_name()
                            .to_string_lossy()
                            .starts_with(
                                format!("{:0>2}", status_of_rradio_channel_number).as_str(),
                            )
                        {
                            // if we get here, it matched & thus we have got the channel file the user wanted
                            let channel_file_info =
                                std::fs::read_to_string(directory_entry_in_playlist_folder.path())
                                    .map_err(|error_string| {
                                        ChannelErrorEvents::CouldNotReadChannelFile {
                                            error_message: error_string.to_string(),
                                            path_to_channel_file:
                                                directory_entry_in_playlist_folder
                                                    .path()
                                                    .to_string_lossy()
                                                    .to_string(),
                                        }
                                    })?;

                            let toml_result: Result<ChannelFileDataDecoded, toml::de::Error> =
                                toml::from_str(channel_file_info.trim_ascii_end());
                            // next work out the type of media
                            match toml_result.clone() {
                                Ok(mut channel_file_data_decoded) => {
                                    if let Some(media_details) =
                                        channel_file_data_decoded.media_details.clone()
                                    {
                                        if media_details.device.starts_with("/dev/sd")
                                            || media_details.device.starts_with("//")
                                            || media_details.disk_identifier.is_some()
                                        {
                                            channel_file_data_decoded.source_type = SourceType::Usb;
                                        } else if media_details.device.starts_with("/dev/sr")
                                            || media_details.device.starts_with("/dev/cdrom")
                                        {
                                            channel_file_data_decoded.source_type = SourceType::Cd;

                                            return play_cd(
                                                &media_details,
                                                &config
                                                    .aural_notifications
                                                    .filename_sound_at_end_of_playlist,
                                            );
                                        }
                                    } else {
                                        channel_file_data_decoded.source_type = SourceType::UrlList;
                                    }

                                    channel_file_data_decoded.last_track_is_a_ding = config
                                        .aural_notifications
                                        .filename_sound_at_end_of_playlist
                                        .is_some();

                                    channel_file_data_decoded.data_is_initialised = true;

                                    return Ok(channel_file_data_decoded);
                                }
                                Err(error) => {
                                    return Err(ChannelErrorEvents::CouldNotParseChannelFile {
                                        channel_number: status_of_rradio_channel_number,
                                        error_message: error.to_string(),
                                    });
                                }
                            }
                        }
                    }

                    Err(error) => {
                        return Err(ChannelErrorEvents::CouldNotParseChannelFile {
                            channel_number: status_of_rradio_channel_number,
                            error_message: error.to_string(),
                        });
                    }
                }
            }
        }
        Err(error) => {
            return Err(ChannelErrorEvents::CouldNotParseChannelFile {
                channel_number: status_of_rradio_channel_number,
                error_message: error.to_string(),
            });
        }
    }

    Err(ChannelErrorEvents::CouldNotFindChannelFile)
}

/// As the albums are not specified, sets up playlist based on a random choice of tracks from all the albums found
/// If specfied in the config TOML file, puts a ding at the end.
fn set_up_playlist_random_albums(
    mount_folder: String,
    filename_sound_at_end_of_playlist_as_option: &Option<String>,
    channel_data_for_wanted_channel: &mut ChannelFileDataDecoded,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    let mut track_list = Vec::new();

    let mut number_of_artists = 0;
    match fs::read_dir(&mount_folder) {
        Ok(artists) => {
            for artist_as_dir_entry_result in artists {
                number_of_artists += 1;
                match artist_as_dir_entry_result {
                    Ok(artist_as_dir_entry) => {
                        if let Ok(artist_file_type) = artist_as_dir_entry.file_type()
                            && artist_file_type.is_dir()
                        // we have found what seems to be an artist folder
                        {
                            let artist = artist_as_dir_entry.path().to_string_lossy().to_string();
                            match fs::read_dir(&artist) {
                                Ok(albums) => {
                                    for album_as_dir_entry_result in albums {
                                        match album_as_dir_entry_result {
                                            Ok(album_as_dir_entry) => {
                                                if let Ok(album_file_type) =
                                                    album_as_dir_entry.file_type()
                                                    && album_file_type.is_dir()
                                                {
                                                    //now we have what seems to be an album folder
                                                    let album = album_as_dir_entry
                                                        .path()
                                                        .to_string_lossy()
                                                        .to_string();
                                                    let tracks = fs::read_dir(album);
                                                    match tracks {
                                                        Ok(tracks_as_dir_entry) => {
                                                            for track_as_dir_entry_result in
                                                                tracks_as_dir_entry
                                                            {
                                                                match track_as_dir_entry_result {
                                                                    Ok(track_as_dir_entry) => {
                                                                        if let Ok(track_file_type) =
                                                                            track_as_dir_entry.file_type()
                                                                            && track_file_type.is_file()
                                                                            && is_supported_file_type(
                                                                                &track_as_dir_entry.path(),
                                                                            )
                                                                        {
                                                                            // we have found a supported audio track
                                                                            let track = track_as_dir_entry
                                                                                .path()
                                                                                .to_string_lossy()
                                                                                .to_string();
                                                                            track_list.push(format!(
                                                                                "file://{track}"
                                                                            ));
                                                                        }
                                                                        else {
                                                                            // do not add it is it not an audio track
                                                                        }
                                                                    }
                                                                    Err(error) => eprintln!(
                                                                        "whilst enumerating the tracks for random tracks got error {}",
                                                                        error
                                                                    ),
                                                                }
                                                            }
                                                        }
                                                        Err(error) => {
                                                            return Err(
                                                ChannelErrorEvents::CouldNotReadChannelsFolder {
                                                    channels_folder: artist,
                                                    error_message: error.to_string(),
                                                },
                                            );
                                                        }
                                                    }
                                                }
                                            }
                                            Err(error) => eprintln!(
                                                "whilst enumerating the albums for random tracks got error {}",
                                                error
                                            ),
                                        }
                                    }
                                }
                                Err(error) => {
                                    return Err(ChannelErrorEvents::CouldNotReadChannelsFolder {
                                        channels_folder: artist,
                                        error_message: error.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    Err(error) => eprintln!(
                        "whilst enumerating the artists for random tracks got error {}",
                        error
                    ),
                }
            }
        }
        Err(error_message) => {
            return Err(ChannelErrorEvents::CouldNotReadChannelsFolder {
                channels_folder: mount_folder,
                error_message: error_message.to_string(),
            });
        }
    }
    //return Ok(ChannelFileDataDecoded { organisation: (), source_type: (), last_track_is_a_ding: (), pause_before_playing_ms: (), media_details: (), station_urls: () });
    println!(
        "before random sort got {} artists & {} tracks\r",
        number_of_artists,
        track_list.len() + 1
    );

    use rand::seq::SliceRandom;
    let mut rng = rand::rng();
    track_list.shuffle(&mut rng);

    let last_track_is_a_ding;
    // if we get here everything has worked
    if let Some(filename_sound_at_end_of_playlist) = &filename_sound_at_end_of_playlist_as_option {
        // add a ding if one has been specified at the end of the list of tracks
        track_list.push(format!("file://{}", filename_sound_at_end_of_playlist));
        last_track_is_a_ding = true;
    } else {
        last_track_is_a_ding = false;
    }

    println!(
        "got {} artists & {} tracks\r",
        number_of_artists,
        track_list.len() + 1
    );
    Ok(ChannelFileDataDecoded {
        organisation: channel_data_for_wanted_channel.organisation.clone(),
        source_type: channel_data_for_wanted_channel.source_type.clone(),
        last_track_is_a_ding,
        media_details: channel_data_for_wanted_channel.media_details.clone(),
        random_tracks_wanted: channel_data_for_wanted_channel.random_tracks_wanted,
        pause_before_playing_ms: channel_data_for_wanted_channel.pause_before_playing_ms,
        station_url: track_list,
        data_is_initialised: false,
    })
}
