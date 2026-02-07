//#![allow(unused_imports)] // TODO - Remove after refactor
//#![allow(unused_variables)] // TODO - Remove after refactor

#[cfg(not(unix))]
compile_error!("You must compile this on linux");

use std::task::Poll;

use futures_util::StreamExt;
use get_channel_details::{ChannelErrorEvents, SourceType};
use gstreamer::ClockTime;
use gstreamer::{SeekFlags, prelude::ElementExtManual};
use gstreamer_interfaces::PlaybinElement;

mod cd_functions;
mod extract_html;
mod get_channel_details;
mod get_config_file_path;
mod get_stored_podcast_data;
mod gstreamer_interfaces;
mod keyboard;
mod lcd;
mod mount_media;
mod ping;
mod play_urls;
mod player_status;
mod read_config;
mod unmount;
mod web;

use crate::extract_html::extract;
use crate::get_channel_details::{ChannelFileDataDecoded, get_ip_address};
use crate::player_status::{PODCAST_CHANNEL_NUMBER, RealTimeDataOnOneChannel};
use get_channel_details::store_channel_details_and_implement_them;
use lcd::get_local_ip_address;
use lcd::{RunningStatus, ScrollData};
use ping::{get_ping_time, see_if_there_is_a_ping_response};
use player_status::NUMBER_OF_POSSIBLE_CHANNELS;
use player_status::PlayerStatus;
use serde::{Deserialize, Serialize};
use string_replace_all::StringReplaceAll;
use unmount::unmount_if_needed;

#[macro_export]
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
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::my_dbg!($val)),+,)
    };
}

/// An enum of all the types of event, each with their own event sub-type
#[derive(Debug)]
enum Event {
    Keyboard(keyboard::Event),
    GStreamer(gstreamer::Message),
    Web(web::Event),
    Ticker(tokio::time::Instant),
}

/// URL of RSS & title as stored in playlists.toml file entry
/// one per station.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PodcastDataFromToml {
    pub title: String,
    pub url: String,
}

/// data downloaded from internet for a single podcast in the downloaded series of podcasts
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataForOnePodcast {
    pub date: String,
    pub subtitle: String,
    pub summary: String,
    pub url: String,
}

/// data downloaded from internet for the downloaded series of podcasts
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EpisodeDataForOnePodcastDownloaded {
    pub channel_title: String,
    pub description: String,
    pub data_for_multiple_episodes: Vec<DataForOnePodcast>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PodcastDataAllStations {
    pub podcast_data_for_all_stations: Vec<PodcastDataFromToml>,
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
    let podcastlists_filename: String = "podcastlists.toml".to_string();

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
    match get_stored_podcast_data::get_stored_podcast_data(&podcastlists_filename) {
        Ok(podcast_data) => {
            status_of_rradio.podcast_data_from_toml = podcast_data;
        }
        Err(error) => {
            if let Err(toml_error) = error {
                status_of_rradio.toml_error = Some(toml_error)
            }
        }
    }

    status_of_rradio.startup_folder = root_folder;
    if let Some(toml_error_message) = toml_error {
        // if we got an error we should display it; hopefully, toml_error == none
        status_of_rradio.toml_error = Some(toml_error_message);
    }

    read_config::insert_samba(&config, &mut status_of_rradio);
    read_config::insert_usb(&config, &mut status_of_rradio);

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

            let (web_data_changed_tx, web_events) = web::start_server();

            let mut mapped_web_events =
                tokio_stream::wrappers::UnboundedReceiverStream::new(web_events).map(Event::Web);

            let mut some_timer = tokio_stream::wrappers::IntervalStream::new(
                tokio::time::interval(std::time::Duration::from_millis(300)),
            )
            .map(Event::Ticker);

            change_volume(
                0, // if direction == 0 it gets the volume, but does not change it
                &config,
                &mut status_of_rradio,
                &mut playbin,
                &web_data_changed_tx,
            );

            let mut child_ping = ping::send_ping(&mut status_of_rradio, &config);

            if let Some(toml_error) = status_of_rradio.toml_error {
                status_of_rradio.line_1_data.update_if_changed(&toml_error); // convert to be a scrollable message
                status_of_rradio.toml_error = None;
            }
            let mut episode_data_for_one_podcast_downloaded = EpisodeDataForOnePodcastDownloaded {
                channel_title: String::new(),
                description: String::new(),
                data_for_multiple_episodes: Vec::new(),
            };
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
                    // First poll the keyboard events source for keyboard events
                    match mapped_keyboard_events.poll_next_unpin(cx) {
                        // std::future::poll_fn strips off "Poll::Ready", so if there is a keyboard event, the next line becomes "event = keyboard_event"
                        Poll::Ready(keyboard_event) => return Poll::Ready(keyboard_event),
                        Poll::Pending => (), //if the match gives Pending, which means that so far event has not been made equal to anything.
                    };

                    // Then poll the GStreamer playbin events source for gstreamer events
                    match mapped_playbin_message_bus.poll_next_unpin(cx) {
                        // std::future::poll_fn strips off "Poll::Ready", so if there is a playbin event, the next line becomes "event = playbin_event"
                        Poll::Ready(playbin_event) => return Poll::Ready(playbin_event),
                        Poll::Pending => (), //if the match gives Pending, which means that so far event has not been made equal to anything.
                    };

                    // Then poll the web events source for web events
                    match mapped_web_events.poll_next_unpin(cx) {
                        // std::future::poll_fn strips off "Poll::Ready", so if there is a web event, the next line becomes "event = web_event"
                        Poll::Ready(web_event) => return Poll::Ready(web_event),
                        Poll::Pending => (), //if the match gives Pending, which means that so far event has not been made equal to anything.
                    }

                    match some_timer.poll_next_unpin(cx) {
                        Poll::Ready(playbin_event) => return Poll::Ready(playbin_event),
                        Poll::Pending => (),
                    }

                    // No event sources are ready, notify we're awaiting, i.e. pending, incoming events.
                    // poll_fn calls this code block the next time it's awoken, not in a CPU intensive tight loop.
                    Poll::Pending //this is the return value event is made equal to Poll::Pending.
                })
                .await;

                //Now that we have an event, work out what to do with it
                match event {
                    None => {
                        // we are ending the program if we get to here
                        if let Some(usb_config) = &config.usb
                            && let Err(error) = unmount_if_needed(
                                &mut status_of_rradio.position_and_duration
                                    [usb_config.channel_number],
                            )
                        {
                            eprintln!(
                                "Failed to unmount local USB stick when ending program. got {}",
                                error
                            )
                        }

                        if let Some(samba_config) = &config.samba
                            && let Err(error) = unmount_if_needed(
                                &mut status_of_rradio.position_and_duration
                                    [samba_config.channel_number],
                            )
                        {
                            eprintln!(
                                "Failed to unmount Samba drive when ending program. got {}",
                                error
                            )
                        }

                        status_of_rradio.running_status = lcd::RunningStatus::ShuttingDown;
                        lcd.clear();
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
                            change_volume(
                                1,
                                &config,
                                &mut status_of_rradio,
                                &mut playbin,
                                &web_data_changed_tx,
                            );
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
                            change_volume(
                                -1,
                                &config,
                                &mut status_of_rradio,
                                &mut playbin,
                                &web_data_changed_tx,
                            );
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
                            status_of_rradio.initialise_for_new_station();
                            if status_of_rradio.position_and_duration
                                [status_of_rradio.channel_number]
                                .position
                                > config.goto_previous_track_time_delta
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
                                status_of_rradio.position_and_duration
                                    [status_of_rradio.channel_number]
                                    .position = ClockTime::ZERO;
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
                                                error_message
                                                    .replace("\n", " ") // cannot handle new lines, so turn into spaces
                                                    .replace("|", " ") // not very meaningful, so turn into spaces
                                                    .replace("^", " ") // not very meaningful, so turn into spaces
                                                    .replace_all("  ", " ")
                                                    .replace_all("  ", " ")
                                                    .replace_all("  ", " ")
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
                                    .update_if_changed(line2.as_str());
                                let _ = web_data_changed_tx.send(
                                    web::DataChanged::EpisodeDataForOnePodcast {
                                        episode_data_for_one_podcast:
                                            EpisodeDataForOnePodcastDownloaded {
                                                channel_title: line2,
                                                description: status_of_rradio.position_and_duration
                                                    [channel_number]
                                                    .artist
                                                    .clone(),
                                                data_for_multiple_episodes: vec![],
                                            },
                                    },
                                );
                            }
                        }
                        keyboard::Event::OutputStatusDebug => {
                            println!("\r");

                            for line in status_of_rradio
                                .generate_rradio_report()
                                .expect("Formatting error while gererating report")
                                .lines()
                            {
                                println!("{line}\r");
                            }
                        }
                        keyboard::Event::OutputConfigDebug => {
                            status_of_rradio.output_config_information(&config);
                        }
                        keyboard::Event::OutputRssData => println!("\r\nRSS data\r\n{:?}", 9999),
                        keyboard::Event::OutputMountFolderContents => {
                            status_of_rradio.output_mount_folder_contents(&config)
                        }
                        keyboard::Event::NewLineOnScreen => println!("\r"), // output a blank line on the screen to aid debugging clarity
                    },

                    Some(Event::GStreamer(gstreamer_message)) => {
                        use gstreamer::MessageView;
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
                                                println!("got new artist!!! {artist:?}\r");
                                                if status_of_rradio.channel_number
                                                    == PODCAST_CHANNEL_NUMBER
                                                {
                                                    let line2 = generate_line2(&status_of_rradio);
                                                    status_of_rradio
                                                        .line_2_data
                                                        .update_if_changed(line2.as_str())
                                                }
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
                                    change_volume(
                                        0,
                                        &config,
                                        &mut status_of_rradio,
                                        &mut playbin,
                                        &web_data_changed_tx,
                                    );
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
                    Some(Event::Web(web_event)) => match web_event {
                        web::Event::EpisodeSelected { episode_index } => {
                            let url = episode_data_for_one_podcast_downloaded
                                .data_for_multiple_episodes[episode_index]
                                .url
                                .clone();
                            status_of_rradio.running_status = RunningStatus::RunningNormally;
                            status_of_rradio.position_and_duration[PODCAST_CHANNEL_NUMBER] =
                                RealTimeDataOnOneChannel {
                                    artist: String::new(),
                                    address_to_ping: get_ip_address(url.clone()),
                                    index_to_current_track: 0,
                                    position: ClockTime::ZERO,
                                    duration: None,
                                    channel_data: ChannelFileDataDecoded {
                                        organisation: format!(
                                            "{} {}",
                                            episode_data_for_one_podcast_downloaded.channel_title, // eg "the Archers"
                                            episode_data_for_one_podcast_downloaded
                                                .data_for_multiple_episodes[episode_index]
                                                .subtitle
                                                .clone()
                                        ),
                                        source_type: SourceType::UrlList,
                                        last_track_is_a_ding: false,
                                        pause_before_playing_ms: None,
                                        station_urls: vec![url],
                                        media_details: None,
                                    },
                                };
                            status_of_rradio.channel_number = PODCAST_CHANNEL_NUMBER;
                            status_of_rradio.initialise_for_new_station();
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
                        web::Event::RequestWebPageStartupData {
                            // we have received a request for the startup data, so send it to the web server
                            web_page_startup_data_tx,
                        } => {
                            let _ = web_page_startup_data_tx.send(web::WebPageStartupData {
                                // initialise the volume on the web page
                                volume: status_of_rradio.current_volume,
                                // set up the dropdown box that allows the user to choose the podcast station
                                podcast_data_for_all_stations: status_of_rradio
                                    .podcast_data_from_toml
                                    .podcast_data_for_all_stations
                                    .clone(),
                            });
                        }
                        web::Event::PlayPause => {
                            // user on a web client has hit the play/pause button
                            let new_state =
                                if status_of_rradio.gstreamer_state == gstreamer::State::Playing {
                                    gstreamer::State::Paused
                                } else {
                                    gstreamer::State::Playing
                                };
                            if let Err(_error_message) = playbin.set_state(new_state) {
                                eprintln!(
                                    "Could not set the gstreamer state when user on web client hit play//pause\r"
                                )
                            }
                        }

                        web::Event::PodcastIndexChanged { podcast_index } => {
                            // the user has changed the podcast they want ie they want "the Archers"
                            if podcast_index >= 0 {
                                let _ = web_data_changed_tx.send(
                                    web::DataChanged::EpisodeDataForOnePodcast {
                                        episode_data_for_one_podcast:
                                            EpisodeDataForOnePodcastDownloaded {
                                                channel_title: "Waiting for the data".to_string(),
                                                description: String::new(),
                                                data_for_multiple_episodes: vec![],
                                            },
                                    },
                                );

                                // check that the value chosen is valid
                                let wanted_podcast = &status_of_rradio
                                    .podcast_data_from_toml
                                    .podcast_data_for_all_stations
                                    [podcast_index as usize]; // podcast = eg The Archers

                                // next send the URL taken from the podcasts.toml file,
                                // which should be a list of playable URLs with associated descriptions
                                match reqwest::get(&wanted_podcast.url).await {
                                    Ok(podcast_response) => match podcast_response.text().await {
                                        Ok(podcast_string) => {
                                            // if we got here, the URL in the TOML file was valid
                                            status_of_rradio.podcast_index = podcast_index;
                                            // so, now we have extract the data from it

                                            let channel_title_temp = extract(
                                                podcast_string.as_str(),
                                                "<title>",
                                                "</title>",
                                            );
                                            let channel_title = decode_html(channel_title_temp);

                                            let description = extract(
                                                podcast_string.as_str(),
                                                "<description><![CDATA[",
                                                "]]></description>",
                                            );

                                            let episodes: Vec<&str> =
                                                podcast_string.split("<item>").collect();

                                            let mut data_for_multiple_podcasts: Vec<
                                                DataForOnePodcast,
                                            > = Vec::new();

                                            for episode in &episodes {
                                                if episode.contains("<enclosure url=") {
                                                    let date =
                                                        extract(episode, "<title>", "</title>");
                                                    let url = extract(
                                                        episode,
                                                        "<enclosure url=\"",
                                                        "\" length",
                                                    );
                                                    let subtitle = extract(
                                                        episode,
                                                        "<itunes:subtitle>",
                                                        "</itunes:subtitle>",
                                                    );
                                                    let summary = extract(
                                                        episode,
                                                        "<itunes:summary><![CDATA[",
                                                        "]]>",
                                                    );

                                                    let data_for_one_podcast: DataForOnePodcast =
                                                        DataForOnePodcast {
                                                            date: date.to_string(),
                                                            subtitle: subtitle.to_string(),
                                                            summary: summary.to_string(),
                                                            url: url.to_string(),
                                                        };
                                                    data_for_multiple_podcasts
                                                        .push(data_for_one_podcast);
                                                }
                                            }
                                            episode_data_for_one_podcast_downloaded =
                                                EpisodeDataForOnePodcastDownloaded {
                                                    channel_title: channel_title.to_string(),
                                                    description: description.to_string(),
                                                    data_for_multiple_episodes:
                                                        data_for_multiple_podcasts,
                                                };

                                            let _ = web_data_changed_tx.send(
                                                web::DataChanged::EpisodeDataForOnePodcast {
                                                    episode_data_for_one_podcast:
                                                        episode_data_for_one_podcast_downloaded
                                                            .clone(),
                                                },
                                            );
                                        }
                                        Err(wait_error) => {
                                            status_of_rradio.latest_podcast_string = None;
                                            eprintln!(
                                                "When waiting for RSS got error {:?}\r",
                                                wait_error.to_string()
                                            )
                                        }
                                    },
                                    Err(wait_error2) => {
                                        status_of_rradio.latest_podcast_string = None;
                                        eprintln!(
                                            "When waiting2 for RSS got error {:?}\r",
                                            wait_error2.to_string()
                                        )
                                    }
                                }
                            } else {
                                eprintln!("Empty station selected by user.\r");
                                let _ = web_data_changed_tx.send(
                                    web::DataChanged::EpisodeDataForOnePodcast {
                                        episode_data_for_one_podcast:
                                            EpisodeDataForOnePodcastDownloaded {
                                                channel_title: "No podcast selected".to_string(),
                                                description: String::new(),
                                                data_for_multiple_episodes: vec![],
                                            },
                                    },
                                );
                            }
                        }
                        web::Event::RequestRRadioStatusReport { report_tx } => {
                            if report_tx
                                .send(status_of_rradio.generate_rradio_report())
                                .is_err()
                            {
                                eprintln!("Failed to send RRadio Status Report to web worker\r");
                            }
                        }
                        web::Event::VolumeDownPressed => change_volume(
                            -1,
                            &config,
                            &mut status_of_rradio,
                            &mut playbin,
                            &web_data_changed_tx,
                        ),
                        web::Event::DeletePodcast => {
                            if status_of_rradio.podcast_index > 0 {
                                if status_of_rradio.podcast_index
                                    < status_of_rradio
                                        .podcast_data_from_toml
                                        .podcast_data_for_all_stations
                                        .len() as i32
                                {
                                    status_of_rradio
                                        .podcast_data_from_toml
                                        .podcast_data_for_all_stations
                                        .remove(status_of_rradio.podcast_index as usize);
                                    if let Ok(()) =
                                        get_stored_podcast_data::write_podcast_data_to_file(
                                            &podcastlists_filename,
                                            &mut status_of_rradio,
                                        )
                                    {
                                        // update the web page with the new list of podcasts
                                        let _ =
                                            web_data_changed_tx.send(web::DataChanged::Podcast {
                                                podcast_data_from_toml: status_of_rradio
                                                    .podcast_data_from_toml
                                                    .podcast_data_for_all_stations
                                                    .clone(),
                                            });
                                        // note
                                        let _ = web_data_changed_tx.send(
                                            web::DataChanged::EpisodeDataForOnePodcast {
                                                episode_data_for_one_podcast:
                                                    EpisodeDataForOnePodcastDownloaded {
                                                        channel_title: "No Podcast selected"
                                                            .to_string(),
                                                        description: String::new(),
                                                        data_for_multiple_episodes: vec![],
                                                    },
                                            },
                                        );
                                    }
                                }
                            } else {
                                eprintln!(
                                    "Error cannot remove podcast from list as out of bounds\r"
                                )
                            }
                        }
                        web::Event::VolumeUpPressed => change_volume(
                            1,
                            &config,
                            &mut status_of_rradio,
                            &mut playbin,
                            &web_data_changed_tx,
                        ),

                        web::Event::UpdatePosition { position_ms } => {
                            let _ = playbin.playbin_element.seek_simple(
                                SeekFlags::FLUSH | SeekFlags::KEY_UNIT | SeekFlags::SNAP_NEAREST,
                                gstreamer::ClockTime::from_mseconds(position_ms),
                            );
                        }
                        web::Event::Getrss => {
                            my_dbg!("getrss button pressed");
                        }

                        web::Event::PodcastText { new_podcast_text } => {
                            //zzzz
                            let _ = web_data_changed_tx.send(
                                web::DataChanged::EpisodeDataForOnePodcast {
                                    episode_data_for_one_podcast:
                                        EpisodeDataForOnePodcastDownloaded {
                                            channel_title: "Looking for RSS data".to_string(),
                                            description: String::new(),
                                            data_for_multiple_episodes: vec![],
                                        },
                                },
                            );

                            if let Ok(podcast_response) =
                                reqwest::get(new_podcast_text.trim()).await
                                && let Ok(podcast_string) = podcast_response.text().await
                                && let Some(channel_title) = is_rss(podcast_string.as_str())
                            {
                                let new_podcast_data = PodcastDataFromToml {
                                    title: channel_title.clone(),
                                    url: new_podcast_text,
                                };
                                status_of_rradio
                                    .podcast_data_from_toml
                                    .podcast_data_for_all_stations
                                    .push(new_podcast_data);

                                if let Ok(()) = get_stored_podcast_data::write_podcast_data_to_file(
                                    &podcastlists_filename,
                                    &mut status_of_rradio,
                                ) {
                                    let _ = web_data_changed_tx.send(web::DataChanged::Podcast {
                                        podcast_data_from_toml: status_of_rradio
                                            .podcast_data_from_toml
                                            .podcast_data_for_all_stations
                                            .clone(),
                                    });
                                    let _ = web_data_changed_tx.send(
                                        web::DataChanged::EpisodeDataForOnePodcast {
                                            episode_data_for_one_podcast:
                                                EpisodeDataForOnePodcastDownloaded {
                                                    channel_title: format!(
                                                        "Stored RSS data for \"{}\"",
                                                        channel_title
                                                    ),
                                                    description: String::new(),
                                                    data_for_multiple_episodes: vec![],
                                                },
                                        },
                                    );
                                } else {
                                    let _ = web_data_changed_tx.send(
                                        web::DataChanged::EpisodeDataForOnePodcast {
                                            episode_data_for_one_podcast:
                                                EpisodeDataForOnePodcastDownloaded {
                                                    channel_title: "Failed to store new RSS data"
                                                        .to_string(),
                                                    description: String::new(),
                                                    data_for_multiple_episodes: vec![],
                                                },
                                        },
                                    );
                                }
                            } else {
                                // we did not get a valid podcast, so hopefully it was a valid URL to play
                                play_urls::play_url(
                                    new_podcast_text.trim().into(),
                                    &mut status_of_rradio,
                                    &mut playbin,
                                    &config,
                                    &mut lcd,
                                );
                            }
                        }

                        web::Event::SliderMoved { value } => {
                            println!("new slider value {}\r", value)
                        }
                    },
                    Some(Event::Ticker(_now)) => {
                        if status_of_rradio.channel_number
                            <= player_status::NUMBER_OF_POSSIBLE_CHANNELS
                            && let Some(position) = playbin
                                .playbin_element
                                .query_position::<gstreamer::ClockTime>()
                        {
                            status_of_rradio.position_and_duration
                                [status_of_rradio.channel_number]
                                .position = position;

                            let duration = playbin.playbin_element.query_duration();

                            status_of_rradio.position_and_duration
                                [status_of_rradio.channel_number]
                                .duration = duration;

                            let _ = web_data_changed_tx
                                .send(web::DataChanged::Position { position, duration });
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

            if let Ok(wait_result) = child_ping.wait()
            // we need to have a wait on the ping in order to keep the compiler happy
                && !wait_result.success()
            {
                eprintln!("Got the errro ping wait status on exit {:?}", wait_result);
            }
        }
        Err(message) => {
            status_of_rradio
                .all_4lines
                .update_if_changed(format!("Failed to get a playbin: {message}").as_str());
            status_of_rradio.running_status = RunningStatus::LongMessageOnAll4Lines;
            lcd.write_rradio_status_to_lcd(&status_of_rradio, &config);
        }
    }

    Ok(()) //as at the start we said it returned either "Ok(())" 
    //or an error, as nothing has failed, we give the "all worked OK termination" value
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
        SourceType::Usb => {
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
            } //  http://open.live.bbc.co.uk/mediaselector/6/redir/version/2.0/mediaset/audio-nondrm-download-rss/proto/http/vpid/p0mksps9.mp3

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
        SourceType::UnknownSource => "Unknown source type".to_string(),
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
    data_changed_tx: &tokio::sync::broadcast::Sender<web::DataChanged>,
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

    let _ = data_changed_tx.send(web::DataChanged::Volume(status_of_rradio.current_volume));
}

//zzzz
/// Work out if a podcast string is an RSS feed, & if it is, return the name of the podcast
/// after de-escaping it
pub fn is_rss(podcast_string: &str) -> Option<String> {
    if podcast_string.contains("<rss version") | podcast_string.contains("xmlns:atom") {
        Some(decode_html(extract(podcast_string, "<title>", "</title>")))
    } else {
        None
    }
}

/// de-escapes an HTML sequence if the input is valid HTML
/// if it is invalid, it returns the input string unchanged
pub fn decode_html(html_string: &str) -> String {
    extern crate htmlescape;
    if let Ok(new_value) = htmlescape::decode_html(html_string) {
        new_value
    } else {
        html_string.to_string()
    }
}
