// mounts USB memory sticks
use super::SourceType;
use crate::get_channel_details::{self, ChannelErrorEvents};
use crate::player_status::PlayerStatus;
pub fn mount_usb2(
    config: &crate::read_config::Config, // the data read from rradio's config.toml
    status_of_rradio: &mut PlayerStatus,
) -> Result<(), ChannelErrorEvents> {
    if let Some(mount_data) = &config.mount_data {
        if status_of_rradio.item_mounted == get_channel_details::ItemMounted::RemoteUsb {
            if let Err(error_message) = crate::unmount_if_needed(config, status_of_rradio) {
                return Err(ChannelErrorEvents::CouldNotUnMountDevice(error_message));
            };
        };
        if let Some(usb) = &mount_data.usb {
            mount_usb(&usb.device, &mount_data.mount_folder, status_of_rradio)
        } else {
            Err(ChannelErrorEvents::NoUSBDeviceSpecifiedInConfigTomlFile)
        }
    } else {
        Err(ChannelErrorEvents::NoMountDeviceSpecifiedInConfigTomlfile)
    }
}

/// Mounts the specfied USB stick in the specified folder; set the mounted status
/// If the USB device is already mounted, does nothing.
pub fn mount_usb(
    device_to_be_mounted: &String, // eg SDA1
    mount_folder: &String,         // eg /home/pi/mount_folder
    status_of_rradio: &mut PlayerStatus,
) -> Result<(), ChannelErrorEvents> {
    if status_of_rradio.item_mounted == get_channel_details::ItemMounted::LocalUsb {
        Ok(()) // it is already mounted, so no need to do anything.
    } else {
        let mount_result_as_result = sys_mount::Mount::builder()
            .fstype("vfat") // vfat as we are mounting a local USB memory stick
            .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
            .data("iocharset=utf8,utf8")
            .mount(device_to_be_mounted, mount_folder);
        match mount_result_as_result {
            Ok(_) => {
                status_of_rradio.item_mounted = get_channel_details::ItemMounted::LocalUsb;
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .channel_data
                    .source_type = SourceType::Usb;
                Ok(())
            }
            Err(mount_error) => {
                eprintln!("mount error is {:?}\r", mount_error);

                // the value returned by the operating system if there is no device
                const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
                const OS_RESOURCE_BUSY: i32 = 16;
                let mount_error_as_option = mount_error.raw_os_error();

                status_of_rradio.item_mounted = get_channel_details::ItemMounted::Nothing; // whatever the previous status was, now we have failed
                match mount_error_as_option {
                    Some(OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY) => {
                        Err(ChannelErrorEvents::NoUSBDevice)
                    }
                    Some(OS_RESOURCE_BUSY) => {
                        // as it is already mounted, we do not need to do mount it again
                        status_of_rradio.item_mounted = get_channel_details::ItemMounted::LocalUsb;
                        status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                            .channel_data
                            .source_type = SourceType::Usb;
                        println!("mounted USB\r");
                        Ok(())
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
}
