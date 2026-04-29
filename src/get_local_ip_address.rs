use crate::player_status::PlayerStatus;
use std::fs;
use substring::Substring;

#[derive(Debug)]
/// if is_valid is true, contains the SSID, local & gateway IP addresses as strings.
pub struct NetworkDataNew {
    pub ssid: String,
    pub local_ip_address: String, // these are only ever used as a string, so it is simpler to keep them as a string
    pub gateway_ip_address: String,
    pub is_valid: bool,
}
impl NetworkDataNew {
    /// initialises SSID, local & gateway IP addresses to 8.8.8.8" & sets is_valid false
    pub fn new() -> Self {
        NetworkDataNew {
            ssid: "not known".to_string(),
            local_ip_address: "8.8.8.8".to_string(),
            gateway_ip_address: "8.8.8.8".to_string(),
            is_valid: false,
        }
    }
}
impl Default for NetworkDataNew {
    fn default() -> Self {
        NetworkDataNew::new()
    }
}

/// Tries once to get the IP address of the Pi's Wi-Fi interface, the IP address of the gateway & the SSID.
/// It might fail the first few times it is called, so might need to be called multiple times.
/// The function assumes the Pi's language is English
pub fn try_once_to_get_wifi_network_data() -> Result<NetworkDataNew, String> {
    // use the command nmcli -t device show wlan0
    // -t = terse format which is easier to parse
    let output_as_result = std::process::Command::new("/bin/nmcli")
        .args(["-t", "device", "show", "wlan0"]) // on a Pi, the Wi-Fi device is wlan0
        .output();
    match output_as_result {
        Ok(output) => {
            let output_as_ascii = unsafe { String::from_utf8_unchecked(output.stdout) }; // convert the output, which is a series of bytes, to a string
            //contains a line such as "GENERAL.STATE   connected " followed by the SSID, or "GENERAL.STATE 30 (disconnected) ; the number spaces is indicative only
            let output_as_a_vec_of_lines: Vec<&str> = // get the output as a vec of individual lines
                    output_as_ascii.split( '\n').collect();
            const SSID_LINE_NUMBER: usize = 5; // the SSID is on line 5
            const LOCAL_IP_ADDRESS_NUMBER: usize = 7; // the local IP address is on line 7
            const GATEWAY_IP_ADDRESS_NUMBER: usize = 8; // the gateway address is on line 8

            let mut ssid = output_as_a_vec_of_lines[SSID_LINE_NUMBER]; // [5] gives the SSID entry
            ssid = ssid.substring("GENERAL.CONNECTION:".len(), ssid.len() - 1);

            let mut local_ip_address = output_as_a_vec_of_lines[LOCAL_IP_ADDRESS_NUMBER];

            if let Some(pos) = local_ip_address.find("/") {
                local_ip_address = local_ip_address.substring("IP4.ADDRESS[1]:".len(), pos)
            } else {
                return Err("failed to parse IP address".to_string());
            }

            let mut gateway_ip_address = output_as_a_vec_of_lines[GATEWAY_IP_ADDRESS_NUMBER];
            gateway_ip_address =
                gateway_ip_address.substring("IP4.GATEWAY:".len(), gateway_ip_address.len());

            Ok(NetworkDataNew {
                ssid: ssid.to_string(),
                local_ip_address: local_ip_address.to_string(),
                gateway_ip_address: gateway_ip_address.to_string(),
                is_valid: true,
            })
        }
        Err(error) => Err(format!("got error {} when trying to use nmcli\r", error)),
    }
}

impl PlayerStatus {
    /// Tries multiple times to get the IP address of the Pi's Wi-Fi interface, the IP address of the gateway & the SSID.
    pub fn update_network_data(
        &mut self,
        lcd: &mut crate::lcd::Lc,
        config: &crate::read_config::Config,
    ) {
        self.running_status = crate::lcd::RunningStatus::LongMessageOnAll4Lines;
        for count in 0..40 {
            // go round the loop multiple times looking for the IP address
            self.all_4lines.update_if_changed(
                format!("Looking for IP address. Attempt number {count}").as_str(),
            );
            lcd.write_rradio_status_to_lcd(self, config);

            match try_once_to_get_wifi_network_data() {
                Ok(network_data) => {
                    self.network_data = network_data;
                    self.running_status = crate::RunningStatus::Startingup;
                    self.all_4lines.update_if_changed("");
                    return;
                }
                Err(error) => self
                    .all_4lines
                    .update_if_changed(format!("Got error {error}  on count {count}").as_str()),
            }
        }
    }
}

// set_up_wifi_password can be tested by using
// nmcli connection show        to find the SSID of the wifi connection in use
// nmcli connection delete the_ssid of the Wi-Fi connection
// & then running the program

/// Reads from file pass.toml in the device specified in the TOML configuration file the SSID and password & stores them in the operating system.
/// If the USB path is not specified in the TOML configuration file, returns an error.
/// If successful status_of_rradio.running_status is set to RunningStatus::Startingup;
pub fn set_up_wifi_password(
    status_of_rradio: &mut crate::player_status::PlayerStatus,
) -> Result<(), String> {
    let wifi_file_mount_path = format!("{}wifi_folder", status_of_rradio.startup_folder);

    const ALREADY_EXISTS: i32 = 17;

    if let Err(rr) = fs::create_dir(&wifi_file_mount_path)
        && rr.raw_os_error() != Some(ALREADY_EXISTS)
    {
        return Err(
            "Wi-Fi does not work and could  not create folder to mount the Wi-Fi file".to_string(),
        );
    }

    if let Err(error) = sys_mount::Mount::builder()
        .fstype("vfat")
        .flags(sys_mount::MountFlags::RDONLY | sys_mount::MountFlags::NOATIME)
        .data("iocharset=utf8")
        .mount("/dev/sda1", &wifi_file_mount_path)
    {
        return Err(format!(
            "Wi-Fi does not work and could not mount a memory stick to read the Wi-Fi password file. got error {}",
            error
        ));
    };

    let passfile = format!("{}//pass.toml", &wifi_file_mount_path);
    if !std::path::Path::new(&passfile).exists() {
        return Err(format!(
            "Wi-Fi does not seem to be working and cannot find the Wi-Fi password file {}",
            passfile
        ));
    }

    let wifi_data_as_result = std::fs::read_to_string(&passfile).map_err(|toml_file_read_error| {
        format!(
            "Program {} Couldn't read file {passfile}. Got {toml_file_read_error}",
            env!("CARGO_PKG_NAME")
        )
    });

    // next unmount the USB stick as we have read the file before any other errors might happen
    if let Err(error_message) =
        sys_mount::unmount(wifi_file_mount_path, sys_mount::UnmountFlags::DETACH)
    {
        return Err(format!(
            "When trying to unmount the USB stick after reading pass.toml got error {}",
            error_message
        ));
    };

    match wifi_data_as_result {
        Ok(wifi_data) => {
            #[derive(Debug, serde::Deserialize)] // serde::Deserialize is needed by toml::from_str
            /// contains the Wi-Fi SSID & password
            struct WifiData {
                ssid: String, // needs to be a string as we do not know its size at compile time
                pass: String, // needs to be a string as we do not know its size at compile time
            }
            let parsed_wifi_data_as_result: Result<WifiData, String> =
                toml::from_str(&wifi_data).map_err(|toml_file_parse_error| {
                    format!(
                        "{} Couldn't parse pass.toml to get the SSID & password. Got {toml_file_parse_error}",
                        env!("CARGO_PKG_NAME")
                    )
                });
            match parsed_wifi_data_as_result {
                Ok(parsed_wifi_data) => {
                    //  use the command "nmcli device password wifi connect the_ssid password thepassword"  (with no quotes)
                    let args = [
                        "device",
                        "wifi",
                        "connect",
                        parsed_wifi_data.ssid.as_str(),
                        "password",
                        parsed_wifi_data.pass.as_str(),
                    ];

                    let output2_as_result =
                        std::process::Command::new("/bin/nmcli").args(args).output();
                    match output2_as_result {
                        Ok(result_as_bytes) => {
                            if !result_as_bytes.stdout.is_empty() {
                                // the command gave an output, possibly an error output
                                let result_of_setting_wifi_password =
                                    unsafe { String::from_utf8_unchecked(result_as_bytes.stdout) };
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
                                            status_of_rradio.all_4lines = crate::lcd::ScrollData::new("", 4);
                                            Ok(())
                                        }
                                        Err(error_message) => {
                                                Err(format!( "Set Wi-Fi password seemingly sucessfully but could not get the IP address and got error {}", error_message).to_string())
                                        }
                                    }
                                } else {
                                    Err("Tried to set Wi-FI SSID & password but failed".to_string())
                                }
                            } else if !result_as_bytes.stderr.is_empty() {
                                let stderr_output =
                                    unsafe { String::from_utf8_unchecked(result_as_bytes.stderr) };
                                Err(format!(
                                    "When trying to set the SSID & Wi-Fi password got error {}",
                                    stderr_output
                                        .chars()
                                        .take(stderr_output.len() - 1)
                                        .collect::<String>()
                                ))
                            } else {
                                Err("Failed to set the SSID & password for an unknown reason"
                                    .to_string())
                            }
                        }
                        Err(error) => Err(format!("Failed to set IP address; got error {}", error)),
                    }
                }
                Err(error) => Err(format!(
                    "Wi-Fi does not seem to be working and cannot read the Wi-Fi password file {}",
                    error
                )),
            }
        }
        Err(error) => Err(format!(
            "Wi-Fi does not seem to be working and cannot read the Wi-Fi password file {}",
            error
        )),
    }
}
