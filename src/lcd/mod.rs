/*
The lines below instruct the Pi to use the hd44780 driver & load it
(or equivalent if different pins are to be used.) The pin numbers specified are GPIO pin numbers
    dtoverlay=hd44780-lcd,pin_rs=16,pin_en=12,display_height=4,display_width=20
    dtparam=pin_d4=24,pin_d5=23,pin_d6=25,pin_d7=9
*/

use std::{io::Write, time::Instant};

use anyhow::Context;

use crate::player_status;

mod character_pattern;
mod get_local_ip_address;
mod get_temperature;
mod get_throttled;
mod get_wifi_strength;

#[derive(PartialEq, Debug)]
pub enum LineNum {
    Line1,
    Line2,
    Line3,
    Line4,
}
#[derive(Debug, PartialEq, Clone)]
/// Specifies if we are starting up, in which case we want to see the startup message, shutting down or running normally.
pub enum RunningStatus {
    Startingup,
    NoChannel,         //user enetered a channel that could not be found
    NoChannelRepeated, //user enetered at least twice consecutively a channel that could not be found
    RunningNormally,
    BadErrorMessage,
    ShuttingDown,
}

pub const NUM_CHARACTERS_PER_LINE: usize = 20; //the display is visually 20 * 4 characters
pub const NUM_CHARACTERS_PER_SCREEN: usize = 4 * NUM_CHARACTERS_PER_LINE;

pub const VOLUME_CHAR_COUNT: usize = 7;
pub const LINE1_DATA_CHAR_COUNT: usize = NUM_CHARACTERS_PER_LINE - VOLUME_CHAR_COUNT;

/// encodes the line numbers  on the LCD screen
impl LineNum {
    fn into_usize(self) -> usize {
        match self {
            LineNum::Line1 => 0,
            LineNum::Line2 => 1,
            LineNum::Line3 => 2,
            LineNum::Line4 => 3,
        }
    }
}

#[derive(Default)]
struct LcdScreenEncodedText {
    bytes: Vec<u8>,
}

impl std::fmt::Debug for LcdScreenEncodedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"")?;

        for &b in self.bytes.iter() {
            match b {
                0x20..=126 => write!(f, "{}", (b as char).escape_debug())?,
                _ => write!(f, "\\x{b:02x}")?,
            }
        }

        write!(f, "\"")?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct ScrollData {
    text: LcdScreenEncodedText,
    scroll_position: usize,
    num_lines: usize,
    last_update_time: Instant,
}

impl ScrollData {
    /// encodes the new text into the LCD screen coding & stores that in text_bytes. Also initialises the scrolling state.
    pub fn new(text: &str, num_lines: usize) -> Self {
        let mut text_bytes = Vec::new();

        for one_char in text.chars() {
            if one_char < '~' {
                text_bytes.push(one_char as u8);
            } else {
                text_bytes.extend_from_slice(match one_char {
                    'é' => &[5], // e accute fifth bespoke character defined starting with the zeroeth bespoke character
                    'è' => &[6], // e grave
                    'à' => &[7], // a grave
                    'ä' => &[0xE1], // a umlaut            // see look up table in GDM2004D.pdf page 9/9
                    'ñ' => &[0xEE], // n tilde
                    'ö' => &[0xEF], // o umlaut
                    'ü' => &[0xF5], // u umlaut
                    'π' => &[0xE4], // pi
                    'µ' => &[0xF7], // mu
                    '~' => &[0xF3], // cannot display tilde using the standard character set in GDM2004D.pdf. This is the best we can do.
                    '' => &[0xFF], // <Control>  = 0x80 replaced by splodge
                    _ => unidecode::unidecode_char(one_char).as_bytes(),
                });
            }
        }

        Self {
            text: LcdScreenEncodedText { bytes: text_bytes },
            scroll_position: 0,
            num_lines,
            last_update_time: Instant::now(),
        }
    }

    /// Updates self with the new text (and initialises the scrolling state) if the encoded version `new_text` does not match the current text.
    pub fn update_if_changed(&mut self, new_text: &str) {
        let new_scroll_data = Self::new(new_text, self.num_lines); // remember that new initialises the scrolling state.

        println!(
            "new_scroll_data {:?}   \r\n  old {:?}\r",
            new_scroll_data, self.text
        );

        if self.text.bytes != new_scroll_data.text.bytes {
            *self = new_scroll_data;
        }
    }

    /// Get the text bytes after the scroll position.
    ///
    /// `impl Iterator<Item = u8> + '_` means it returns some anonymous type which implements Iterator with an Item of u8 and a lifetime of the same lifetime as `self`
    pub fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.text.bytes.iter().copied().skip(self.scroll_position)
    }

    pub fn text_equals(&self, other: &Self) -> bool {
        self.text.bytes == other.text.bytes
    }

    pub fn fits_onto_one_line(&self) -> bool {
        self.text.bytes.len() <= NUM_CHARACTERS_PER_LINE
    }

    pub fn scroll(&mut self) {
        let length_buffer_in_octets = self.num_lines * NUM_CHARACTERS_PER_LINE;

        if self.text.bytes.len() > length_buffer_in_octets
            && self.last_update_time.elapsed() > std::time::Duration::from_secs(3)
        {
            // We need to scroll

            let increment = self
                .text
                .bytes
                .iter() // Iterate over the octets in the text
                .enumerate() // We want to know how far we've advanced
                .take(14) // we want to advance at most 14
                .skip(6) // we want to advance at least 6
                .find_map(|(increment, character)| (*character == b' ').then_some(increment)) // Find the position offset where that character is a space
                .unwrap_or(6); // If we can't find a space, move 6 characters

            self.scroll_position += increment;

            match self.text.bytes.get((self.scroll_position)..) {
                None => {
                    // We've scrolled past the end of the text

                    self.scroll_position = 0;
                }
                Some(displayed_text) => {
                    // If we've scrolled almost to the end
                    if displayed_text.len() < 10 {
                        self.scroll_position = 0;
                    }
                }
            }
        } else {
            // We don't need to scroll
        }
    }
}

#[derive(Debug)] // if we let the programmer copy or clone this, we will get different versions of the buffer, & it is important that there there is only one version of the truth
pub struct TextBuffer {
    buffer: [u8; NUM_CHARACTERS_PER_SCREEN],
}

impl TextBuffer {
    /// Create a new textbuffer containing NUM_CHARACTERS_PER_SCREEN sapces
    pub const fn new() -> Self {
        // const means it can be evaluated at compile time
        Self {
            buffer: [b' '; NUM_CHARACTERS_PER_SCREEN],
        }
    }

    /// Copies octet_count octets from text as bytes into the offset specified by start into the buffer TextBuffer
    pub fn write_text(
        &mut self,
        text_bytes: impl Iterator<Item = u8>,
        start: usize,
        octet_count: usize,
    ) {
        let buffer_bytes = self
            .buffer
            .iter_mut() //iter_mut iterates over the entire buffer
            .skip(start) // skip the skips the first count of these, so we ony get 40 - start items. if start > 39 it will output an empty series of octets
            .take(octet_count); // and take from the output of skip the first octet_count octets; if count is too big, it stops without giving an error. It accepts zero octets on its input.

        for (buffer_byte, text_byte) in buffer_bytes.zip(text_bytes) {
            *buffer_byte = text_byte;
        }
    }

    /// Writes the message to entire buffer & copies it to STDerr
    pub fn write_abortive_error_message_to_entire_buffer(&mut self, error_message: &str) {
        self.write_text_to_lines(
            error_message.bytes(),
            LineNum::Line1,
            NUM_CHARACTERS_PER_SCREEN,
        );
        eprintln!("{error_message}\r");
    }

    /// copies (line_count * NUM_CHARACTERS_PER_LINE) offset by the number of lines into the buffer TextBuffer
    pub fn write_text_to_lines(
        &mut self,
        text_bytes: impl Iterator<Item = u8>,
        line: LineNum,
        line_count: usize,
    ) {
        self.write_text(
            text_bytes,
            line.into_usize() * NUM_CHARACTERS_PER_LINE,
            line_count * NUM_CHARACTERS_PER_LINE,
        );
    }

    /// copies NUM_CHARACTERS_PER_LINE octets into the specified line of the buffer TextBuffer
    pub fn write_text_to_single_line(
        &mut self,
        text_bytes: impl Iterator<Item = u8>,
        line: LineNum,
    ) {
        self.write_text_to_lines(text_bytes, line, 1);
    }

    pub fn write_character_to_single_position(
        &mut self,
        line: LineNum,
        column: usize,
        character: u8,
    ) {
        if column < NUM_CHARACTERS_PER_LINE {
            let index = line.into_usize() * NUM_CHARACTERS_PER_LINE + column;

            self.buffer[index] = character;
        } else {
            println!(
                "Trying to write character \\x{character:02x} to invalid location: line {line:?}, column {column}"
            );
        }
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Lc {
    lcd_file: std::fs::File,
}

impl Lc {
    fn clear_screen(mut lcd_file: impl std::io::Write) {
        if let Err(err) = write!(lcd_file, "\x1b[LI\x1b[Lb\x1b[Lc") {
            // initialises the screen & stops the cursor blinking & turns the cursor off
            eprintln!("Failed to initialise the screen : {err}");
        }

        // generate the cursors in positions 0 to 7 of the character generator, as the initialisation MIGHt have cleared it
        for char_count in 0..8 {
            let mut out_string = format!("\x1b[LG{:01x}", char_count);
            for col_count in 0..8 {
                let s = format!("{:02x}", character_pattern::BITMAPS[char_count][col_count]);
                out_string = out_string + &s;
            }
            out_string.push(';');

            if let Err(err) = write!(lcd_file, "{}", out_string) {
                eprintln!("Failed to initialise the screen : {err}");
            }

            /*
            the first five strings that software generates & sends are
            const INIT_STRING0: &str = "\x1b[LG0101010101010101f;";
            const INIT_STRING1: &str = "\x1b[LG1080808080808081f;";
            const INIT_STRING2: &str = "\x1b[LG2040404040404041f;";
            const INIT_STRING3: &str = "\x1b[LG3020202020202021f;";
            const INIT_STRING4: &str = "\x1b[LG4010101010101011f;";

            write!(lcd_file, "\x1b[LI\x1b[Lb\x1b[LC") // initialise the screen & stop the cursor blinking & turn the cursor on
                .context("Failed to initialise the screen")?;

            write!(lcd_file, "{}", INIT_STRING0) // write the cursor symbol
                .context("Failed to initialise the screen")?;

            write!(lcd_file, "{}", INIT_STRING1) // write the cursor symbol
                .context("Failed to initialise the screen")?;
            write!(lcd_file, "{}", INIT_STRING2) // write the cursor symbol
                .context("Failed to initialise the screen")?;
            write!(lcd_file, "{}", INIT_STRING3) // write the cursor symbol
                .context("Failed to initialise the screen")?;
            write!(lcd_file, "{}", INIT_STRING4) // write the cursor symbol
                .context("Failed to initialise the screen")?;
            */

            /*println!(
                "initialised character {} with string {}",
                char_count, out_string
            );*/
        }
    }

    /// returns a handle to the LCD screen or panics & explains why.
    pub fn new() -> anyhow::Result<Self> {
        let lcd_file = std::fs::File::options()
            .write(true)
            .open("/dev/lcd")
            .context("Failed to open LCD file. Are you running with root privilege")?;
        Self::clear_screen(&lcd_file);
        Ok(Lc { lcd_file })
    }

    /// Clears the LCD screen, but not any associated text buffers
    pub fn clear(&mut self) {
        Self::clear_screen(&mut self.lcd_file);
    }

    pub fn fill_text_buffer_and_write_to_lcd(
        &mut self,
        status_of_rradio: &player_status::PlayerStatus,
    ) {
        /*println!(
            "chanel number {}  position & duration {:?} {:?}\r",
            status_of_rradio.channel_number,
            status_of_rradio.position_and_duration[status_of_rradio.index_to_current_track]
                .position_ms,
            status_of_rradio.position_and_duration[status_of_rradio.index_to_current_track]
                .duration_ms
        );*/
        let mut text_buffer = TextBuffer::new();

        if let Some(toml_error) = &status_of_rradio.toml_error {
            text_buffer.write_abortive_error_message_to_entire_buffer(toml_error.as_str());
        } else {
            match status_of_rradio.running_status.clone() {
                RunningStatus::Startingup => {
                    Lc::fill_text_buffer_when_starting(&mut text_buffer, status_of_rradio)
                }
                RunningStatus::RunningNormally => {
                    Lc::fill_text_buffer_when_running_normally(&mut text_buffer, status_of_rradio)
                }
                RunningStatus::NoChannel => {
                    Lc::fill_text_buffer_channel_not_found(&mut text_buffer, status_of_rradio)
                }
                RunningStatus::NoChannelRepeated => {
                    Lc::fill_text_buffer_channel_not_found_twice(&mut text_buffer)
                }
                RunningStatus::ShuttingDown => {
                    Lc::fill_text_buffer_when_shutting_down(&mut text_buffer)
                }
                RunningStatus::BadErrorMessage => {
                    Lc::bad_error_message(&mut text_buffer, status_of_rradio)
                }
            };
        }
        for (line_number, line) in text_buffer
            .buffer
            .chunks(NUM_CHARACTERS_PER_LINE)
            .enumerate()
        {
            if let Err(err) = write!(self.lcd_file, "\x1b[Lx0y{line_number};") {
                // move the cursor to the start of the specified line
                println!("in write_text_buffer, Failed to write move the cursor : {err}");
                return;
            }
            if let Err(err) = self.lcd_file.write_all(line) {
                println!("in write_text_buffer, Failed to write text : {err}");
                return;
            }
        }
    }

    pub fn fill_text_buffer_when_starting(
        text_buffer: &mut TextBuffer,
        status_of_rradio: &player_status::PlayerStatus,
    ) {
        text_buffer.write_text(
            get_local_ip_address::get_local_ip_address().bytes(),
            0,
            LINE1_DATA_CHAR_COUNT,
        );
        text_buffer.write_text(
            Lc::get_vol_string(status_of_rradio).bytes(),
            LINE1_DATA_CHAR_COUNT,
            VOLUME_CHAR_COUNT,
        );
        text_buffer.write_text_to_single_line(Lc::get_current_time_text().bytes(), LineNum::Line3);

        text_buffer.write_text_to_single_line(
            Lc::get_temperature_and_wifi_strength_text().bytes(),
            LineNum::Line4,
        );
    }
    pub fn fill_text_buffer_when_running_normally(
        text_buffer: &mut TextBuffer,
        status_of_rradio: &player_status::PlayerStatus,
    ) {
        text_buffer.write_text(
            format!("Station {}", status_of_rradio.channel_number).bytes(),
            0,
            LINE1_DATA_CHAR_COUNT,
        );

        text_buffer.write_text(
            Lc::get_vol_string(status_of_rradio).bytes(),
            LINE1_DATA_CHAR_COUNT,
            VOLUME_CHAR_COUNT,
        );
        text_buffer.write_text_to_single_line(status_of_rradio.line_2_data.bytes(), LineNum::Line2);

        text_buffer.write_text_to_lines(status_of_rradio.line_34_data.bytes(), LineNum::Line3, 2);

        if status_of_rradio.line_34_data.fits_onto_one_line() {
            let trimmed_buffer: u8 = (status_of_rradio.buffering_percent)
                .clamp(0, 99)
                .try_into()
                .unwrap(); // 0 to 100 is 101 values, & the screen only handles 100 values, so trim downwards
                           // the unwrap cannot be called as the min value is 0 & the max is 99 which a U8 can handle

            let column = usize::from(trimmed_buffer / 5);

            let character: u8 = trimmed_buffer % 5;

            text_buffer.write_text_to_single_line("                    ".bytes(), LineNum::Line4);
            text_buffer.write_character_to_single_position(LineNum::Line4, column, character);
        } else {
            text_buffer.write_text(
                format!(
                    "{:>Width$.Width$}",
                    status_of_rradio.buffering_percent,
                    Width = 3
                )
                .bytes(),
                NUM_CHARACTERS_PER_SCREEN - 3,
                3,
            );
        };
    }

    pub fn bad_error_message(
        text_buffer: &mut TextBuffer,
        status_of_rradio: &player_status::PlayerStatus,
    ) {
        text_buffer.write_text_to_lines(status_of_rradio.all_4lines.bytes(), LineNum::Line1, 4);
        eprintln!("Status of rrr is{:?}\r", status_of_rradio.running_status);
    }

    /// Outputs error message with channel number, IP address, data & time temperature & signal strength;
    /// used when the not found occurs for a wrong channel that is not the same as the previous channel
    pub fn fill_text_buffer_channel_not_found(
        text_buffer: &mut TextBuffer,
        status_of_rradio: &player_status::PlayerStatus,
    ) {
        text_buffer.write_text(
            format!("No station {}", status_of_rradio.channel_number).bytes(),
            0,
            LINE1_DATA_CHAR_COUNT,
        );
        text_buffer.write_text(
            Lc::get_vol_string(status_of_rradio).bytes(),
            LINE1_DATA_CHAR_COUNT,
            VOLUME_CHAR_COUNT,
        );
        text_buffer.write_text_to_single_line(Lc::get_current_time_text().bytes(), LineNum::Line3);

        text_buffer.write_text_to_single_line(
            Lc::get_temperature_and_wifi_strength_text().bytes(),
            LineNum::Line4,
        );
    }
    /// Outputs error message with channel number, IP address, data & time temperature & signal strength.
    /// Used when the user selects the same wrong channel twice consecutively
    pub fn fill_text_buffer_channel_not_found_twice(text_buffer: &mut TextBuffer) {
        text_buffer
            .write_text_to_single_line(compile_time::datetime_str!().bytes(), LineNum::Line1);

        text_buffer
            .write_text_to_single_line(Lc::get_throttled_status_and_time().bytes(), LineNum::Line3);
        text_buffer.write_text_to_single_line(
            //"\x00 \x01 \x02 \x03 \x04\x05\x06\x07ñäöü~ÆÇ",
            ScrollData::new("\x00 \x01 \x02 \x03 \x04\x05\x06\x07ñäöüÆÇç", 1).bytes(),
            LineNum::Line4,
        );
    }

    /// Gets the throttled status & time; if the Pi is not throttled it returns "NotThrottled" followed by the time of day, otherwise it returns the throttled code followed by time of day
    pub fn get_throttled_status_and_time() -> String {
        let throttled_status = get_throttled::is_throttled();
        if !throttled_status.pi_is_throttled {
            format!("NotThrottled{}", chrono::Local::now().format("%H:%M:%S"))
        } else {
            format!(
                "{}{} ",
                throttled_status.result,
                chrono::Local::now().format("%H:%M:%S")
            )
        }
    }

    pub fn fill_text_buffer_when_shutting_down(text_buffer: &mut TextBuffer) {
        text_buffer.write_text_to_single_line("Ending screen driver".bytes(), LineNum::Line1);
        text_buffer.write_text_to_single_line("Computer not shut".bytes(), LineNum::Line3);
        text_buffer.write_text_to_single_line("down".bytes(), LineNum::Line4);
    }

    /// returns the volume as a String if playing, if not the gstreamer state as a String
    fn get_vol_string(status_of_rradio: &player_status::PlayerStatus) -> String {
        if status_of_rradio.gstreamer_state == gstreamer::State::Playing {
            format!(
                "Vol{:>Width$.Width$}",
                status_of_rradio.current_volume,
                Width = VOLUME_CHAR_COUNT - 3
            )
        } else {
            format!(
                "{:>width$.width$}",
                match status_of_rradio.gstreamer_state {
                    gstreamer::State::VoidPending => "Void",
                    gstreamer::State::Paused => "Paused",
                    gstreamer::State::Playing => "Playing",
                    gstreamer::State::Ready => "Ready",
                    gstreamer::State::Null => "Null",
                },
                width = VOLUME_CHAR_COUNT
            )
        }
    }

    /// gets the current date & time
    pub fn get_current_time_text() -> String {
        chrono::Local::now().format("%d %b %y %H:%M:%S").to_string()
    }

    /// Returns the temperature of the CPU followed by Wi-Fi signal strength.
    pub fn get_temperature_and_wifi_strength_text() -> String {
        format!(
            "CPU Temp {}C WiFi{}",
            get_temperature::get_cpu_temperature(),
            get_wifi_strength::get_wifi_signal_strength()
        )
    }

    /// writes text_buffer's contents to the LCD without translation
    pub fn write_text_buffer(&mut self, text_buffer: &TextBuffer) {
        for (line_number, line) in text_buffer
            .buffer
            .chunks(NUM_CHARACTERS_PER_LINE)
            .enumerate()
        {
            if let Err(err) = write!(self.lcd_file, "\x1b[Lx0y{line_number};") {
                // move the cursor to the start of the specified line
                println!("in write_text_buffer, Failed to write move the cursor : {err}");
                return;
            }
            if let Err(err) = self.lcd_file.write_all(line) {
                println!("in write_text_buffer, Failed to write text : {err}");
                return;
            }
        }
    }
}

/*
\f" will clear the display and put the cursor home.

"\x1b[LD" will enable the display, "\x1b[Ld" will disable it.
"\x1b[LC" will turn the cursor on, "\x1b[Lc" will turn it off.
"\x1b[LB" will enable blink. "\x1b[Lb" will disable it.
"\x1b[LL" will shift the display left. "\x1b[LR" will shift it right.
"\x1b[Ll" will shift the cursor left. "\x1b[Lr" will shift it right.
"\x1b[Lk" will erase the rest of the line.
"\x1b[LI" will initialise the display.
"\x1b[Lx001y001;" will move the cursor to character 001 of line 001. Use any other numbers for different positions. You can also use "\001;" and "\x1b[Ly001;" on their own.
"\x1b[LG0040a0400000000000;" will set up user defined character 00 as a "°" symbol. The first "0" is the character number to define (0-7) and the next 16 characters are hex values for the 8 bytes to define.

*/
