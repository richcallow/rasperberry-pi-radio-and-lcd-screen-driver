//! This file get the details of the channel we are about to play as ChannelFileData

use std::{fs, os::fd::AsRawFd};
use substring::Substring;

use crate::player_status::PlayerStatus;
pub mod cd_functions;
mod mount_ext;

/// The data about channel being played extracted from the TOML file.
/// If there is an error trying to find a channel file, most of these entries will be empty
#[derive(Debug, serde::Deserialize)]
pub struct ChannelFileDataFromTOML {
    /// The name of the organisation    eg       organisation = "Tradcan"
    pub organisation: String,
    /// What to play       eg       station_url = "https://dc1.serverse.com/proxy/wiupfvnu?mp=/TradCan\"
    pub station_url: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
/// enum of the pososible media types
pub enum SourceType {
    Unknown,
    UrlList,
    CD,
    Usb,
}

#[derive(Debug, Clone, PartialEq)]
/// decoded data sucessfully read from the station channel file
pub struct ChannelFileDataDecoded {
    /// The name of the organisation    eg       organisation = "Tradcan"
    pub organisation: String,
    /// What to play       eg       station_url = "https://dc1.serverse.com/proxy/wiupfvnu?mp=/TradCan\"
    pub station_url: Vec<String>,
    pub source_type: SourceType,
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
        channel_number: u8,
        error_message: String,
    },

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
}
impl ChannelErrorEvents {
    /// given an error enum, returns  a string to go on the LCD screen
    pub fn to_lcd_screen(&self) -> String {
        match &self {
            ChannelErrorEvents::CouldNotFindChannelFile => "unused".to_string(),
            ChannelErrorEvents::CouldNotParseChannelFile {
                channel_number,
                error_message,
            } => {
                format!("{}, {}", channel_number, error_message)
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
                _ => format!("unexpected CD error {}", error).to_string(),
            },
            ChannelErrorEvents::CouldNotGetNumberOfCDTracks(error) => {
                format!("When getting number of CD tracks, got error {}", error)
            }
        }
    }
}

/// Given the folder that contains the channel files & the channel number as a string.
/// If successful returns the details of the channel as the struct ChannelFileData.
/// namely organisation, station_url (which is type SourceType::UrlList)
/// & setting the source type to be Source_type: SourceType::UrlList
///
pub fn get_usb_details(
    config: &crate::read_config::Config, // the data read from rradio's config.toml
    status_of_rradio: &mut PlayerStatus,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    if let Some(usb_config) = &config.usb {
        match mount_ext::mount(
            &usb_config.device,
            &usb_config.mount_folder,
            status_of_rradio,
        ) {
            Ok(_mount_result) => {
                let mut list_of_audio_cd_images = vec![]; //get an empty list of all the audio CD images on the USB memory stick

                let length_of_mount_folder_path = usb_config.mount_folder.len();

                match fs::read_dir(&usb_config.mount_folder) {
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
                                                                        let filename = format!(
                                                                            "{:?}",
                                                                            file_entry.file_name()
                                                                        )
                                                                        .to_lowercase();
                                                                        let length = filename.len();
                                                                        match filename.substring(
                                                                            length - 5,
                                                                            length - 1,
                                                                        ) {
                                                                            ".mp3" | ".wav"
                                                                            | ".ogg" => {
                                                                                let album_name = format!(
                                                                                    "{:?}",
                                                                                    album_dir_entry
                                                                                        .path()
                                                                                );
                                                                                list_of_audio_cd_images.push(format!("{:?}", album_dir_entry.path())); // the use of (:?} adds unwanted quotes arround the string)
                                                                                break;
                                                                            }
                                                                            _ => {} // if it is not a music file skip it
                                                                        }
                                                                    } else {
                                                                        return Err(ChannelErrorEvents::USBReadReadError("Failed while searching for audio files in folder".to_string()));
                                                                    }
                                                                }
                                                            }
                                                            Err(error_message) => {
                                                                return Err(
                                                                    ChannelErrorEvents::USBReadReadError(
                                                            format!("While searching for music files, got error {:?}",error_message).to_string(),
                                                                    ),
                                                                );
                                                            }
                                                        }
                                                    } /*else {
                                                          println!("skipping as not a folder\r")
                                                      };*/
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

                let chosen_cd_with_quotes = list_of_audio_cd_images
                    [rand::random_range(0..=(list_of_audio_cd_images.len() - 1))]
                .as_str(); // there are unwanted quotes around the string
                let chosen_cd = chosen_cd_with_quotes.substring(1, chosen_cd_with_quotes.len() - 1); // remove the quotes
                let mut list_of_wanted_tracks = vec![]; // list of the tracks that we will return
                match fs::read_dir(chosen_cd) {
                    Ok(audio_files) => {
                        for file_as_result in audio_files {
                            if let Ok(audio_file_dir_entry) = file_as_result {
                                if let Ok(file_type) = audio_file_dir_entry.file_type() {
                                    if file_type.is_file() {
                                        let file_as_option =
                                            audio_file_dir_entry.path().as_os_str().to_str();

                                        if let Some(audio_file) =
                                            audio_file_dir_entry.path().as_os_str().to_str()
                                        // got a valid audio file
                                        {
                                            list_of_wanted_tracks
                                                .push(format!("file://{}", audio_file));
                                            // we do not use {:?} in the format string as that adds unwanted quotes
                                        }
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
                        return Err(ChannelErrorEvents::USBReadReadError(format!(
                            "whilst getting audio file names got {:?}",
                            error_message
                        )))
                    }
                }
                // if we get here everything has worked
                if let Some(filename_sound_at_end_of_playlist) =
                    &config.aural_notifications.filename_sound_at_end_of_playlist
                {
                    // add a ding if one has been specified at the end of the list of tracks
                    list_of_wanted_tracks.push(filename_sound_at_end_of_playlist.to_string());
                }

                Ok(ChannelFileDataDecoded {
                    organisation: chosen_cd // if we remove the first part, we get the singer's name and the album name concatonated together
                        .substring(length_of_mount_folder_path + 1, chosen_cd.len() - 1)
                        .to_string(),
                    station_url: list_of_wanted_tracks,
                    source_type: SourceType::Usb,
                })
            }
            Err(mount_error) => Err(mount_error), // return the error returned by the mount function
        }
    } else {
        Err(ChannelErrorEvents::NoUSBDeviceSpecifiedInConfigTomlFile)
    }
}

#[repr(C)]
#[derive(Debug, Default)]
struct CdToc {
    cdth_trk0: u8, /* start track */
    cdth_trk1: u8, /* end track */
}

pub fn get_cd_details(
    config: &crate::read_config::Config, // the data read from rradio's config.toml
    status_of_rradio: &mut PlayerStatus,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    let device =
        std::fs::File::open("/dev/cdrom") //dev/cdrom is hard coded as it cannot be anything else
            .map_err(|err| ChannelErrorEvents::FailedToOpenCdDrive(err.raw_os_error()))?;

    const CDROM_DRIVE_STATUS: nix::sys::ioctl::ioctl_num_type = 0x5326; /* Get tray position, etc. */
    const CDROM_DISC_STATUS: u64 = 0x5327; /* Get disc type, etc. */
    const CDROMREADTOCHDR: u64 = 0x5305; /* Read TOC header
                                         (struct cdrom_tochdr) */

    //  nix::ioctl_none!(read_status, CDROM_DISC_STATUS,)
    nix::ioctl_none_bad!(read_cd_status, CDROM_DRIVE_STATUS);

    match unsafe { read_cd_status(device.as_raw_fd()) } {
        Ok(4) => {}
        Ok(n) => {}
        Err(error) => {
            let g: i32 = 3; //error.into();
        }
    };

    match unsafe { libc::ioctl(device.as_raw_fd(), CDROM_DRIVE_STATUS, 0) } {
        4 => {} // CDS_DISC_OK
        //0 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(0)), // CDS_NO_INFO
        //1 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(1)), // CDS_NO_DISC
        //2 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(2)), // CDS_TRAY_OPEN
        //3 => return Err(ChannelErrorEvents::FailedtoGetCDdriveStatus(3)), // CDS_DRIVE_NOT_READY
        n => return Err(ChannelErrorEvents::FailedtoGetCDdriveOrDiskStatus(n)),
    };

    match unsafe { libc::ioctl(device.as_raw_fd(), CDROM_DISC_STATUS, 0) } {
        100 => {} // CDS_AUDIO
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

    //let tracks = (toc.cdth_trk0..=toc.cdth_trk1);
    println!("  tracks {:?}   {} \r", toc.cdth_trk0, toc.cdth_trk1);

    status_of_rradio.channel_file_data.source_type = SourceType::CD;
    status_of_rradio.channel_file_data.organisation = "CD".to_string();

    let mut station_url = Vec::new();
    for track_count in toc.cdth_trk0..=toc.cdth_trk1 {
        station_url.push(format!("cdda://{track_count}"));
    }
    // if we get here everything has worked
    if let Some(filename_sound_at_end_of_playlist) =
        &config.aural_notifications.filename_sound_at_end_of_playlist
    {
        // add a ding if one has been specified at the end of the list of tracks
        station_url.push(format!("file://{filename_sound_at_end_of_playlist}"));
    }

    println!("list{:?}\r", station_url);
    Ok(ChannelFileDataDecoded {
        organisation: "CD".to_string(),
        station_url,
        source_type: SourceType::CD,
    })
}

/// Given the folder that contains the channel files & the channel number as a string.
/// If successful returns the details of the channel as the struct ChannelFileData.
/// namely organisation, station_url (which is type SourceType::UrlList) & source_type.
pub fn get_channel_details(
    channels_folder: String, // The folder containing all the channels, eg /boot/playlists3
    channel_number: u8,      //     in the range 0 to 99 inclusive
    config: &crate::read_config::Config, // the data read from rradio's config.toml
    status_of_rradio: &mut PlayerStatus,
) -> Result<ChannelFileDataDecoded, ChannelErrorEvents> {
    if config
        .usb
        .as_ref()
        .is_some_and(|usb_data| channel_number == usb_data.channel_number)
    {
        get_usb_details(
            config,
            &mut *status_of_rradio, // * means an immutable binding, which is a mutable re-borrow
        )
    } else if config
        .cd_channel_number
        .as_ref()
        .is_some_and(|cd_data| &channel_number == cd_data)
    {
        get_cd_details(config, &mut *status_of_rradio)
    } else {
        let directory_entries_in_playlist_folder =
            std::fs::read_dir(&channels_folder).map_err(|read_error| {
                ChannelErrorEvents::CouldNotReadChannelsFolder {
                    channels_folder: channels_folder.clone(),
                    error_message: read_error.to_string(),
                }
            })?;

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
                .starts_with(format!("{:0>2}", channel_number).as_str())
            {
                // if we get here, it matched & thus we have got the channel file the user wanted
                let channel_file_data =
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
                    toml::from_str(channel_file_data.trim_ascii_end());
                let toml_data = toml_result.map_err(|toml_eror| {
                    ChannelErrorEvents::CouldNotParseChannelFile {
                        channel_number,
                        error_message: toml_eror.to_string(),
                    }
                })?;

                println!("toml{:?}\r", toml_data);
                return Ok(ChannelFileDataDecoded {
                    organisation: toml_data.organisation,
                    station_url: toml_data.station_url,
                    source_type: SourceType::UrlList,
                });
            }
        }
        return Err(ChannelErrorEvents::CouldNotFindChannelFile);
    }
}
