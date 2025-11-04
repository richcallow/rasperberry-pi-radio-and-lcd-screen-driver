use super::SourceType;
use crate::get_channel_details::{self, ChannelErrorEvents, SambaDetails};
use crate::player_status::PlayerStatus;

pub fn mount_samba(
    config: &crate::read_config::Config, // eg /home/pi/mount_folder
    status_of_rradio: &mut PlayerStatus,
) -> Result<(), ChannelErrorEvents> {
    if let Some(mount_data) = &config.mount_data {
        if status_of_rradio.item_mounted == get_channel_details::ItemMounted::RemoteUsb {
            return Ok(()); // it is already mounted
        }

        if let Some(samba_details) = mount_data.samba_details.clone() {
            if status_of_rradio.item_mounted == get_channel_details::ItemMounted::LocalUsb {
                if let Err(error_message) = crate::unmount_if_needed(config, status_of_rradio) {
                    return Err(ChannelErrorEvents::CouldNotUnMountDevice(error_message));
                };
            };

            let mut data_string = format!(
                "user={},pass={}",
                samba_details.username, samba_details.password
            );
            if let Some(version) = &samba_details.version {
                // for some devices, one must specify the version nmumber
                data_string = format!("{},vers={}", data_string, version) // so this line allows the user to specify the version
            }

            data_string = format!("{},iocharset=utf8", data_string); // add on chracter sets
                                                                     //.data("iocharset=utf8,utf8")
            let mount_result_as_result = sys_mount::Mount::builder()
                .fstype("cifs") // cifs as were are mounting a SAMBA drive
                .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
                .data(&data_string)
                .mount(&samba_details.device, &mount_data.mount_folder);
            match mount_result_as_result {
                Ok(_) => {
                    status_of_rradio.item_mounted = get_channel_details::ItemMounted::RemoteUsb;
                    status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                        .channel_data
                        .source_type = SourceType::Samba;
                    status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                        .channel_data
                        .samba_details = Some(SambaDetails {
                        device: samba_details.device,
                        password: samba_details.password,
                        username: samba_details.username,
                        version: samba_details.version,
                    });

                    return Ok(());
                }

                Err(mount_error) => {
                    eprintln!("samba mount error is {:?}\r", mount_error);

                    // the value returned by the operating system if there is no device
                    const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
                    const OS_RESOURCE_BUSY: i32 = 16;
                    let mount_error_as_option = mount_error.raw_os_error();

                    status_of_rradio.item_mounted = get_channel_details::ItemMounted::Nothing; // whatever the previous status was, now we have failed
                    match mount_error_as_option {
                        Some(OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY) => {
                            return Err(ChannelErrorEvents::NoUSBDevice)
                        }
                        Some(OS_RESOURCE_BUSY) => {
                            // as it is already mounted, we do not need to do mount it again
                            status_of_rradio.item_mounted =
                                get_channel_details::ItemMounted::RemoteUsb;
                            status_of_rradio.position_and_duration
                                [status_of_rradio.channel_number]
                                .channel_data
                                .source_type = SourceType::Samba;
                            println!("mounting samba\r");
                            return Ok(());
                        }
                        Some(error_number) => {
                            return Err(ChannelErrorEvents::UsbMountMountError(format!(
                                "Got Operating System error {} ",
                                error_number
                            )))
                        }
                        None => {
                            return Err(ChannelErrorEvents::UsbMountMountError(
                                mount_error.kind().to_string(),
                            ))
                        }
                    }
                }
            }
        }
        Ok(())
    } else {
        Err(ChannelErrorEvents::MountDataMustBeSpecifiedInTomlFile)
    }
}
