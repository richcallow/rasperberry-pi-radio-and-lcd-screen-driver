use crate::player_status::{PlayerStatus, RealTimeDataOnOneChannel};
/// Unmounts whatever device is mounted in the mount folder; returns an error string if it fails
pub fn unmount_if_needed(
    real_time_data_one_channel: &mut RealTimeDataOnOneChannel,
) -> Result<(), String> {
    if let Some(media_details) = &mut real_time_data_one_channel.channel_data.media_details {
        if !media_details.is_mounted
            || media_details.device.starts_with("/dev/sr") // no need to unmount a CD drive
            || media_details.device == "/dev/cdrom"
        {
            return Ok(());
        }
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
        } else {
            media_details.is_mounted = false;
        };

        Ok(())
    } else {
        Ok(())
    }
}

pub fn unmount_all(status_of_rradio: &mut PlayerStatus) {
    for one_channel in &mut status_of_rradio.position_and_duration {
        let _ = unmount_if_needed(one_channel);
    }
}
