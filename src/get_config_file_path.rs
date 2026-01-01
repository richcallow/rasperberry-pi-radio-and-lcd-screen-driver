pub fn get_config_file_path(default_toml_path: &String) -> Result<String, String> {
    let mut config_file_path_from_args = String::from(default_toml_path); // the default value if not specified
                                                                          //let config_file_path = {
    let mut args = std::env::args().skip(1); //skip the name of the first executable
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "-C" | "--config" => {
                config_file_path_from_args = args.next().ok_or(
                    "the format is -c followed by the file name, but could not find the file name.",
                )?;
            }
            "-V" | "-v" | "--version" => {
                println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                //return Ok("got -V".to_string());
            }
            _ => {
                let error_message = format!("unhandled argument  {arg:?}. Valid arguments are -c then the config file name or -V");

                return Err(error_message);
            }
        }
    }

    //};
    Ok(config_file_path_from_args)
}
