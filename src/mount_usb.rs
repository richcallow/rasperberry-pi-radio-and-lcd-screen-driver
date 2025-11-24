// mounts USB memory sticks
use super::SourceType;
use crate::get_channel_details::ChannelErrorEvents;
use crate::player_status::PlayerStatus;
use crate::read_config;

/// Mounts local usb stick & sets status_of_rradio.usb_mounted = true
pub fn mount_usb(
    usb_details: &read_config::Usb,
    status_of_rradio: &mut PlayerStatus,
) -> Result<(), ChannelErrorEvents> {
    if status_of_rradio.usb_mounted {
        eprintln!("USB is already mounted\r");
        return Ok(()); // it is already mounted
    }
    let mount_result_as_result = sys_mount::Mount::builder()
        .fstype("vfat") // vfat as we are mounting a local USB memory stick
        .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
        .data("iocharset=utf8,utf8")
        .mount(&usb_details.device, &usb_details.local_mount_folder);
    match mount_result_as_result {
        Ok(_) => {
            status_of_rradio.usb_mounted = true;
            status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .source_type = SourceType::Usb;
            Ok(())
        }
        Err(mount_error) => {
            eprintln!("usb mount error is {:?}\r", mount_error);

            // the value returned by the operating system if there is no device
            const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
            const OS_RESOURCE_BUSY: i32 = 16;
            let mount_error_as_option = mount_error.raw_os_error();

            status_of_rradio.usb_mounted = false; // whatever the previous status was, now we have failed
            match mount_error_as_option {
                Some(OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY) => Err(ChannelErrorEvents::NoUSBDevice),
                Some(OS_RESOURCE_BUSY) => {
                    // as it is already mounted, we do not need to do mount it again
                    status_of_rradio.usb_mounted = true;
                    println!("\r\nUSB already mounted\r");

                    status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                        .channel_data
                        .source_type = SourceType::Usb;
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
