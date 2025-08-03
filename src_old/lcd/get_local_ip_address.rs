/// Gets the IP adress of the Pi's interface
/// if successful, returns the IP address of the PI's interface as a string or None
/// It will probably fal the first few times it is called so need to be called multiple times
pub fn get_local_ip_address1() -> Option<String> {
    for iface in pnet::datalink::interfaces() {
        if iface.is_up() && !iface.is_loopback() && !iface.ips.is_empty() {
            // this if statement filters off the loopback address & addresses that do not have an IP address
            for ipaddr in &iface.ips {
                let ip4addr = match ipaddr {
                    pnet::ipnetwork::IpNetwork::V4(addr) => addr.ip(), // filters off the "/24" at the end of the IP address
                    pnet::ipnetwork::IpNetwork::V6(_) => continue,
                };
                return Some(ip4addr.to_string());
            }
        }
    }
    None
}
/// Looks to see if the WiFi is connected; if not, it reads from file pass.toml on SDA1 the SSID and password & stores them in the operating system.
pub fn set_up_wifi() -> Option<String> {
    let output_as_result = std::process::Command::new("/bin/nmcli")
        .arg("device")
        .output();
    match output_as_result {
        Ok(output) => {
            let output_as_ascii = unsafe { String::from_utf8_unchecked(output.stdout) };
            if let Some(position) = output_as_ascii.find("wifi ") {
                let connection_status = output_as_ascii[position..position + 50].to_string();
                println!("connection_status{connection_status}\r");
                if let Some(position_end_line) = connection_status.find("\n") {
                    if connection_status.contains("disconnected") {
                        println!("Wi-FI disconnected\r"); // so we must get the SSID & password

                        let mount_result_as_result = sys_mount::Mount::builder()
                            .fstype("vfat")
                            .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
                            .mount("/dev/sda1", "//home//pi//mount_folder");

                        match mount_result_as_result {
                            Ok(_) => {
                                let config_as_result =
                                    std::fs::read_to_string("//home//pi//mount_folder//pass.toml")
                                        .map_err(|toml_file_read_error| {
                                            format!(
                                        "{} Couldn't read file pass.toml. Got {toml_file_read_error}",
                                        env!("CARGO_PKG_NAME")
                                    )
                                        });

                                match config_as_result {
                                    Ok(config_as_string) => {
                                        #[derive(Debug, serde::Deserialize)]
                                        #[serde(default)]
                                        struct WifiData {
                                            ssid: String,
                                            password: String,
                                        }
                                        impl Default for WifiData {
                                            fn default() -> Self {
                                                Self {
                                                    ssid: String::new(),
                                                    password: String::new(),
                                                }
                                            }
                                        }

                                        let wifi_data_as_result: Result<WifiData, String> =
                                            toml::from_str(&config_as_string).map_err(
                                                |toml_file_parse_error| {
                                                    format!(
                                                        "{} Couldn't parse passfile  Got {toml_file_parse_error}",
                                                        env!("CARGO_PKG_NAME")
                                                    )
                                                },
                                            );
                                        match wifi_data_as_result {
                                            Ok(wi_fi_data) => {
                                                let args = [
                                                    "device",
                                                    "wifi",
                                                    "connect",
                                                    wi_fi_data.ssid.as_str(),
                                                    "password",
                                                    wi_fi_data.password.as_str(),
                                                ];
                                                let output2_as_result =
                                                    std::process::Command::new("/bin/nmcli")
                                                        .args(args)
                                                        .output();

                                                println!(
                                                    "output from setting {:?}\rargs {:?}",
                                                    output2_as_result, args
                                                );
                                            }
                                            Err(error) => {
                                                eprintln!("could not read pass.toml file. Got error {:?}\r", error);
                                                return None;
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "When parsing the pass.toml file, got error {:?}",
                                            error
                                        )
                                    }
                                }
                            }
                            Err(error_message) => {
                                eprintln!("Mount failure. Got error {:?}\r", error_message);
                            }
                        }

                        /*use sys_mount::UnmountFlags;
                        if let Err(error_message) =
                            sys_mount::unmount(&usb.mount_folder, UnmountFlags::DETACH)
                        {
                            eprintln!("When trying to unmount the USB stick after reading pass.toml gotr error {}", error_message)
                        }*/

                        return None;
                    }
                }
            }
            Some("got it".to_string())
        }
        Err(the_error) => {
            eprintln!("Failed to run nmcli. Got error {:?}\r", the_error);
            None
        }
    }
}
