//#![allow(unused_imports)] // TODO - Remove after refactor
#![allow(unused_variables)] // TODO - Remove after refactor

#[cfg(not(unix))]
compile_error!("You must compile this on linux");

use futures_util::StreamExt;
use get_channel_details::{get_channel_details, ChannelErrorEvents, SourceType};

use gstreamer::prelude::ElementExtManual;
use gstreamer_interfaces::PlaybinElement;
use lcd::{RunningStatus, ScrollData, TextBuffer};
use player_status::{PlayerStatus, PositionAndDuration};
use std::task::Poll;
use sys_mount::UnmountFlags;

mod get_channel_details;
mod gstreamer_interfaces;
mod keyboard;
mod lcd;
mod player_status;
mod read_config;

/*#[macro_export]
macro_rules! my_dbg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `eprintln!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `eprintln!`
    // will be malformed.
    () => {
        std::eprintln!("[{}:{}:{}]", std::file!(), std::line!(), std::column!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                std::eprintln!("[{}:{}:{}] {} = {:?}\r",
                std::file!(), std::line!(), std::column!(), std::stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::my_dbg!($val)),+,)
    };
}*/

/// An enum of all the types of event, each with their own event sub-type
#[derive(Debug)]
enum Event {
    Keyboard(keyboard::Event),
    GStreamer(gstreamer::Message),
    Ticker(tokio::time::Instant),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), String> {
    //    we need async as for example, we will need to wait for input from gstreamer or the keyboard
    let mut lcd;
    match lcd::Lc::new() {
        Ok(success) => lcd = success,
        Err(lcd_error) => {
            eprintln!("{lcd_error:?}");
            return Err("Could not open the LCD driver".to_string());
        }
    }

    let mut config_file_path_from_args = String::from("config2.toml"); // the default value if not specified
    let config_file_path = {
        let mut args = std::env::args().skip(1); //skip the name of the first executable
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-c" | "--config" => {
                    config_file_path_from_args = args.next().ok_or("the format is -c followed by the file name, but could not find the file name.")?;
                }
                "-V" | "--version" => {
                    println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                    return Ok(());
                }
                _ => {
                    let error_message = format!("Unhandled argument {arg:?}. The valid arguments are -c followed by the configuration file name OR - V");
                    let mut text_buffer: TextBuffer = TextBuffer::new();
                    text_buffer.write_abortive_error_message_to_entire_buffer(&error_message);
                    lcd.write_text_buffer(&text_buffer);
                    return Err(error_message);
                }
            }
        }
        config_file_path_from_args
    };
    println!("Reading from config path = {config_file_path}\r");

    let mut toml_error: Option<String> = None;

    let config = read_config::Config::from_file(&config_file_path).unwrap_or_else(|error| {
        toml_error = Some(error);
        //let mut text_buffer = TextBuffer::new();
        //text_buffer.write_abortive_error_message_to_entire_buffer(&error);
        //lcd.write_text_buffer(&text_buffer);
        //eprintln!("{:?}", error);
        read_config::Config::default()
    });
    println!("conf {:?}", config);
    let mut status_of_rradio: PlayerStatus = PlayerStatus::new(&config);
    if let Some(toml_error_message) = toml_error {
        status_of_rradio.toml_error = Some(toml_error_message);
    }

    if let Ok(_worked_ok) = gstreamer::init() {
    } else {
        status_of_rradio.all_4lines = ScrollData::new("Failed it to intialise gstreamer", 4);
        status_of_rradio.running_status = lcd::RunningStatus::BadErrorMessage;
        lcd.fill_text_buffer_and_write_to_lcd(&status_of_rradio);
    };

    match gstreamer_interfaces::PlaybinElement::setup(&config) {
        Ok((mut playbin, bus_stream)) => {
            println!("playbin{:?}    bus stream{:?}", playbin, bus_stream);

            // if Some(filename) can match config.aural_notifications.filename_startup then execute the block
            if let Some(filename) = config.aural_notifications.filename_startup.clone() {
                status_of_rradio.channel_file_data.station_url = vec![format!("file://{filename}")];
                if let Err(error_message) = playbin.play_track(&status_of_rradio) {
                    status_of_rradio.all_4lines = ScrollData::new(error_message.as_str(), 4);
                    lcd.fill_text_buffer_and_write_to_lcd(&status_of_rradio);
                }
            } else {
                println!("No startup ding wanted");
            }

            let keyboard_events = keyboard::setup_keyboard(config.input_timeout);

            //Map the different stream item types (such as `keyboard::Event` and `gstreamer::Message`) into a common stream item type (i.e. Event)
            //We need a common event type in order to merge several sources of events and handle whichever event occurs first, no matter the source.
            //This is needed for the loop statement that follows.
            let mut mapped_keyboard_events = keyboard_events.map(Event::Keyboard);
            let mut mapped_playbin_message_bus = bus_stream.map(Event::GStreamer);
            change_volume(
                0, //zero as the direction outputs the volume, but does not change it
                &config,
                &mut status_of_rradio,
                &mut playbin,
            );

            let mut some_timer = tokio_stream::wrappers::IntervalStream::new(
                tokio::time::interval(std::time::Duration::from_millis(3000)),
            )
            .map(Event::Ticker);

            let start_time = tokio::time::Instant::now();

            loop {
                let event = std::future::poll_fn(|cx| {
                    //First poll the keyboard events source for keyboard events
                    match mapped_keyboard_events.poll_next_unpin(cx) {
                        //std::future::poll_fn strips off "Poll::Ready", so if there is a keyboard event, the next line becomes "event = keyboard_event"
                        Poll::Ready(keyboard_event) => return Poll::Ready(keyboard_event),
                        Poll::Pending => (), //if the match gives Pending, which means that so far event has not been made equal to anything.
                    };

                    //Then poll the GStreamer playbin events source for gstreamer events
                    match mapped_playbin_message_bus.poll_next_unpin(cx) {
                        //std::future::poll_fn strips off "Poll::Ready", so if there is a playbin event, the next line becomes "event = playbin_event"
                        Poll::Ready(playbin_event) => return Poll::Ready(playbin_event),
                        Poll::Pending => (), //if the match gives Pending, which means that so far event has not been made equal to anything.
                    };

                    match some_timer.poll_next_unpin(cx) {
                        Poll::Ready(playbin_event) => return Poll::Ready(playbin_event),
                        Poll::Pending => (),
                    }

                    //No event sources are ready, notify we're awaiting, i.e. pending, incoming events.
                    // poll_fn calls this code block the next time it's awoken, not in a CPU intensive tight loop.
                    Poll::Pending //this is the return value event is made equal to Poll::Pending.
                })
                .await;

                //Now that we have an event, work out what to do with it
                match event {
                    None => {
                        let _unmount_result = unmount_if_needed(&config, &mut status_of_rradio);
                        status_of_rradio.running_status = lcd::RunningStatus::ShuttingDown;
                        lcd.clear(); // we are ending the program if we get to here
                        lcd.fill_text_buffer_and_write_to_lcd(&status_of_rradio);
                        break; // if we get here, the program will terminate
                    } //One of the streams has closed, signalling a shutdown of the program, so break out of the main loop
                    Some(Event::Keyboard(keyboard_event)) => match keyboard_event {
                        keyboard::Event::PlayPause => {
                            let new_state =
                                if status_of_rradio.gstreamer_state == gstreamer::State::Playing {
                                    gstreamer::State::Paused
                                } else {
                                    gstreamer::State::Playing
                                };
                            if let Err(_error_message) = playbin.set_state(new_state) {
                                eprintln!(
                                    "Could not set the gstreamer state when user hit play//pause"
                                )
                            }
                        }
                        keyboard::Event::EjectCD => {
                            eprintln!(
                                "eject result {:?}\r",
                                get_channel_details::cd_functions::eject()
                            );
                        }
                        keyboard::Event::VolumeUp => {
                            change_volume(1, &config, &mut status_of_rradio, &mut playbin);
                        }
                        keyboard::Event::VolumeDown => {
                            change_volume(-1, &config, &mut status_of_rradio, &mut playbin);
                        }
                        keyboard::Event::PreviousTrack => {
                            println!("PreviousTrack\r");
                            status_of_rradio.index_to_current_track = (status_of_rradio
                                .index_to_current_track
                                + status_of_rradio.channel_file_data.station_url.len()
                                - 1)
                                % status_of_rradio.channel_file_data.station_url.len(); // % is a remainder operator not modulo
                            if let Err(playbin_error_message) =
                                playbin.play_track(&status_of_rradio)
                            {
                                status_of_rradio.all_4lines = ScrollData::new(
                                    format!("When playing a track got {playbin_error_message}")
                                        .as_str(),
                                    4,
                                );
                                status_of_rradio.running_status = RunningStatus::BadErrorMessage;
                            } else {
                                status_of_rradio.line_2_data.update_if_changed(
                                    status_of_rradio.channel_file_data.organisation.as_str(),
                                );
                            }
                        }
                        keyboard::Event::NextTrack => {
                            next_track(&mut status_of_rradio, &playbin);
                        }
                        keyboard::Event::PlayStation { channel_number } => {
                            if status_of_rradio.previous_channel_number
                                == status_of_rradio.channel_number
                            {
                                status_of_rradio.previous_channel_number =
                                    status_of_rradio.channel_number;
                                status_of_rradio.position_and_duration
                                    [status_of_rradio.index_to_current_track] =
                                    PositionAndDuration {
                                        position_ms: 0,
                                        duration_ms: None,
                                    }
                            }

                            status_of_rradio.channel_number = channel_number;
                            match get_channel_details::get_channel_details(
                                config.stations_directory.clone(),
                                channel_number,
                                &config,
                                &mut status_of_rradio,
                            ) {
                                Ok(channel_file_data) => {
                                    status_of_rradio.toml_error = None; // clear out the toml error if there is one
                                    status_of_rradio.running_status =
                                        lcd::RunningStatus::RunningNormally;
                                    println!(
                                        "returned source type {:?}\r",
                                        channel_file_data.source_type
                                    );

                                    /*if (status_of_rradio.channel_file_data.source_type
                                        != channel_file_data.source_type)
                                        | !((channel_file_data.source_type != SourceType::Usb)
                                            || (channel_file_data.source_type != SourceType::CD))
                                    {
                                        println!("unmounting\r ");
                                        if let Err(error_message) =
                                            unmount_if_needed(&config, &mut status_of_rradio)
                                        {
                                            eprintln!(
                                                "When unmounting, got error {}",
                                                error_message
                                            )
                                        };
                                    };*/

                                    status_of_rradio.channel_file_data = channel_file_data;
                                    status_of_rradio.artist = String::new();

                                    if let Err(playbin_error_message) =
                                        playbin.play_track(&status_of_rradio)
                                    {
                                        status_of_rradio.all_4lines = ScrollData::new(
                                            format!(
                                                "When playing a track got {playbin_error_message}"
                                            )
                                            .as_str(),
                                            4,
                                        );
                                        status_of_rradio.running_status =
                                            RunningStatus::BadErrorMessage;
                                    } else {
                                        status_of_rradio.line_2_data.update_if_changed(
                                            status_of_rradio
                                                .channel_file_data
                                                .organisation
                                                .as_str(),
                                        );
                                    }
                                }
                                Err(the_error) => {
                                    println!("got channel detail error {:?}\r", &the_error);

                                    if let ChannelErrorEvents::CouldNotFindChannelFile = the_error {
                                        if (status_of_rradio.running_status
                                            == lcd::RunningStatus::NoChannel)
                                            && (status_of_rradio.channel_number
                                                == status_of_rradio.previous_channel_number)
                                        {
                                            status_of_rradio.running_status =
                                                lcd::RunningStatus::NoChannelRepeated;
                                        } else {
                                            status_of_rradio.running_status =
                                                lcd::RunningStatus::NoChannel;
                                        }

                                        if let Some(ding_filename) =
                                            &config.aural_notifications.filename_error
                                        {
                                            // play a ding if one has been specified
                                            status_of_rradio.channel_file_data.station_url =
                                                vec![format!("file://{ding_filename}")];
                                            let _ignore_error_if_beep_fails =
                                                playbin.play_track(&status_of_rradio);
                                        }
                                    } else {
                                        status_of_rradio.all_4lines =
                                            ScrollData::new(the_error.to_lcd_screen().as_str(), 4);
                                        status_of_rradio.running_status =
                                            RunningStatus::BadErrorMessage;
                                    };
                                    //status_of_rradio.initialise_for_new_station();
                                    //do not remember we played a ding
                                }
                            }
                            println!("status_of_rradio {:?}\r", status_of_rradio);
                        }
                    },
                    Some(Event::GStreamer(gstreamer_message)) => {
                        // println!("got gstreamer message{:?}\r", gstreamer_message);
                        use gstreamer::MessageView;
                        // let yy = gstreamer_message.view();
                        // println!("yy{:?}\r", yy);

                        match gstreamer_message.view() {
                            MessageView::Buffering(buffering) => {
                                status_of_rradio.buffering_percent = buffering.percent()
                            }

                            MessageView::Tag(tag) => {
                                for (tag_name, tag_value) in tag.tags().iter() {
                                    //println!("tag_name{tag_name:?} {tag_value:?} \r");
                                    match tag_name.as_str() {
                                        "title" => {
                                            if let Ok(title) = tag_value.get::<&str>() {
                                                if status_of_rradio.album != title {
                                                    status_of_rradio
                                                        .line_34_data
                                                        .update_if_changed(title);
                                                    println!("got new album OR IS IT A TITLE!!! {title:?} {:?}\r", status_of_rradio.line_34_data);
                                                }
                                            }
                                        }
                                        "organization" => {
                                            if let Ok(organization) = tag_value.get::<&str>() {
                                                if status_of_rradio.channel_file_data.organisation
                                                    != organization
                                                {
                                                    status_of_rradio
                                                        .channel_file_data
                                                        .organisation = organization.to_string();
                                                    status_of_rradio
                                                        .line_2_data
                                                        .update_if_changed(organization);
                                                    println!("got new organization!!! {organization:?}\r")
                                                }
                                            }
                                        }
                                        "artist" => {
                                            if let Ok(artist) = tag_value.get::<&str>() {
                                                if status_of_rradio.artist != artist {
                                                    status_of_rradio.artist = artist.to_string();
                                                    println!("got new artist!!! {artist:?}\r")
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            MessageView::StateChanged(state_changed) => {
                                if state_changed.src().is_some_and(|state_changed_source| {
                                    *state_changed_source == playbin.playbin_element
                                    // we only want stage changes from playbin0
                                }) {
                                    status_of_rradio.gstreamer_state = state_changed.current();
                                    println!(
                                        "statechanged {:?}\r",
                                        status_of_rradio.gstreamer_state
                                    );
                                    change_volume(0, &config, &mut status_of_rradio, &mut playbin);
                                }
                            }

                            MessageView::Eos(_end_of_stream) => {
                                if status_of_rradio.channel_file_data.station_url.len() > 1 {
                                    next_track(&mut status_of_rradio, &playbin);
                                }
                            }

                            MessageView::Error(error) => {
                                println!("the error {:?}\r", error)
                            }

                            _ => {}
                        }

                        //if let Some(gst_message_buffering) = gstreamer_message.structure() {
                        //     println!("gst_message_buffering{:?}\r", gst_message_buffering);
                        //} else {
                        //    println!("got a gstreamer_mesage but no strcture\r");
                        //}
                    }
                    Some(Event::Ticker(now)) => {
                        if let Some(position_ms) = playbin
                            .playbin_element
                            .query_position::<gstreamer::ClockTime>()
                            .map(gstreamer::ClockTime::mseconds)
                        {
                            status_of_rradio.position_and_duration
                                [status_of_rradio.index_to_current_track] = PositionAndDuration {
                                position_ms,
                                duration_ms: playbin
                                    .playbin_element
                                    .query_duration() // it is an Option as infinite streams do not have a duration
                                    .map(gstreamer::ClockTime::mseconds),
                            };
                        } // else if there is no position we cannot do anything useful
                    }
                }
                lcd.fill_text_buffer_and_write_to_lcd(&status_of_rradio);
            } // closing parentheses of loop
        }
        Err(message) => {
            status_of_rradio.all_4lines =
                ScrollData::new(format!("Failed to get a playbin: {message}").as_str(), 4);
            status_of_rradio.running_status = RunningStatus::BadErrorMessage;
            lcd.fill_text_buffer_and_write_to_lcd(&status_of_rradio);
        }
    }

    Ok(()) //as at the start we said it returned either "Ok(())" or an error, as nothing has failed, we give the "all worked OK termination" value
}

/// plays the next track by modulo incrementing status_of_rradio.index_to_current_track
fn next_track(status_of_rradio: &mut PlayerStatus, playbin: &PlaybinElement) {
    status_of_rradio.index_to_current_track = (status_of_rradio.index_to_current_track + 1)
        % status_of_rradio.channel_file_data.station_url.len();
    if let Err(playbin_error_message) = playbin.play_track(status_of_rradio) {
        status_of_rradio.all_4lines = ScrollData::new(
            format!("When playing a track got {playbin_error_message}").as_str(),
            4,
        );
        status_of_rradio.running_status = RunningStatus::BadErrorMessage;
    } else {
        status_of_rradio
            .line_2_data
            .update_if_changed(status_of_rradio.channel_file_data.organisation.as_str());
    }
}

/// Changes the volume by config.volume_offset dB up or down as controlled by "direction".
/// Checks are made that the volume remains in bounds.
fn change_volume(
    direction: i32,
    config: &read_config::Config,
    status_of_rradio: &mut player_status::PlayerStatus,
    playbin: &mut PlaybinElement,
) {
    assert! ((direction ==1) || (direction == -1) || (direction == 0), "direction must be plus or minus 1 to change the volume, or zero to merely output the current volume");
    status_of_rradio.current_volume =
        (status_of_rradio.current_volume + config.volume_offset * direction).clamp(
            gstreamer_interfaces::VOLUME_MIN,
            gstreamer_interfaces::VOLUME_MAX,
        );
    if let Err(error_message) = playbin.set_volume(status_of_rradio.current_volume) {
        eprintln!("When changing the volume got error {}\r", error_message);
    }
}

/// unmounts whatever device is mounted if the mount folder; returns an eror string if it fails
fn unmount_if_needed(
    config: &read_config::Config,
    status_of_rradio: &mut player_status::PlayerStatus,
) -> Result<(), String> {
    if let Some(usb) = &config.usb {
        if status_of_rradio.usb_is_mounted {
            if let Err(error_message) = sys_mount::unmount(&usb.mount_folder, UnmountFlags::DETACH)
            {
                return Err(format!(
                    "Failed to unmount the device mounted on {}",
                    usb.mount_folder
                ));
            } else {
                status_of_rradio.usb_is_mounted = false
            };
        }
        Ok(())
    } else {
        Ok(())
    }
}
