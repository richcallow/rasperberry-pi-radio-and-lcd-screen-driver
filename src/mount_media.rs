use crate::get_channel_details::{ChannelErrorEvents, SourceType};
use crate::player_status::PlayerStatus;
use crate::read_config::MediaDetails;

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

    if !matches!(
        status_of_rradio.position_and_duration[status_of_rradio.channel_number]
            .channel_data
            .source_type,
        SourceType::Usb | SourceType::Samba
    ) {
        return Err(ChannelErrorEvents::MediaNotMountabletype);
    }

    mount_media(media_details)
}

/// Mounts a remote memory stick using Samba or CIFS; sets is_mounted = true if successful
pub fn mount_media(media_details: &mut MediaDetails) -> Result<String, ChannelErrorEvents> {
    if media_details.is_mounted {
        println!("Device is already mounted\r");
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
        // for some devices, one must specify the version nmumber
        data_string = format!("{},vers={}", data_string, version) // so this line allows the user to specify the version
    }

    let fstype;
    if media_details.device.starts_with("//") {
        println!("mounting samba\r");
        fstype = "cifs";
        data_string = format!("{},iocharset=utf8", data_string); // add on chracter sets
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
            eprintln!("samba mount error is {:?}\r", mount_error);

            // the value returned by the operating system if there is no device
            const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
            const OS_ERROR_NO_SUCH_DEVICE_OR_ADDRESS: i32 = 6;
            const OS_RESOURCE_BUSY: i32 = 16;
            let mount_error_as_option = mount_error.raw_os_error();

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
