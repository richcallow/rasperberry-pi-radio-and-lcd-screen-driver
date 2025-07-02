use std::fs::File;
use std::io::prelude::Read; //needed for .read_to_string

/// gets the temperature in degrees Centrigrade; negative numbers mean that there was an error 
pub fn get_cpu_temperature() -> i32 {
    let mut file = match File::open("/sys/class/thermal/thermal_zone0/temp") {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "Problem opening the CPU temperature pseudo-file: {:?}",
                error
            );
            return -2;
        }
    };

    let mut cpu_temperature = String::new();

    match file.read_to_string(&mut cpu_temperature) {
        Err(why) => {
            eprintln!("couldn't read the temperature from the pseduo file {}", why);
            -1
        }
        Ok(_file_size) => {
            match cpu_temperature //cpu_temperature contains the temperature in milli-C and a line terminator
                .trim()
                .parse::<i32>()
            {
                Ok(milli_temp) => milli_temp / 1000, //divide by 1000 to convert to C from milli-C and return the temperature
                Err(err) => {
                    eprintln!("got err {} when parsing the temperature", err);
                    -3
                }
            }
        }
    }
}
