//#![allow(unused_imports)] // TODO - Remove after refactor
//#![allow(unused_variables)] // TODO - Remove after refactor

#[cfg(not(unix))]
compile_error!("You must compile this on linux");

use chrono::TimeDelta;
use futures_util::StreamExt;
use get_channel_details::{ChannelErrorEvents, SourceType};
use gstreamer::{SeekFlags, prelude::ElementExtManual};
use gstreamer_interfaces::PlaybinElement;
use player_status::PlayerStatus;

mod cd_functions;
pub mod mount_samba;
pub mod mount_usb;

use std::task::Poll;

use crate::{
    get_channel_details::store_channel_details_and_implement_them,
    lcd::get_local_ip_address,
    ping::{get_ping_time, see_if_there_is_a_ping_response},
    player_status::NUMBER_OF_POSSIBLE_CHANNELS,
    unmount::unmount_if_needed,
};
use lcd::{RunningStatus, ScrollData, TextBuffer};

mod get_channel_details;
mod get_config_file_path;
mod gstreamer_interfaces;
mod keyboard;
mod lcd;
mod ping;
mod player_status;
mod read_config;
mod unmount;

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
            return Err(format!(
                "Could not open the LCD driver. Got error {}",
                lcd_error
            ));
        }
    }

    let mut config_file_path = "config.toml".to_string(); // the default file name of the config TOML file
    let root_folder;

    if let Some(path) = std::env::args().next()
        && let Some(position) = path.rfind("/")
    {
        root_folder = path[0..position + 1].to_string();
        config_file_path = format!("{}{}", root_folder, config_file_path);
    } else {
        root_folder = String::new();
    }

    let mut toml_error: Option<String> = None; // a temporary store of the master store; we need a temporary store as we cannot create status_of_rradio until we have read the config file
    match get_config_file_path::get_config_file_path(&config_file_path) {
        Ok(new_path) => config_file_path = new_path,
        Err(error_message) => {
            eprintln!("{}", error_message);
            toml_error = Some(error_message);
        }
    }

    let config = read_config::Config::from_file(&config_file_path).unwrap_or_else(|error| {
        toml_error = Some(error);
        read_config::Config::default()
    });

    let mut status_of_rradio: PlayerStatus = PlayerStatus::new(&config);
    status_of_rradio.startup_folder = root_folder;
    if let Some(toml_error_message) = toml_error {
        // if we got an error we should display it; hopefully, toml_error == none
        status_of_rradio.toml_error = Some(toml_error_message);
    }

    read_config::insert_samba(&config, &mut status_of_rradio);
    // first assume that the WiFi is working and has a valid SSID & Password
    status_of_rradio.update_network_data(&mut lcd, &config);

    if !status_of_rradio.network_data.is_valid {
        match lcd::get_local_ip_address::set_up_wifi_password(
            &mut status_of_rradio,
            &mut lcd,
            &config,
        ) {
            Ok(()) => {}
            Err(error_message) => {
                status_of_rradio
                    .all_4lines
                    .update_if_changed(error_message.as_str());
            }
        };
    }

    if gstreamer::init().is_err() {
        status_of_rradio.all_4lines = ScrollData::new("Failed it to intialise gstreamer", 4);
        status_of_rradio.running_status = lcd::RunningStatus::LongMessageOnAll4Lines;
        lcd.write_rradio_status_to_lcd(&status_of_rradio, &config);
    };

    status_of_rradio.line_1_data = ScrollData::new(
        format!(
            "{} {}",
            status_of_rradio.network_data.local_ip_address,
            lcd::Lc::get_vol_string(&status_of_rradio)
        )
        .as_str(),
        1,
    );

    match gstreamer_interfaces::PlaybinElement::setup(&config) {
        Ok((mut playbin, bus_stream)) => {
            /*println!("playbin{:?}    bus stream{:?}", playbin, bus_stream);  */
            if let Some(filename) = config.aural_notifications.filename_startup.clone() {
                status_of_rradio.channel_number = player_status::START_UP_DING_CHANNEL_NUMBER;

                status_of_rradio.position_and_duration
                    [player_status::START_UP_DING_CHANNEL_NUMBER]
                    .channel_data
                    .station_urls = vec![format!("file://{filename}")];
                status_of_rradio.position_and_duration
                    [player_status::START_UP_DING_CHANNEL_NUMBER]
                    .channel_data
                    .source_type = SourceType::UrlList;
                if let Err(error_message) =
                    playbin.play_track(&mut status_of_rradio, &config, &mut lcd, false)
                {
                    status_of_rradio.all_4lines = ScrollData::new(error_message.as_str(), 4);
                    lcd.write_rradio_status_to_lcd(&status_of_rradio, &config);
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
                0, // if direction == 0 it gets the volume, but does not change it
                &config,
                &mut status_of_rradio,
                &mut playbin,
            );

            let mut some_timer = tokio_stream::wrappers::IntervalStream::new(
                tokio::time::interval(std::time::Duration::from_millis(300)),
            )
            .map(Event::Ticker);

            //let g= get_local_ip_address::set_up_wifi_password(&mut status_of_rradio, &mut lcd, &config);

            let mut child_ping = ping::send_ping(&mut status_of_rradio, &config);

            loop {
                if status_of_rradio.ping_data.can_send_ping {
                    //we must get the output
                    if let Err(error) =
                        get_ping_time(child_ping.wait_with_output(), &mut status_of_rradio)
                    {
                        eprintln!("Got ping error {error}\r")
                    };
                    child_ping = ping::send_ping(&mut status_of_rradio, &config);
                } else {
                    see_if_there_is_a_ping_response(&mut status_of_rradio);
                }

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
                        // we are ending the program if we get to here
                        if let Some(usb) = &config.usb
                            && let Err(message) = &unmount_if_needed(
                                &usb.local_mount_folder,
                                &mut status_of_rradio.usb_mounted,
                            )
                        {
                            eprintln!("Failed to unmount the local USB drive {}\r", message)
                        }

                        if let Some(samba) = &config.samba
                            && let Err(message) = &unmount_if_needed(
                                &samba.remote_mount_folder,
                                &mut status_of_rradio.samba_mounted,
                            )
                        {
                            eprintln!("Failed to unmount the remote USB drive {}\r", message)
                        }

                        status_of_rradio.running_status = lcd::RunningStatus::ShuttingDown;
                        lcd.clear(); // we are ending the program if we get to here
                        lcd.write_rradio_status_to_lcd(&status_of_rradio, &config);
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
                                    "Could not set the gstreamer state when user hit play//pause\r"
                                )
                            }
                        }
                        keyboard::Event::EjectCD => {
                            eprintln!("eject result {:?}\r", cd_functions::eject());
                        }
                        keyboard::Event::VolumeUp => {
                            change_volume(1, &config, &mut status_of_rradio, &mut playbin);
                            status_of_rradio.line_1_data.update_if_changed(
                                format!(
                                    "{} {}",
                                    status_of_rradio.network_data.local_ip_address,
                                    lcd::Lc::get_vol_string(&status_of_rradio)
                                )
                                .as_str(),
                            );
                        }
                        keyboard::Event::VolumeDown => {
                            change_volume(-1, &config, &mut status_of_rradio, &mut playbin);
                            status_of_rradio.line_1_data.update_if_changed(
                                format!(
                                    "{} {}",
                                    status_of_rradio.network_data.local_ip_address,
                                    lcd::Lc::get_vol_string(&status_of_rradio)
                                )
                                .as_str(),
                            );
                        }
                        keyboard::Event::PreviousTrack => {
                            println!("PreviousTrack\r");
                            status_of_rradio.initialise_for_new_station();
                            if status_of_rradio.position_and_duration
                                [status_of_rradio.channel_number]
                                .position
                                > TimeDelta::milliseconds(config.goto_previous_track_time_delta)
                            {
                                // We have been playing for some time, so seek the start of the track
                                let _ = playbin.playbin_element.seek_simple(
                                    SeekFlags::FLUSH
                                        | SeekFlags::KEY_UNIT
                                        | SeekFlags::SNAP_NEAREST,
                                    gstreamer::ClockTime::ZERO,
                                );
                            } else {
                                // we have only just started, so user wants the previous track
                                status_of_rradio.position_and_duration
                                    [status_of_rradio.channel_number]
                                    .index_to_current_track = (status_of_rradio
                                    .position_and_duration[status_of_rradio.channel_number]
                                    .index_to_current_track
                                    + status_of_rradio.position_and_duration
                                        [status_of_rradio.channel_number]
                                        .channel_data
                                        .station_urls
                                        .len()
                                    - 1)
                                    % status_of_rradio.position_and_duration
                                        [status_of_rradio.channel_number]
                                        .channel_data
                                        .station_urls
                                        .len(); // % is a remainder operator not modulo
                                if let Err(playbin_error_message) = playbin.play_track(
                                    &mut status_of_rradio,
                                    &config,
                                    &mut lcd,
                                    false,
                                ) {
                                    status_of_rradio.all_4lines.update_if_changed(
                                    format!("When wanting to play the previous track got {playbin_error_message}")
                                        .as_str(),
                                );
                                    status_of_rradio.running_status =
                                        RunningStatus::LongMessageOnAll4Lines;
                                } else {
                                    status_of_rradio.line_2_data.update_if_changed(
                                        status_of_rradio.position_and_duration
                                            [status_of_rradio.channel_number]
                                            .channel_data
                                            .organisation
                                            .as_str(),
                                    );
                                }
                            }
                        }
                        keyboard::Event::NextTrack => {
                            status_of_rradio.ping_data.number_of_pings_to_this_channel = 0;
                            next_track(&mut status_of_rradio, &playbin, &config, &mut lcd);
                        }
                        keyboard::Event::PlayStation { channel_number } => {
                            status_of_rradio.initialise_for_new_station();

                            if channel_number == status_of_rradio.channel_number
                                && status_of_rradio.running_status == RunningStatus::NoChannel
                            {
                                status_of_rradio.running_status = RunningStatus::NoChannelRepeated;
                            } else {
                                status_of_rradio.running_status = RunningStatus::RunningNormally;
                                status_of_rradio.line_2_data.update_if_changed("");
                                status_of_rradio.line_34_data.update_if_changed("");
                                status_of_rradio.all_4lines.update_if_changed("");
                                let previous_channel_number = status_of_rradio.channel_number;
                                status_of_rradio.channel_number = channel_number;

                                if let Err(the_channel_error_events) =
                                    store_channel_details_and_implement_them(
                                        &config,
                                        &mut status_of_rradio,
                                        &playbin,
                                        previous_channel_number,
                                        &mut lcd,
                                    )
                                {
                                    match the_channel_error_events {
                                        ChannelErrorEvents::CouldNotFindChannelFile => {
                                            status_of_rradio.toml_error = None; // clear the TOML error out, the user must have seen it by now
                                            status_of_rradio.running_status =
                                                if previous_channel_number == channel_number {
                                                    RunningStatus::NoChannelRepeated
                                                } else {
                                                    RunningStatus::NoChannel
                                                };
                                            if let Some(ding_filename) =
                                                &config.aural_notifications.filename_error
                                            {
                                                // play a ding if one has been specified
                                                status_of_rradio.position_and_duration
                                                    [player_status::START_UP_DING_CHANNEL_NUMBER]
                                                    .channel_data
                                                    .station_urls =
                                                    vec![format!("file://{ding_filename}")];
                                                let _ignore_error_if_beep_fails = playbin
                                                    .play_track(
                                                        &mut status_of_rradio,
                                                        &config,
                                                        &mut lcd,
                                                        false,
                                                    );
                                                status_of_rradio.position_and_duration
                                                    [player_status::START_UP_DING_CHANNEL_NUMBER]
                                                    .index_to_current_track = 0;
                                            }
                                        }
                                        ChannelErrorEvents::CouldNotParseChannelFile {
                                            channel_number,
                                            error_message,
                                        } => {
                                            status_of_rradio.toml_error = Some(format!(
                                                "Could not parse channel {channel_number}. {}",
                                                error_message.replace("\n", " ")
                                            ));
                                        }

                                        _ => {
                                            status_of_rradio.all_4lines.update_if_changed(
                                                the_channel_error_events.to_lcd_screen().as_str(),
                                            );
                                            status_of_rradio.running_status =
                                                RunningStatus::LongMessageOnAll4Lines;
                                        }
                                    }
                                }
                            }
                            if let Err(playbin_error_message) =
                                playbin.play_track(&mut status_of_rradio, &config, &mut lcd, true)
                            {
                                status_of_rradio.all_4lines.update_if_changed(
                                    format!("When playing a track on channel {} got {playbin_error_message}", status_of_rradio.channel_number)
                                        .as_str());
                                status_of_rradio.running_status =
                                    RunningStatus::LongMessageOnAll4Lines;
                            } else {
                                // play worked
                                let line2 = generate_line2(&status_of_rradio);
                                status_of_rradio
                                    .line_2_data
                                    .update_if_changed(line2.as_str())
                            }
                        }
                        keyboard::Event::OutputStatusDebug => {
                            status_of_rradio.output_rradio();
                        }
                        keyboard::Event::OutputConfigDebug => {
                            status_of_rradio.output_config_information(&config);
                        }
                        keyboard::Event::OutputMountFolderContents => {
                            status_of_rradio.output_mount_folder_contents(&config)
                        }
                    },

                    Some(Event::GStreamer(gstreamer_message)) => {
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
                                                status_of_rradio
                                                    .line_34_data
                                                    .update_if_changed(title);
                                            }
                                        }
                                        "organization" => {
                                            if let Ok(mut organization) = tag_value.get::<&str>() {
                                                if organization == "LaPremiere" {
                                                    organization = "La Première"
                                                    // correct the name of the station
                                                }

                                                if status_of_rradio.position_and_duration
                                                    [status_of_rradio.channel_number]
                                                    .channel_data
                                                    .organisation
                                                    != organization
                                                {
                                                    status_of_rradio.position_and_duration
                                                        [status_of_rradio.channel_number]
                                                        .channel_data
                                                        .organisation = organization.to_string();
                                                    status_of_rradio
                                                        .line_2_data
                                                        .update_if_changed(organization);
                                                    println!(
                                                        "got new organization!!! {organization:?}\r"
                                                    )
                                                }
                                            }
                                        }
                                        "artist" => {
                                            if let Ok(artist) = tag_value.get::<&str>()
                                                && status_of_rradio.position_and_duration
                                                    [status_of_rradio.channel_number]
                                                    .artist
                                                    != artist
                                            {
                                                status_of_rradio.position_and_duration
                                                    [status_of_rradio.channel_number]
                                                    .artist = artist.to_string();
                                                println!("got new artist!!! {artist:?}\r")
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                let line2 = generate_line2(&status_of_rradio);
                                status_of_rradio
                                    .line_2_data
                                    .update_if_changed(line2.as_str());
                            }

                            MessageView::StateChanged(state_changed) => {
                                if state_changed.src().is_some_and(|state_changed_source| {
                                    *state_changed_source == playbin.playbin_element
                                    // we only want stage changes from playbin0
                                }) {
                                    status_of_rradio.gstreamer_state = state_changed.current();
                                    change_volume(0, &config, &mut status_of_rradio, &mut playbin);
                                }
                            }

                            MessageView::Eos(_end_of_stream) => {
                                if status_of_rradio.position_and_duration
                                    [status_of_rradio.channel_number]
                                    .channel_data
                                    .station_urls
                                    .len()
                                    > 1
                                {
                                    next_track(&mut status_of_rradio, &playbin, &config, &mut lcd);
                                }
                            }

                            MessageView::Error(gstreamer_error) => {
                                let mut output_message =
                                    format!("gstreamer_error {:?}", gstreamer_error);
                                if let Some(message) = gstreamer_error.message().structure() {
                                    let formatted_message = format!("{:?}", message);
                                    if formatted_message.contains("No such file") {
                                        output_message = formatted_message;
                                    }
                                }
                                println!("gstreamer error {}\r", output_message);
                                status_of_rradio.all_4lines =
                                    ScrollData::new(output_message.as_str(), 4);
                                status_of_rradio.running_status =
                                    RunningStatus::LongMessageOnAll4Lines;
                            }

                            _ => {}
                        }
                    }
                    Some(Event::Ticker(_now)) => {
                        if status_of_rradio.channel_number
                            <= player_status::NUMBER_OF_POSSIBLE_CHANNELS
                            && let Some(position_ms) = playbin
                                .playbin_element
                                .query_position::<gstreamer::ClockTime>()
                                .map(gstreamer::ClockTime::mseconds)
                            && let Ok(position_i64) = i64::try_from(position_ms)
                        {
                            status_of_rradio.position_and_duration
                                [status_of_rradio.channel_number]
                                .position = TimeDelta::milliseconds(position_i64);
                            status_of_rradio.position_and_duration
                                [status_of_rradio.channel_number]
                                .duration_ms = playbin
                                .playbin_element
                                .query_duration()
                                .map(gstreamer::ClockTime::mseconds);
                        } // else if there is no position we cannot do anything useful
                    }
                }
                status_of_rradio
                    .line_1_data
                    .update_scroll(&config, lcd::NUM_CHARACTERS_PER_LINE);
                status_of_rradio
                    .line_2_data
                    .update_scroll(&config, lcd::NUM_CHARACTERS_PER_LINE);

                let space_needed_for_buffer =
                    if status_of_rradio.channel_number <= NUMBER_OF_POSSIBLE_CHANNELS {
                        if status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                            .channel_data
                            .source_type
                            == SourceType::UrlList
                        {
                            3 // we need space to display the buffer
                        } else {
                            0 // we do not need space as it is not a URL list
                        }
                    } else {
                        0 // we do not need space
                    };
                status_of_rradio.line_34_data.update_scroll(
                    &config,
                    lcd::NUM_CHARACTERS_PER_LINE * 2 - space_needed_for_buffer,
                );
                status_of_rradio
                    .all_4lines
                    .update_scroll(&config, lcd::NUM_CHARACTERS_PER_LINE * 4);
                lcd.write_rradio_status_to_lcd(&status_of_rradio, &config);
            } // closing parentheses of loop
        }
        Err(message) => {
            status_of_rradio
                .all_4lines
                .update_if_changed(format!("Failed to get a playbin: {message}").as_str());
            status_of_rradio.running_status = RunningStatus::LongMessageOnAll4Lines;
            lcd.write_rradio_status_to_lcd(&status_of_rradio, &config);
        }
    }

    Ok(()) //as at the start we said it returned either "Ok(())" or an error, as nothing has failed, we give the "all worked OK termination" value
}

/// Generates the text for line 2 for the nornmal running case, ie streaming, USB or CD. Adds the throttled state if the Pi is throttled
pub fn generate_line2(status_of_rradio: &PlayerStatus) -> String {
    let mut line2 = match status_of_rradio.position_and_duration[status_of_rradio.channel_number]
        .channel_data
        .source_type
    {
        SourceType::Cd => {
            let mut num_tracks = status_of_rradio.position_and_duration
                [status_of_rradio.channel_number]
                .channel_data
                .station_urls
                .len();
            if status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .last_track_is_a_ding
            {
                num_tracks -= 1
            }
            format!(
                "CD track {} of {}",
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .index_to_current_track
                    + 1, // +1 as humans start counting at 1, not zero
                num_tracks
            )
        }
        SourceType::Usb | SourceType::Samba => {
            let mut num_tracks = status_of_rradio.position_and_duration
                [status_of_rradio.channel_number]
                .channel_data
                .station_urls
                .len();
            if status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .last_track_is_a_ding
            {
                num_tracks -= 1
            }
            format!(
                "{} ({} of {})",
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .channel_data
                    .organisation,
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .index_to_current_track
                    + 1, // +1 as humans start counting at 1, not zero
                num_tracks
            )
        }
        SourceType::UrlList => status_of_rradio.position_and_duration
            [status_of_rradio.channel_number]
            .channel_data
            .organisation
            .to_string(),
        SourceType::UnknownSource => "Unnown source type".to_string(),
    };
    let throttled_status = lcd::get_throttled::is_throttled();
    if throttled_status.pi_is_throttled {
        line2 = format!("{line2} {}", throttled_status.result)
    };
    line2
}

/// Plays the next track by modulo incrementing status_of_rradio.index_to_current_track
fn next_track(
    status_of_rradio: &mut PlayerStatus,
    playbin: &PlaybinElement,
    config: &crate::read_config::Config,
    lcd: &mut crate::lcd::Lc,
) {
    status_of_rradio.running_status = RunningStatus::RunningNormally; // at least hope that this is true
    status_of_rradio.position_and_duration[status_of_rradio.channel_number]
        .index_to_current_track = (status_of_rradio.position_and_duration
        [status_of_rradio.channel_number]
        .index_to_current_track
        + 1)
        % status_of_rradio.position_and_duration[status_of_rradio.channel_number]
            .channel_data
            .station_urls
            .len();
    if let Err(playbin_error_message) = playbin.play_track(status_of_rradio, config, lcd, false) {
        status_of_rradio.all_4lines.update_if_changed(
            format!(
                "When wanting to play the next track playing a track got {playbin_error_message}"
            )
            .as_str(),
        );
        status_of_rradio.running_status = RunningStatus::LongMessageOnAll4Lines;
    } else {
        let line2 = generate_line2(status_of_rradio);
        status_of_rradio
            .line_2_data
            .update_if_changed(line2.as_str());
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
    assert!(
        (direction == 1) || (direction == -1) || (direction == 0),
        "direction must be plus or minus 1 to change the volume, or zero to merely output the current volume"
    );
    status_of_rradio.current_volume =
        (status_of_rradio.current_volume + config.volume_offset * direction).clamp(
            gstreamer_interfaces::VOLUME_MIN,
            gstreamer_interfaces::VOLUME_MAX,
        );
    if let Err(error_message) = playbin.set_volume(status_of_rradio.current_volume) {
        eprintln!("When changing the volume got error {}\r", error_message);
    }
}
