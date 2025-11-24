/// unmounts whatever device is mounted in the mount folder; returns an error string if it fails
pub fn unmount_if_needed(mount_path: &str, mount_flag: &mut bool) -> Result<(), String> {
    println!("unmounting if needed {} \r", mount_path);
    if *mount_flag {
        println!("unmounting {}\r", mount_path);
        if let Err(error_message) = sys_mount::unmount(mount_path, sys_mount::UnmountFlags::DETACH)
        {
            eprintln!(
                "Failed to unmount the device mounted on {}. Got error {:?}\r",
                mount_path, error_message
            );
            return Err(format!(
                "Failed to unmount the device mounted on {}",
                mount_path
            ));
        } else {
            *mount_flag = false;
        };
    }
    Ok(())
}
