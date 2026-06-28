use crate::{get_channel_details::ChannelFileDataDecoded, player_status::PlayerStatus};
/// Unmounts whatever device is mounted in the mount folder; returns an error string if it fails
pub fn unmount_if_needed(
    channel_file_data_decoded: &mut ChannelFileDataDecoded,
) -> Result<(), String> {
    if let Some(media_details) = &mut channel_file_data_decoded.media_details
        && media_details.is_mounted
        && !media_details.device.starts_with("/dev/sr")
        && media_details.device != "/dev/cdrom"
    // we do not need to unmount CDs
    {
        println!("unmounting {:?}\r", media_details);
        if let Err(error_message) =
            sys_mount::unmount(&media_details.mount_folder, sys_mount::UnmountFlags::DETACH)
        {
            eprintln!(
                "Failed to unmount the device mounted on {}. Got error {:?}\r",
                media_details.mount_folder, error_message
            );
            return Err(format!(
                "Failed to unmount the device mounted on {}",
                media_details.mount_folder
            ));
        }
        media_details.is_mounted = false; // record that the unmount worked
    }
    Ok(())
}

pub fn unmount_all(status_of_rradio: &mut PlayerStatus) {
    for one_channel in &mut status_of_rradio.position_and_duration {
        let _ = unmount_if_needed(&mut one_channel.channel_data);
    }
}
