use std::time::Duration;

use futures_util::{FutureExt, StreamExt};
use tokio::sync::mpsc;

#[derive(Debug)]
/// An enum of all posssible outputs from the keyboard
pub enum Event {
    PlayPause,
    EjectCD,
    VolumeUp,
    VolumeDown,
    PreviousTrack,
    NextTrack,
    OutputStatusDebug,                     // output the status of rradio
    OutputConfigDebug,                     // output the config info
    OutputMountFolderContents,             // output the contents of the mount folders
    NewLineOnScreen,                       // output a blank line on the screen 
    PlayStation { channel_number: usize }, // channel_number will be  in the range "00" to "99", giving us the number of the station to play
}

/// puts the keyboard into raw mode & prepares it to return a series of keyboard events
pub fn setup_keyboard(
    input_timeout: Duration,
) -> tokio_stream::wrappers::UnboundedReceiverStream<Event> {
    let (events_tx, events_rx) = mpsc::unbounded_channel(); 
    // Create both ends of a message queue. The sender can be cloned, but the receiver cannot, hence MPSC (Multi-Producer, Single Consumer)

    tokio::spawn(
        async move {
            match crossterm::terminal::enable_raw_mode() {
                Ok(()) => (),
                Err(err) => {
                    eprintln!("Failed to enable raw mode in the terminal and got error {err}");
                    return;
                }
            }

            let mut stored_previous_digit_and_time: Option<(char, tokio::time::Instant)> = None; // store the previous digit entered;
            let mut keyboard_events = crossterm::event::EventStream::new();
            loop {
                // this loop matches keyboard events; other events are matched in a different task (& a different source file)
                match keyboard_events.next().await {
                    None => {
                        // no more keyboard events
                        println!("No more keyboard events\r");
                        break;
                    }
                    Some(Err(keyboard_error)) => {
                        eprintln!("Got keyboard error {keyboard_error}\r");
                        break;
                    }
                    Some(Ok(crossterm::event::Event::Key(key_event))) => {
                        // we got a keyboard event
                        let keyboard_event = match key_event.code {
                            // match to find out which key it is
                            crossterm::event::KeyCode::Char('Q' | 'q')
                            | crossterm::event::KeyCode::Backspace => break, // alternative termination key (crossterm intercepts Control C so we cannot use that to terminate)
                            crossterm::event::KeyCode::Enter => Event::PlayPause,
                            crossterm::event::KeyCode::Char('.') => Event::EjectCD,
                            crossterm::event::KeyCode::Char('*') => Event::VolumeUp,
                            crossterm::event::KeyCode::Char('/') => Event::VolumeDown,
                            crossterm::event::KeyCode::Char('-') => Event::PreviousTrack,
                            crossterm::event::KeyCode::Char('+') => Event::NextTrack,
                            crossterm::event::KeyCode::Char('!') => Event::OutputStatusDebug,
                            crossterm::event::KeyCode::Char('£') => Event::OutputConfigDebug,
                            crossterm::event::KeyCode::Char('$') => Event::OutputMountFolderContents,
                              crossterm::event::KeyCode::Char('^') => Event::NewLineOnScreen,
                         
                            
                            crossterm::event::KeyCode::Char(current_digit @ '0'..='9') => {
                                //the "@" symbol means make current_digit equal to the character that matched
                                match stored_previous_digit_and_time {
                                    //match if there is a previous digit & the elpased time is short enough
                                    Some((previous_digit, previous_digit_pressed_time))
                                        if previous_digit_pressed_time.elapsed()
                                            < input_timeout =>
                                    {
                                        let new_channel =
                                            format!("{}{}", previous_digit, current_digit)
                                                .parse::<usize>();
                                        Event::PlayStation {
                                            channel_number: new_channel.expect("When trying to turn 2 characters into a u8 it failed"),
                                        }
                                    }
                                    _ => {
                                        stored_previous_digit_and_time =
                                            Some((current_digit, tokio::time::Instant::now())); // Store both the current digit and the time it was pressed

                                        continue; // exit the current match statement & ignore all code in the rest of the loop & go round the loop again
                                    }
                                }
                            }
                            _ => continue,
                        };

                        stored_previous_digit_and_time = None; // sets both the previous digit & time to none

                        match events_tx.send(keyboard_event) {
                            Ok(()) => (),
                            Err(_) => break, // The receiver (IE the main program) has closed.
                        }
                    }
                    Some(Ok(_)) => continue, //it was not a key event,so ignore all the other unwanted cases
                }
            }
        }
        .then(|()| async {
            match crossterm::terminal::disable_raw_mode() {
                Ok(()) => (),
                Err(err) => {
                    eprintln!("Failed to disable raw mode in the terminal and got error {err}")
                }
            }
        }),
    );

    tokio_stream::wrappers::UnboundedReceiverStream::new(events_rx)
}
