use crate::get_channel_details::{ChannelErrorEvents, SourceType};
use crate::my_dbg;
use crate::player_status::PlayerStatus;
use crate::read_config::MediaDetails;
use std::fs;

/// Mounts media if the media is a type that is mountable.
/// Returns the mount folder if the mount is successful.
pub fn mount_media_for_current_channel(
    status_of_rradio: &mut PlayerStatus,
) -> Result<String, ChannelErrorEvents> {
    let Some(media_details) = &mut status_of_rradio.position_and_duration
        [status_of_rradio.channel_number]
        .channel_data
        .media_details
    else {
        return Ok(String::new()); // Err(ChannelErrorEvents::MediaNotSpecifiedInTomlfile)
    };

    if status_of_rradio.position_and_duration[status_of_rradio.channel_number]
        .channel_data
        .source_type
        != SourceType::Usb
    {
        return Err(ChannelErrorEvents::MediaNotMountabletype(
            status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .source_type
                .clone(),
        ));
    }
    mount_media(media_details)
}

/// Mounts a remote memory stick using Samba or CIFS; sets is_mounted = true if successful
/// & returns the mount folder if the mount is successful.
pub fn mount_media(media_details: &mut MediaDetails) -> Result<String, ChannelErrorEvents> {
    if media_details.is_mounted {
        println!("Device is already mounted {:?}\r", media_details);
        return Ok(media_details.mount_folder.clone()); // it is already mounted
    }

    let mut data_string;
    if let Some(authentication_data) = &media_details.authentication_data {
        data_string = format!(
            "user={},pass={}",
            authentication_data.username, authentication_data.password
        )
    } else {
        data_string = String::new()
    }
    if let Some(version) = &media_details.version {
        // for some devices, one must specify the version number
        data_string = format!("{},vers={}", data_string, version) // so this line allows the user to specify the version
    }

    let fstype;
    if media_details.device.starts_with("//") {
        println!("mounting samba\r");
        fstype = "cifs";
        data_string = format!("{},iocharset=utf8", data_string); // add on chracter sets

        // check to see
        if let Some(_disk_identifer) = &media_details.disk_identifier {
            return mount_exact_drive_unknown(media_details);
        };
    } else {
        println!("mounting local mem stick\r");
        fstype = "vfat";
        data_string = format!("{},iocharset=utf8,utf8", data_string); // add on chracter sets
    }
    let mount_result_as_result = sys_mount::Mount::builder()
        .fstype(fstype)
        .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
        .data(&data_string)
        .mount(&media_details.device, &media_details.mount_folder);
    match mount_result_as_result {
        Ok(_) => {
            media_details.is_mounted = true;
            Ok(media_details.mount_folder.clone())
        }

        Err(mount_error) => {
            eprintln!("Samba mount error is {:?}\r", mount_error);

            // the value returned by the operating system if there is no device
            const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
            const OS_ERROR_NO_SUCH_DEVICE_OR_ADDRESS: i32 = 6;
            const OS_RESOURCE_BUSY: i32 = 16;
            let mount_error_as_option = mount_error.raw_os_error();
            my_dbg!("calling zz");
            media_details.is_mounted = false; // whatever the previous status was, now we have failed
            match mount_error_as_option {
                Some(OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY) => Err(ChannelErrorEvents::NoUSBDevice),
                Some(OS_ERROR_NO_SUCH_DEVICE_OR_ADDRESS) => Err(
                    ChannelErrorEvents::NoSuchDeviceOrDirectory(media_details.device.clone()),
                ),
                Some(OS_RESOURCE_BUSY) => {
                    // as it is already mounted, we do not need to do mount it again
                    println!("media already mounted\r");
                    media_details.is_mounted = true;

                    Ok(media_details.mount_folder.clone())
                }
                Some(error_number) => Err(ChannelErrorEvents::UsbMountMountError(format!(
                    "Got Operating System error {} ",
                    error_number
                ))),
                None => Err(ChannelErrorEvents::UsbMountMountError(
                    mount_error.kind().to_string(),
                )),
            }
        }
    }
}

/// Mounts a Samba drive by enumerating all the shares at the given IP address
/// Chooses the share where media_details.disk_identifier matches the one specified in
fn mount_exact_drive_unknown(
    media_details: &mut MediaDetails,
) -> Result<String, ChannelErrorEvents> {
    // enumerate the Samba shares using the smbclient command.
    // the format depends on whether or not a password is supplied
    let samba_command_as_result = if let Some(auth_data) = &media_details.authentication_data {
        std::process::Command::new("/bin/smbclient")
            .args([
                "-L",                  // IP address is the next parameter
                &media_details.device, // the IP address of the Samba share
                "-g",                  // sets the output format to be one we expect (easier to machine parse)
                "-U", // Username & password about to follow, separated by the "%" character
                format!("{}%{}", auth_data.username, auth_data.password).as_str(),
            ])
            .output()
    } else {
        std::process::Command::new("/bin/smbclient")
            .args(["-N", "-L", &media_details.device, "-g"])
            .output() // -N in previous line means no username and password 
    };
    match samba_command_as_result {
        Ok(output) => {
            if output.status.success() {
                let return_string = format!("{}", String::from_utf8_lossy(&output.stdout));
                let output_as_a_vec_of_lines: Vec<&str> = return_string.split("\n").collect();
                let mut local_media_details = media_details.clone();
                for one_output_line in output_as_a_vec_of_lines {
                    if one_output_line.starts_with("Disk") {
                        // we have found a Samba share
                        let new_device = format!(
                            "{}{}",
                            media_details.device,
                            &one_output_line
                                ["Disk|".len()..one_output_line.len() - 1]
                        );
                        // at this point, we have found a mountable Samba drive, but we do not know if it is the correct one
                        local_media_details.device = new_device;
                        local_media_details.disk_identifier = None;
                        match mount_media(&mut local_media_details) {
                            Ok(mount_folder) => match fs::read_dir(&mount_folder) {
                                Ok(read_dir) => {
                                    if let Some(disk_identifier) = &media_details.disk_identifier {
                                        for folder_as_result in read_dir {
                                            if let Ok(folder) = folder_as_result
                                                && disk_identifier
                                                    == &folder
                                                        .file_name()
                                                        .to_string_lossy()
                                                        .to_string()
                                            {
                                                my_dbg!(&mount_folder);
                                                media_details.is_mounted = true;
                                                return Ok(mount_folder);
                                            }
                                        }
                                    }
                                    // if we get here, the share we looked at was not the wanted one
                                    // so unmount it
                                    if let Err(error) = sys_mount::unmount(
                                        mount_folder.as_str(),
                                        sys_mount::UnmountFlags::DETACH,
                                    ) {
                                        return Err(ChannelErrorEvents::UsbMountMountError(
                                            format!("Got unmount error {}", error),
                                        ));
                                    }
                                    local_media_details.is_mounted = false;
                                }
                                Err(error) => {
                                    return Err(ChannelErrorEvents::ErrorReadingFolderEntry {
                                        error_message: format!(
                                            "failed to read {}. got {}",
                                            mount_folder, error
                                        ),
                                    });
                                }
                            },
                            Err(error) => {
                                return Err(error);
                            }
                        }
                    }
                }
                Err(ChannelErrorEvents::CouldNotFindSambaShareWithFolder(
                    media_details.disk_identifier.clone(),
                ))
            } else {
                my_dbg!(output);
                Err(ChannelErrorEvents::CouldNotEnumerateSamba(
                    "Got error when enumerating Samba clients".to_string(),
                ))
            }
        }
        Err(error) => Err(ChannelErrorEvents::CouldNotEnumerateSamba(
            error.to_string(),
        )),
    }
}
