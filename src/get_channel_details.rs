//! This file get the details of the channel we are about to play as ChannelFileData
//! It normally picks a random album & then plays all of that; however if a playlist is specified, it selects a random album and then plays it.

use crate::read_config;
use crate::{
    gstreamer_interfaces::PlaybinElement,
    player_status::{PlayerStatus, START_UP_DING_CHANNEL_NUMBER},
};

use crate::{lcd, unmount};
use std::{fs, os::fd::AsRawFd};
use substring::Substring;

use crate::mount_samba;
use crate::mount_usb;

/// The data about channel being played extracted from the TOML file.
/// If there is an error trying to find a channel file, most of these entries will be empty
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)] // the specification of default means that all fields do not have to be specified
pub struct ChannelFileDataFromTOML {
    /// The name of the organisation    eg       organisation = "Tradcan"
    pub organisation: String,
    //allows the buffer to fill before we start playing
    pub pause_before_playing_ms: Option<u64>,
    /// What to play       eg       station_url = "https://dc1.serverse.com/proxy/wiupfvnu?mp=/TradCan\"
    pub station_url: Vec<String>,
    /// typically /dev/sda1 or None
    pub playlist_device: Option<String>,
}

#[derive(Debug, PartialEq, Clone)]
/// enum of the possible media types
pub enum SourceType {
    /// will be unknown if the channel cannot be found.
    UnknownSource,
    /// a list of URLs to play
    UrlList,
    Cd,
    /// we will play random tracks on this USB device
    Usb,
    /// we will play random tracks on this USB device  
    Samba,
}

#[derive(Debug, PartialEq, Clone)]
/// Decoded data sucessfully read from the station channel file, ie organisaton, source_type,
/// if the last track is a ding, pause_before_playing_ms, samba_details & station_urls as a Vec,
pub struct ChannelFileDataDecoded {
    /// The name of the organisation    eg       organisation = "Tradcan"
    pub organisation: String,
    /// The type of the source, such as URL list, CD, USB or unknown
    pub source_type: SourceType,
    /// True if the last entry in URL list is a ding.
    pub last_track_is_a_ding: bool,
    pub pause_before_playing_ms: Option<u64>,
    pub samba_details_all: Option<read_config::SambaDetailsAll>,
    /// What to play       eg       station_url = "https://dc1.serverse.com/proxy/wiupfvnu?mp=/TradCan\"
    pub station_urls: Vec<String>,
}
impl ChannelFileDataDecoded {
    pub fn new() -> Self {
        Self {
            organisation: String::new(),
            station_urls: vec![],
            source_type: SourceType::UnknownSource,
            last_track_is_a_ding: false,
            pause_before_playing_ms: None,
            samba_details_all: None,
        }
    }
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

    /// Could not read the channnels folder (eg \boot\playlists\) that contains all the channel files
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

    /// Could not find the album specifed in the play list, possibly because the wrong memory stick is inserted
    CouldNotFindAlbum(String),

    /// no USB device, but one was requested.
    NoUSBDeviceSpecifiedInConfigTomlFile,

    /// No USBDevice
    NoUSBDevice,

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

    /// could not unmount a USB device or a Samba link
    CouldNotUnMountDevice(String),

    /// probably a bug as there should be files
    NoFilesInArray,
}

impl ChannelErrorEvents {
    /// Given an error enum, returns a string to go on the LCD screen
    pub fn to_lcd_screen(&self) -> String {
        match &self {
            ChannelErrorEvents::CouldNotFindChannelFile => "CouldNotFindChannelFile".to_string(),

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

            ChannelErrorEvents::NoUSBDeviceSpecifiedInConfigTomlFile => {
                "No USB device but one was requested".to_string()
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
            ChannelErrorEvents::CouldNotUnMountDevice(message) => {
                format!("could not unmount USB device or Samba link as {}", message)
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
                "Probalby hit a bug as there were no files in the array".to_string()
            }
        }
    }
}

/// Given the folder that contains the channel files & the channel number as a string.
/// If successful returns the details of the channel as the struct ChannelFileData
/// namely organisation, station_url & sets the source type to be SourceType::UrlList
/// works on both local USB devices & remotly mounted ones, which are expected to be
pub fn get_usb_details(
    config: &read_config::Config, // the data read from rradio's config.toml
    status_of_rradio: &mut PlayerStatus,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    let mount_folder;
    let mount_result;

    if let Some(usb) = &config.usb
        && usb.channel_number == status_of_rradio.channel_number
    {
        mount_folder = &usb.local_mount_folder;
        mount_result = mount_usb::mount_usb(usb, status_of_rradio)
    } else if let Some(samba) = &config.samba
        && samba.channel_number == status_of_rradio.channel_number
    {
        mount_folder = &samba.remote_mount_folder;
        mount_result = mount_samba::mount_samba(status_of_rradio)
    } else {
        return Err(ChannelErrorEvents::CouldNotFindChannelFile);
    }
    match mount_result {
        Ok(_mount_result) => {
            //get an empty list of all the audio CD images on the USB memory stick or samba device
            let mut list_of_audio_album_images = vec![];
            let length_of_mount_folder_path = mount_folder.len();

            match fs::read_dir(mount_folder) {
                Ok(artists) => {
                    for artist_as_result in artists {
                        if let Ok(artist_dir_entry) = artist_as_result {
                            match fs::read_dir(artist_dir_entry.path()) {
                                Ok(albums) => {
                                    for album_as_result in albums {
                                        if let Ok(album_dir_entry) = album_as_result {
                                            if let Ok(file_type) = album_dir_entry.file_type() {
                                                if file_type.is_dir() {
                                                    match fs::read_dir(album_dir_entry.path()) {
                                                        Ok(files) => {
                                                            for file_as_result in files {
                                                                if let Ok(file_entry) =
                                                                    file_as_result
                                                                {
                                                                    let file_name_as_os_string =
                                                                        file_entry.path();
                                                                    let file_name =
                                                                        std::path::Path::new(
                                                                            &file_name_as_os_string,
                                                                        );
                                                                    let file_extension = file_name
                                                                        .extension()
                                                                        .map(|extension| {
                                                                            extension
                                                                                .to_string_lossy()
                                                                                .to_ascii_lowercase(
                                                                                )
                                                                        });

                                                                    if let Some(
                                                                        "mp3" | "wav" | "ogg"
                                                                        | "flac",
                                                                    ) = file_extension.as_deref()
                                                                    {
                                                                        list_of_audio_album_images
                                                                            .push(
                                                                            album_dir_entry
                                                                                .path()
                                                                                .to_string_lossy()
                                                                                .to_string(),
                                                                        );
                                                                        break;
                                                                    }
                                                                } else {
                                                                    return Err(ChannelErrorEvents::USBReadReadError("Failed while searching for audio files in folder".to_string()));
                                                                }
                                                            }
                                                        }
                                                        Err(error_message) => {
                                                            return Err(
                                                                    ChannelErrorEvents::USBReadReadError(
                                                            format!("While searching for music files, got error {:?}",error_message),
                                                                    ),
                                                                );
                                                        }
                                                    }
                                                }
                                            } else {
                                                return Err(ChannelErrorEvents::USBReadReadError("Error when readiing USB; could not get the file type".to_string()));
                                            }
                                        } else {
                                            return Err(ChannelErrorEvents::USBReadReadError(
                                                "Read error When trying to read an album"
                                                    .to_string(),
                                            ));
                                        }
                                    }
                                }
                                Err(error_message) => {
                                    const OS_ERROR_NOT_A_DIRECTORY: i32 = 20; // if the error is not a directory we skip it.
                                    if let Some(OS_ERROR_NOT_A_DIRECTORY) =
                                        error_message.raw_os_error()
                                    {
                                    } else {
                                        return Err(ChannelErrorEvents::USBReadReadError(
                                            format!(
                                            "When trying to get the folder containing the albums got error {}",
                                            error_message
                                        )
                                            .to_string(),
                                        ));
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
                        "When trying to get the folder containing the artists got error {}",
                        error_message
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
                                && let Some(audio_fileqq) = audio_or_other_type_of_file_dir_entry
                                    .path()
                                    .as_os_str()
                                    .to_str()
                            {
                                // got a file not a folder, in the audio files folder. but is it an audio file
                                let file_name_as_os_string =
                                    audio_or_other_type_of_file_dir_entry.path();
                                let file_name = std::path::Path::new(&file_name_as_os_string);
                                let file_extension = file_name.extension().map(|extension| {
                                    extension.to_string_lossy().to_ascii_lowercase()
                                });
                                if let Some("mp3" | "wav" | "ogg" | "flac") =
                                    file_extension.as_deref()
                                {
                                    list_of_wanted_tracks.push(format!("file://{}", audio_fileqq));
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
                &config.aural_notifications.filename_sound_at_end_of_playlist
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
                        .substring(
                            length_of_mount_folder_path + 1,
                            chosen_album.len(),
                        )
                        .to_string(),
                    station_urls: list_of_wanted_tracks,
                    source_type: status_of_rradio.position_and_duration[status_of_rradio.channel_number].channel_data.source_type.clone(),
                    last_track_is_a_ding,
                    pause_before_playing_ms: status_of_rradio.position_and_duration[status_of_rradio.channel_number].channel_data.pause_before_playing_ms,
                    samba_details_all: status_of_rradio.position_and_duration[status_of_rradio.channel_number].channel_data.samba_details_all.clone(),
                    })
        }
        Err(mount_error) => Err(mount_error), // return the error returned by the mount function
    }
}

//#[repr(C)]
#[derive(Debug, Default)]
struct CdToc {
    first_cd_track: u8, // start track
    last_cd_track: u8,  // end track
}

/// If successful returns the details of the channel as the struct ChannelFileData
/// namely organisation (=CD), station_url & sets the source type to be SourceType::CD
pub fn get_cd_details(
    config: &crate::read_config::Config, // the data read from rradio's config.toml
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    let device =
        std::fs::File::open("/dev/cdrom") //dev/cdrom is hard coded as it cannot be anything else
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
    if let Some(filename_sound_at_end_of_playlist) =
        &config.aural_notifications.filename_sound_at_end_of_playlist
    {
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
        station_urls: station_url,
        source_type: SourceType::Cd,
        last_track_is_a_ding,
        pause_before_playing_ms: None,
        samba_details_all: None,
    })
}

/// updates status_of_rradio with the new channel data,
/// unless previous_channel_number == status_of_rradio.channel_number != previous_channel_number AND data has already been got for the channel
pub fn store_channel_details_and_implement_them(
    config: &crate::read_config::Config,
    status_of_rradio: &mut PlayerStatus,
    playbin: &PlaybinElement,
    previous_channel_number: usize,
    lcd: &mut lcd::Lc,
) -> Result<(), ChannelErrorEvents> {
    if (status_of_rradio.channel_number != previous_channel_number)
        && !(status_of_rradio.position_and_duration[status_of_rradio.channel_number]
            .channel_data
            .station_urls
            .is_empty())
    {
        if let Some(usb) = &config.usb
            && status_of_rradio.usb_mounted
            && status_of_rradio.channel_number != usb.channel_number
            && let Err(message) = unmount::unmount_if_needed(
                &usb.local_mount_folder,
                &mut status_of_rradio.usb_mounted,
            )
        {
            return Err(ChannelErrorEvents::CouldNotUnMountDevice(message));
        }
        return Ok(());
    }

    match get_channel_details(config, status_of_rradio) {
        Ok(new_channel_file_data) => {
            status_of_rradio.toml_error = None;

            // next work out address to ping & store it
            let mut source_address = new_channel_file_data.station_urls[0].clone();
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
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .address_to_ping = address_to_ping;
            }
            // set  organisation,  station_urls, source_type,  last_track_is_a_ding
            status_of_rradio.position_and_duration[status_of_rradio.channel_number].channel_data =
                new_channel_file_data;
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
                        .station_urls = vec![format!("file://{ding_filename}")];
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
fn get_channel_details(
    config: &crate::read_config::Config, // the data read from rradio's config.toml
    status_of_rradio: &mut PlayerStatus,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    println!("channel number is {}\r", status_of_rradio.channel_number);
    if let Some(usb) = &config.usb
        && usb.channel_number == status_of_rradio.channel_number
    {
        println!("get_usb_details1\r");
        get_usb_details(
            config,
            &mut *status_of_rradio, // * means an immutable binding, which is a mutable re-borrow
        )
    } else if let Some(samba) = &config.samba
        && samba.channel_number == status_of_rradio.channel_number
    {
        println!("get_usb_details2\r");
        get_usb_details(
            config,
            &mut *status_of_rradio, // * means an immutable binding, which is a mutable re-borrow
        )
    } else if config
        .cd_channel_number
        .as_ref()
        .is_some_and(|cd_channel_number| &status_of_rradio.channel_number == cd_channel_number)
    {
        get_cd_details(config)
    } else {
        let directory_entries_in_playlist_folder = std::fs::read_dir(&config.stations_directory)
            .map_err(
                |read_error| ChannelErrorEvents::CouldNotReadChannelsFolder {
                    channels_folder: config.stations_directory.clone(),
                    error_message: read_error.to_string(),
                },
            )?;

        for directory_entry_in_playlist_folder_as_result in directory_entries_in_playlist_folder {
            // As OK, enumerate all the files in the folder
            let directory_entry_in_playlist_folder = directory_entry_in_playlist_folder_as_result
                .map_err(|the_error| {
                ChannelErrorEvents::ErrorReadingFolderEntry {
                    error_message: the_error.to_string(),
                }
            })?;
            // we have got a valid file name, but does it start with the required 2 digit number
            if directory_entry_in_playlist_folder
                .file_name()
                .to_string_lossy()
                .starts_with(format!("{:0>2}", status_of_rradio.channel_number).as_str())
            {
                // if we get here, it matched & thus we have got the channel file the user wanted
                let channel_file_info =
                    std::fs::read_to_string(directory_entry_in_playlist_folder.path()).map_err(
                        |error_string| ChannelErrorEvents::CouldNotReadChannelFile {
                            error_message: error_string.to_string(),
                            path_to_channel_file: directory_entry_in_playlist_folder
                                .path()
                                .to_string_lossy()
                                .to_string(),
                        },
                    )?;

                let toml_result: Result<ChannelFileDataFromTOML, toml::de::Error> =
                    toml::from_str(channel_file_info.trim_ascii_end());
                let channel_toml_data = toml_result.map_err(|toml_error| {
                    ChannelErrorEvents::CouldNotParseChannelFile {
                        channel_number: status_of_rradio.channel_number,
                        error_message: toml_error.to_string(),
                    }
                })?;

                // at this point, the channel_toml_data is the data from the channel file
                if channel_toml_data.playlist_device.is_some() {
                    return set_up_playlist(channel_toml_data, config, &mut *status_of_rradio);
                // it is a playlist, not a simple USB system
                } else if channel_toml_data.station_url.is_empty() {
                    return Err(ChannelErrorEvents::CouldNotParseChannelFile {
                        channel_number: status_of_rradio.channel_number,
                        error_message: "No URLs etc specified".to_string(),
                    });
                } else {
                    return Ok(ChannelFileDataDecoded {
                        organisation: channel_toml_data.organisation,
                        station_urls: channel_toml_data.station_url,
                        source_type: SourceType::UrlList,
                        last_track_is_a_ding: false,
                        pause_before_playing_ms: channel_toml_data.pause_before_playing_ms,
                        samba_details_all: None,
                    });
                };
            }
        }
        Err(ChannelErrorEvents::CouldNotFindChannelFile)
    }
}

/// Sets up a playlist based on a random choice of the albums specified & then puts all of the tracks of the specfied album into the list of tracks to be played.
/// If specfied in the config TOML file, puts a ding at the end.
fn set_up_playlist(
    toml_data: ChannelFileDataFromTOML,
    config: &crate::read_config::Config, // the data read from rrradio's config.toml
    status_of_rradio: &mut PlayerStatus,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    if let Some(usb_details) = &config.usb {
        match mount_usb::mount_usb(usb_details, status_of_rradio) {
            Ok(_mount_result) => {
                if toml_data.station_url.is_empty() {
                    return Err(ChannelErrorEvents::NoFilesInArray);
                }

                let chosen_album = toml_data.station_url
                    [rand::random_range(0..(toml_data.station_url.len()))]
                .as_str();

                let chosen_album_and_path =
                    format!("{}/{}", usb_details.local_mount_folder, chosen_album);

                match fs::read_dir(&chosen_album_and_path) {
                    Ok(audio_files) => {
                        let mut list_of_audio_album_images = Vec::new();

                        for file_as_result in audio_files {
                            if let Ok(file) = file_as_result {
                                // at this point, the name could be the name of a folder, so next check it is a file
                                if let Ok(file_type) = file.file_type()
                                    && file_type.is_file()
                                {
                                    let file_name_as_os_string = file.file_name();
                                    let file_name = std::path::Path::new(&file_name_as_os_string);
                                    let Some("mp3" | "wav" | "ogg" | "flac") = file_name
                                        .extension()
                                        .map(|extension| extension.to_string_lossy().to_lowercase())
                                        .as_deref()
                                    else {
                                        continue;
                                    };

                                    list_of_audio_album_images
                                        .push(format!("file://{}", file.path().to_string_lossy()));
                                }
                            } else {
                                return Err(ChannelErrorEvents::USBReadReadError(
                                    "Failed while geting audio file entries".to_string(),
                                ));
                            }
                        }
                        let last_track_is_a_ding;
                        // if we get here everything has worked
                        if let Some(filename_sound_at_end_of_playlist) =
                            &config.aural_notifications.filename_sound_at_end_of_playlist
                        {
                            // add a ding if one has been specified at the end of the list of tracks
                            list_of_audio_album_images
                                .push(format!("file://{}", filename_sound_at_end_of_playlist));
                            last_track_is_a_ding = true;
                        } else {
                            last_track_is_a_ding = false;
                        }

                        println!("toml_data{:?}\r", toml_data);

                        Ok(ChannelFileDataDecoded {
                            organisation: format!("{}/{}", chosen_album, toml_data.organisation),
                            station_urls: list_of_audio_album_images,
                            source_type: SourceType::Usb,
                            last_track_is_a_ding,
                            pause_before_playing_ms: toml_data.pause_before_playing_ms,
                            samba_details_all: None,
                        })
                    }
                    Err(error_message) => {
                        if let Some(2) = error_message.raw_os_error() {
                            Err(ChannelErrorEvents::CouldNotFindAlbum(format!(
                                "{chosen_album}. Is the right memory stick in place?"
                            )))
                        } else {
                            Err(ChannelErrorEvents::USBReadReadError(format!(
                                "Whilst getting audio file names in the playlist folder {} got {:?} ",
                                &chosen_album_and_path, error_message
                            )))
                        }
                    }
                }
            }
            Err(mount_error) => Err(mount_error), // return the error returned by the mount function
        }
    } else {
        Err(ChannelErrorEvents::NoUSBDeviceSpecifiedInConfigTomlFile)
    }
}
