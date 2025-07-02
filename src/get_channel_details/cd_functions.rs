/// Ejects the CD drive, or returns an error string
pub fn eject() -> Result<(), String> {
    let num_cd_drives = eject::discovery::cd_drives().count();
    if num_cd_drives == 0 {
        return Err("No drives present".to_string());
    }

    let cdrom_path = eject::discovery::cd_drives().next().unwrap(); // cannot fail in theorey as we have checked we have a CD drive present
    let cdrom = eject::device::Device::open(&cdrom_path).map_err(|error_message| {
        format!(
            "Got the following error when trying to open the CD drive {:?}",
            error_message.to_string()
        )
    })?;

    cdrom.eject().map_err(|error_message| {
        format!(
            "got the following error when ejecting the CD{:?}",
            error_message
        )
    })
}
