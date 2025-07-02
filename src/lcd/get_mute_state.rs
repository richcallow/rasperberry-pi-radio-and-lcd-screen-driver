use core::fmt;

use rppal::gpio::Gpio;

/// Specifies the state of the DigiAMP+ amplfier
#[derive(PartialEq)]
pub enum MuteState {
    Muted,       // the amplifer has been set to be muted
    NotMuted,    // the amplifer has been set to be not muted
    NoAmplifier, // no DigiAMP+ amplfier present AND used.
    ErrorFound,  // got an error & we could not work out if there was a DigiAMP+ amplfier
}
impl fmt::Display for MuteState {
    // the rust-approved way of implementing to_string()
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MuteState::Muted => write!(f, "muted"),
            MuteState::NotMuted => write!(f, ""),
            MuteState::ErrorFound => write!(f, "mute_error"),
            MuteState::NoAmplifier => write!(f, "No amplifier"),
        }
    }
}

/// Gets the mute state of the DigiAMP+ amplfier if there is one
pub fn get_mute_state() -> MuteState {
    // this command sets the port low  raspi-gpio set 22 op dl
    // this command sets the port high raspi-gpio set 22 op dh
    const MUTE_PORT: u8 = 22; // GPIO 22 controls whether or not the DigiAMP+ amplifier is muted
    let all_gpios_and_errors = Gpio::new();
    match all_gpios_and_errors {
        Ok(gpios) => match gpios.get(MUTE_PORT) {
            Ok(pin22) => {
                // as it is Ok, we got GPIO 22; but is it in use to control the DigiAMP+ amplifier
                if pin22.mode() == rppal::gpio::Mode::Input {
                    MuteState::NoAmplifier // there is no DigiAMP+ amplifier, or at least the kernal has not changed the pin to be an output pin
                } else if pin22.into_input().is_low() {
                    MuteState::Muted // & the light on the amplifier board is on
                } else {
                    MuteState::NotMuted // and the light is off
                }
            }
            Err(pin22_err) => {
                println!("Got error {} when trying to get mute pin", pin22_err);
                MuteState::ErrorFound
            }
        },
        Err(err_message) => {
            println!(
                "When trying to get a GPIO pin got the error {}",
                err_message
            );
            MuteState::ErrorFound
        }
    }
}
