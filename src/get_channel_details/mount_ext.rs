// mounts USB memory sticks
use crate::get_channel_details::ChannelErrorEvents;
use crate::player_status::PlayerStatus;

use super::SourceType;

/// Mounts the specfied USB stick in the specified folder; set the mounted status in status_of_rradio.usb_is_mounted.
/// If the USB device is already mounted, does nothing.
pub fn mount(
    device_to_be_mounted: &String, // eg SDA1
    mount_folder: &String,         // eg /home/pi/mount_folder
    status_of_rradio: &mut PlayerStatus,
) -> Result<(), ChannelErrorEvents> {
    if status_of_rradio.usb_is_mounted {
        Ok(()) // it is already mounted, so no need to do anything.
    } else {
        let mount_result_as_result = sys_mount::Mount::builder()
            .fstype("vfat")
            .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
            .data("iocharset=utf8,utf8")
            .mount(device_to_be_mounted, mount_folder);
        match mount_result_as_result {
            Ok(_) => {
                status_of_rradio.usb_is_mounted = true;
                status_of_rradio.channel_file_data.source_type = SourceType::Usb;
                Ok(())
            }
            Err(mount_error) => {
                println!("mount error is {:?}\r", mount_error);

                // the value returned by the operating system if there is no device
                const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
                const OS_RESOURCE_BUSY: i32 = 16;
                let mount_error_as_option = mount_error.raw_os_error();

                status_of_rradio.usb_is_mounted = false; // whatever the previous status was, now we have failed
                match mount_error_as_option {
                    Some(OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY) => {
                        Err(ChannelErrorEvents::NoUSBDevice)
                    }
                    Some(OS_RESOURCE_BUSY) => {
                        // as it is already mounted, we do not need to do mount it again
                        status_of_rradio.usb_is_mounted = true;
                        status_of_rradio.channel_file_data.source_type = SourceType::Usb;
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
