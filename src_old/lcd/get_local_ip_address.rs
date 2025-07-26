/// Gets the IP adress of the Pi's interface
pub fn get_local_ip_address() -> Option<String> {
    let mut return_value: String = String::from("bad IP address");
    let mut got_address = false;

    for _ip_count in 1..100 {
        // go round a wait loop many times as we need to wait for the IP addresses
        // many times before we get the local IP address working
        for iface in pnet::datalink::interfaces() {
            if iface.is_up() && !iface.is_loopback() && !iface.ips.is_empty() {
                // this if statement filters off the loopback address & addresses that do not have an IP address
                for ipaddr in &iface.ips {
                    let ip4addr = match ipaddr {
                        pnet::ipnetwork::IpNetwork::V4(addr) => addr.ip(), // filters off the "/24" at the end of the IP address
                        pnet::ipnetwork::IpNetwork::V6(_) => continue,
                    };
                    return_value = ip4addr.to_string();
                    got_address = true;
                }
            }
            if got_address {
                break;
            }
        }
        if got_address {
            break;
        }
        use std::thread::sleep;
        use std::time::Duration;
        sleep(Duration::from_millis(50)); //sleep until the Ethernet interface is up
    }
    if got_address {
        Some(return_value)
    } else {
        None
    }
}
/// Looks to see if the WiFi is connected; if not, it reads from file pass.toml on SDA1 the SSID and password and puts them in the correct place
pub fn set_up_wifi(config: &crate::read_config::Config) -> Option<String> {
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
                                        "{} Couldn't read pass.toml Got {toml_file_read_error}",
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
                                                        "{} Could'nt parse passfile  Got {toml_file_parse_error}",
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
                                                    "output frm setting {:?}\rargs {:?}",
                                                    output2_as_result, args
                                                );
                                            }
                                            Err(error) => {
                                                eprintln!("could not read pass.toml file\r");
                                                return None;
                                            }
                                        }
                                    }
                                    Err(error) => {}
                                }
                                return None;
                            }
                            Err(error_message) => {
                                eprintln!("Mount failure\r");
                                return None;
                            }
                        }
                    }
                }
            }
            Some("got it".to_string())
        }
        Err(the_error) => {
            eprintln!("failed to run nmcli\r");
            None
        }
    }
}
