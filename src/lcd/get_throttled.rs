/// A struct to allow us to return both the success as a bool & a String
pub struct ThrottledAsStruct {
    pub pi_is_throttled: bool, // true if the Pi is throttled
    pub result: String, // a 13 to 17 character string which is the result of vcgencmd get_throttled, or an error message as string of unknown length.
}

/// Returns true if the pi is throttled, false otherwise.
/// Returns a 13 to 17 character string which is the result of vcgencmd get_throttled, or an error message as string of unknown length.
/// For details see https://www.raspberrypi.com/documentation/computers/os.html and search for get_throttled
pub fn is_throttled() -> ThrottledAsStruct {
    let mut return_string: String;
    let output_as_result = std::process::Command::new("/bin/vcgencmd")
        .arg("get_throttled")
        .output();
    match output_as_result {
        Ok(output) => {
            if output.status.success() {
                return_string = format!("{}", String::from_utf8_lossy(&output.stdout));
            } else {
                return_string = format!("{}", String::from_utf8_lossy(&output.stderr));
            }
        }
        Err(the_error) => {
            return_string = format!("Throttle err{:?}", the_error);
        }
    }

    return_string = return_string.trim().into();

    // note we couls say "return ThrottledAsStruct {" to be explicit what the return value is.
    ThrottledAsStruct {
        pi_is_throttled: return_string != "throttled=0x0",
        result: return_string,
    }
}
