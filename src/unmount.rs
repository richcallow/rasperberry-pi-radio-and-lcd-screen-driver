use crate::get_channel_details;

/// unmounts whatever device is mounted if the mount folder; returns an error string if it fails
pub fn unmount_if_needed(
    config: &crate::read_config::Config,
    status_of_rradio: &mut crate::player_status::PlayerStatus,
) -> Result<(), String> {
    if let Some(mount_data) = &config.mount_data {
        if status_of_rradio.item_mounted != get_channel_details::ItemMounted::Nothing {
            if let Err(error_message) =
                sys_mount::unmount(&mount_data.mount_folder, sys_mount::UnmountFlags::DETACH)
            {
                eprintln!(
                    "Failed to unmount the device mounted on {}. Got error {:?}\r",
                    mount_data.mount_folder, error_message
                );

                return Err(format!(
                    "Failed to unmount the device mounted on {}",
                    mount_data.mount_folder
                ));
            } else {
                status_of_rradio.item_mounted = get_channel_details::ItemMounted::Nothing;
                println!("unmounted\r")
            };
        }
        Ok(())
    } else {
        Ok(())
    }
}
