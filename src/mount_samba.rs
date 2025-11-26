use super::SourceType;
use crate::get_channel_details::{ChannelErrorEvents};
use crate::player_status::{PlayerStatus};

/// mounts a remote memory stick using Samba & sets status_of_rradio.samba_mounted = true
pub fn mount_samba(status_of_rradio: &mut PlayerStatus,
) -> Result<(), ChannelErrorEvents> {
        if status_of_rradio.samba_mounted {
            println!("Samba is already mounted\r");
            return Ok(()); // it is already mounted
        }
    
        if let Some (samba_data) = 
                &status_of_rradio.position_and_duration[status_of_rradio.channel_number].channel_data.samba_details_all{

        let mut data_string;
        if let Some (authentication_data) = &samba_data.authentication_data{
            data_string = format!(
            "user={},pass={}",
            authentication_data.username, authentication_data.password)
        }
        else {data_string = String::new()}
        if let Some(version) = &samba_data.version {
            // for some devices, one must specify the version nmumber
            data_string = format!("{},vers={}", data_string, version) // so this line allows the user to specify the version
        }

        data_string = format!("{},iocharset=utf8", data_string); // add on chracter sets
                                                                    //.data("iocharset=utf8,utf8")
        let mount_result_as_result = sys_mount::Mount::builder()
            .fstype("cifs") // cifs as were are mounting a SAMBA drive
            .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
            .data(&data_string)
            .mount(&samba_data.device, &samba_data.remote_mount_folder);
        match mount_result_as_result {
            Ok(_) => {
                status_of_rradio.samba_mounted = true;
                Ok(())
            }

            Err(mount_error) => {
                eprintln!("samba mount error is {:?}\r", mount_error);

                // the value returned by the operating system if there is no device
                const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
                const OS_RESOURCE_BUSY: i32 = 16;
                let mount_error_as_option = mount_error.raw_os_error();

                status_of_rradio.samba_mounted = false; // whatever the previous status was, now we have failed
                match mount_error_as_option {
                    Some(OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY) => {
                        Err(ChannelErrorEvents::NoUSBDevice)
                    }
                    Some(OS_RESOURCE_BUSY) => {
                        // as it is already mounted, we do not need to do mount it again
                        println!("samba already mounted\r");
                        status_of_rradio.samba_mounted = true;
                        status_of_rradio.position_and_duration
                            [status_of_rradio.channel_number]
                            .channel_data
                            .source_type = SourceType::Samba;
                             Ok(())
                    }
                    Some(error_number) => {
                        Err(ChannelErrorEvents::UsbMountMountError(format!(
                            "Got Operating System error {} ",
                            error_number
                        )))
                    }
                    None => {
                        Err(ChannelErrorEvents::UsbMountMountError(
                            mount_error.kind().to_string(),
                        ))
                    }                  
                }
            }
        }
    }
    else  { 
        Err(ChannelErrorEvents::NoUSBDeviceSpecifiedInConfigTomlFile)
    }
}
