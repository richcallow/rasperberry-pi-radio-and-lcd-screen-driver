/// ejects the CD drive, or returns an error string
pub fn eject() -> Result<(), String> {
    /*let mut device = std::fs::OpenOptions::new()
            .custom_flags(libc::O_NONBLOCK)
            .read(true)
            .open("/dev/cdrom");
    */
    use eject::{device::Device, discovery::cd_drives};

    let num_cd_drives = eject::discovery::cd_drives().count();
    if num_cd_drives == 0 {
        return Err("No drives present".to_string());
    }

    let cdrom_path = cd_drives().next().unwrap(); // cannot fail in theorey as we have checked we have a CD drive present
                                                  //println!("cdrom_path{:?}", cdrom_path); // it is "/dev/sr0"
    let cdrom = Device::open(&cdrom_path).map_err(|error_message| {
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
