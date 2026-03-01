use crate::{
    my_dbg,
    player_status::{PlayerStatus, RealTimeDataOnOneChannel},
};
/// Unmounts whatever device is mounted in the mount folder; returns an error string if it fails
pub fn unmount_if_needed(
    real_time_data_one_channel: &mut RealTimeDataOnOneChannel,
) -> Result<(), String> {
    if let Some(data_one_channel) = &mut real_time_data_one_channel.channel_data.media_details {
        if !data_one_channel.is_mounted {
            return Ok(());
        }
        println!("unmounting {:?}\r", data_one_channel);
        if let Err(error_message) = sys_mount::unmount(
            &data_one_channel.mount_folder,
            sys_mount::UnmountFlags::DETACH,
        ) {
            eprintln!(
                "Failed to unmount the device mounted on {}. Got error {:?}\r",
                data_one_channel.mount_folder, error_message
            );
            return Err(format!(
                "Failed to unmount the device mounted on {}",
                data_one_channel.mount_folder
            ));
        } else {
            data_one_channel.is_mounted = false;
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
