use substring::Substring;

#[derive(Debug)]
/// if is_valid is true, contains the SSID, local & gateway IP addresses as strings.
pub struct NetworkData {
    pub ssid: String,
    pub local_ip_address: String,
    pub gateway_ip_address: String,
    pub is_valid: bool,
}
impl NetworkData {
    /// initialises SSID, local & gateway IP addresses to "not known" & sets is_valid false
    pub fn new() -> Self {
        NetworkData {
            ssid: "not known".to_string(),
            local_ip_address: "not known".to_string(),
            gateway_ip_address: "not known".to_string(),
            is_valid: false,
        }
    }
}

/// Tries once to get the IP address of the Pi's Wi-Fi interface, the IP address of the gateway & the SSID.
/// If it fails, it sleeps for 50 ms so the CPU is not hogged if called multiple times quickly.
/// It will probably fail the first few times it is called, so needs to be called multiple times.
/// The function assumes the Pi's language is English
pub fn try_once_to_get_wifi_network_data() -> Result<NetworkData, String> {
    // use the command nmcli device show wlan0
    let output_as_result = std::process::Command::new("/bin/nmcli")
        .args(["device", "show", "wlan0"]) // on a Pi, the Wi-Fi device is wlan0
        .output();
    match output_as_result {
        Ok(output) => {
            let output_as_ascii = unsafe { String::from_utf8_unchecked(output.stdout) }; // convert the output, which is a series of bytes, to a string
            if output_as_ascii.contains("100 (") {
                //contains a line such as "GENERAL.STATE   connected " followed by the SSID, or "GENERAL.STATE 30 (disconnected) ; the number spaces is indicative only
                let output_as_a_vec_of_lines: Vec<&str> = // get the output as a vec of individual lines
                    output_as_ascii.split( '\n').collect();
                let ssid_line_number = 5; // the SSID is on line 5
                let local_ip_address_number = 7; // the local IP address is on line 7
                let gateway_ip_address_number = 8; // the gatewway address is on line 8
                let ssid = output_as_a_vec_of_lines[ssid_line_number] // [5] gives the SSID entry
                    .substring(20, output_as_a_vec_of_lines[ssid_line_number].len()) //20 skips the name of the entry
                    .trim()
                    .to_string();

                let mut local_ip_address = output_as_a_vec_of_lines[local_ip_address_number]
                    .substring(15, output_as_a_vec_of_lines[local_ip_address_number].len())
                    .to_string(); // 15 skips the name of the entry
                local_ip_address = local_ip_address.trim().to_string(); // at this point it has the format 192.168.1.2/23 ie it contains the length too.

                if let Some(pos) = local_ip_address.find("/") {
                    local_ip_address = local_ip_address[0..pos].to_string(); // remove the lenth & the slash that precedes it
                };

                let gateway_ip_address = output_as_a_vec_of_lines[gateway_ip_address_number]
                    .substring(
                        15, // 15 skips the name of the entry
                        output_as_a_vec_of_lines[gateway_ip_address_number].len(),
                    )
                    .trim()
                    .to_string();

                Ok(NetworkData {
                    ssid,
                    local_ip_address,
                    gateway_ip_address,
                    is_valid: true,
                })
            } else {
                use std::thread::sleep;
                use std::time::Duration;
                sleep(Duration::from_millis(50)); //sleep for a little time si if this subroutine is called again at one, it does not hog the CPU
                Err(format!("not connected; got {}", output_as_ascii))
            }
        }
        Err(error) => Err(format!("got error {} when trying to use nmcli\r", error)),
    }
}

/// Reads from file pass.toml in the device specified in the TOML configuration file the SSID and password & stores them in the operating system.
/// If the USB path is not specified in the TOML configuration file, returns an error.
/// If successful status_of_rradio.running_status is set to RunningStatus::Startingup;
pub fn set_up_wifi_password(
    status_of_rradio: &mut crate::player_status::PlayerStatus,
    lcd: &mut crate::lcd::Lc,
    config: &crate::read_config::Config,
) -> Result<(), String> {
    status_of_rradio
        .all_4lines
        .update_if_changed("Please wait maybe 20 seconds; trying to set new Wi-Fi password");
    lcd.write_rradio_status_to_lcd(status_of_rradio, config);

    let mount_device;
    let mount_folder;
    if let Some(usb) = &config.usb {
        mount_device = &usb.device;
        mount_folder = &usb.mount_folder;
    } else {
        return Err(
            "USB must be specified in TOML file so the program can read the SSID etc".to_string(),
        );
    }

    let passfile = format!("{mount_folder}//pass.toml");

    match sys_mount::Mount::builder()
        .fstype("vfat")
        .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
        .mount(mount_device, mount_folder)
    {
        Ok(_) => {
            status_of_rradio.usb_is_mounted = true;
            let config_as_result =
                std::fs::read_to_string(passfile.clone()).map_err(|toml_file_read_error| {
                    format!(
                        "Program {} Couldn't read file {passfile}. Got {toml_file_read_error}",
                        env!("CARGO_PKG_NAME")
                    )
                });
            // next unmount the USB stick as we have read the file before any other errors might happen
            if let Err(error_message) =
                sys_mount::unmount(mount_folder, sys_mount::UnmountFlags::DETACH)
            {
                return Err(format!(
                    "When trying to unmount the USB stick after reading pass.toml got error {}",
                    error_message
                ));
            } else {
                status_of_rradio.usb_is_mounted = false; // we unmounted the USB stick OK
            }

            match config_as_result {
                Ok(config_as_string) => {
                    #[derive(Debug, serde::Deserialize)] // serde::Deserialize is needed by toml::from_str
                    /// contains the Wi-Fi SSID & password
                    struct WifiData {
                        ssid: String,     // needs to be a string as we do not know its size at compile time
                        password: String, // needs to be a string as we do not know its size at compile time
                    }

                    let wifi_data_as_result: Result<WifiData, String> =
                        toml::from_str(&config_as_string).map_err(|toml_file_parse_error| {
                            format!(
                                "{} Couldn't parse pass.toml to get the SSID & password. Got {toml_file_parse_error}",
                                env!("CARGO_PKG_NAME")
                            )
                        });

                    match wifi_data_as_result {
                        Ok(wi_fi_data) => {
                            //  use the commnad "nmcli device password wifi connect the_ssid password thepassword"  (with no quotes)
                            let args = [
                                "device",
                                "wifi",
                                "connect",
                                wi_fi_data.ssid.as_str(),
                                "password",
                                wi_fi_data.password.as_str(),
                            ];
                            let output2_as_result =
                                std::process::Command::new("/bin/nmcli").args(args).output();
                            match output2_as_result {
                                Ok(result_as_bytes) => {
                                    if !result_as_bytes.stdout.is_empty() {
                                        // the command gave an output, possibly an error output
                                        let result_of_setting_wifi_password = unsafe {
                                            String::from_utf8_unchecked(result_as_bytes.stdout)
                                        };
                                        // the return string should be similar to "Device 'wlan0' successfully activated with '7c9b9098-88a2-4593-b541-5ef496f3781f'." with a trailing new line
                                        // next we need to get the IP address, assuming it worked OK
                                        if result_of_setting_wifi_password
                                            .contains("successfully activated with ")
                                        {
                                            // not only did we get an output, but the SSID & password were accepted
                                            match try_once_to_get_wifi_network_data() {
                                                // so next get the network data
                                                Ok(network_data) => {
                                                    status_of_rradio.network_data = network_data;
                                                    status_of_rradio.running_status =
                                                        crate::RunningStatus::Startingup;
                                                    Ok(())
                                                }
                                                Err(error_message) => {
                                                     Err(format!( "Set Wi-Fi password seemingly sucessfully but could not get the IP addresss and got error {}", error_message).to_string())
                                                }
                                            }
                                        } else {
                                            Err("Tried to set Wi-FI SSID & password but failed"
                                                .to_string())
                                        }
                                    } else if !result_as_bytes.stderr.is_empty() {
                                        let stderr_output = unsafe {
                                            String::from_utf8_unchecked(result_as_bytes.stderr)
                                        };
                                        Err(format!(
                                            "When trying to set the SSID & Wi-Fi password got error {}",                                       
                                            stderr_output.chars().take(stderr_output.len()-1).collect::<String>()

                                        ))
                                    } else {
                                        Err("Failed to set the SSID & password for an unknown reason".to_string())
                                    }
                                }
                                Err(error) => {
                                    Err(format!("Failed to set IP address; got error {}", error))
                                }
                            }
                        }
                        Err(error) => Err(format!(
                            "could not read pass.toml file. Got error {:?}\r",
                            error
                        )),
                    }
                }
                Err(error) => Err(format!(
                    "When parsing the pass.toml file, got error {:?}",
                    error
                )),
            }
        }
        Err(error_message) => Err(format!("Mount failure. Got error {:?}\r", error_message)),
    }
}
