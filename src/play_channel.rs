// plays the specified channel, typically in the range 00 to 99
use super::PlayerStatus;
use super::SeekTimes;
use super::gstreamer_interfaces::PlaybinElement;
use super::previous_or_nextrack::generate_line2;
use super::web;
use crate::ChannelErrorEvents;
use crate::DataChanged;
use crate::RunningStatus;
use crate::get_channel_details::SourceType;
use crate::html_helpers::{write_message_to_web_page, write_status_to_web_page};
use crate::my_dbg;
use crate::read_config;
use crate::store_channel_details_and_implement_them;
use gstreamer::ClockTime;
use string_replace_all::StringReplaceAll;

/// plays the specified channel typically 00 to 99
pub fn play_channel(
    channel_number: usize,
    status_of_rradio: &mut PlayerStatus,
    config: &read_config::Config,
    playbin: &mut PlaybinElement,
    lcd: &mut crate::lcd::Lc,
    web_data_changed_tx: &tokio::sync::broadcast::Sender<DataChanged>,
) {
    status_of_rradio.initialise_for_new_station();
    if channel_number == status_of_rradio.channel_number
        && status_of_rradio.running_status == RunningStatus::NoChannel
    {
        status_of_rradio.running_status = RunningStatus::NoChannelRepeated;
    } else {
        status_of_rradio.running_status = RunningStatus::RunningNormally;
        status_of_rradio.position_and_duration[status_of_rradio.channel_number].position =
            ClockTime::ZERO;
        status_of_rradio.line_2_data.update_if_changed("");
        status_of_rradio.line_34_data.update_if_changed("");
        status_of_rradio.all_4lines.update_if_changed("");
        write_status_to_web_page(status_of_rradio, web_data_changed_tx);
        let previous_channel_number = status_of_rradio.channel_number;
        status_of_rradio.channel_number = channel_number;

        match status_of_rradio.position_and_duration[status_of_rradio.channel_number]
            .channel_data
            .source_type
        {
            SourceType::Usb | SourceType::Cd => {
                let _ =
                    web_data_changed_tx.send(web::DataChanged::CanSeekBackwards(Some(SeekTimes {
                        short_seek_time: -config.short_advance_time,
                        long_seek_time: -config.long_advance_time,
                    })));

                let _ =
                    web_data_changed_tx.send(web::DataChanged::CanSeekForwards(Some(SeekTimes {
                        short_seek_time: config.short_advance_time,
                        long_seek_time: config.long_advance_time,
                    })));
            }
            SourceType::UrlList | SourceType::UnknownSource => {
                let _ = web_data_changed_tx.send(web::DataChanged::CanSeekBackwards(None));

                let _ = web_data_changed_tx.send(web::DataChanged::CanSeekForwards(None));
            }
        }

        if let Err(the_channel_error_events) = store_channel_details_and_implement_them(
            config,
            status_of_rradio,
            playbin,
            previous_channel_number,
            lcd,
        ) {
            my_dbg!(format!("{:?}", the_channel_error_events));
            write_message_to_web_page(
                format!("{:?}", the_channel_error_events),
                String::new(),
                web_data_changed_tx,
            );
            match the_channel_error_events {
                ChannelErrorEvents::CouldNotFindChannelFile => {
                    status_of_rradio.toml_error = None; // clear the TOML error out, the user must have seen it by now
                    status_of_rradio.running_status = if previous_channel_number == channel_number {
                        RunningStatus::NoChannelRepeated
                    } else {
                        RunningStatus::NoChannel
                    };
                    if let Some(ding_filename) = &config.aural_notifications.filename_error {
                        // play a ding if one has been specified
                        status_of_rradio.position_and_duration
                            [crate::player_status::START_UP_DING_CHANNEL_NUMBER]
                            .channel_data
                            .station_urls = vec![format!("file://{ding_filename}")];
                        let _ignore_error_if_beep_fails =
                            playbin.play_track(status_of_rradio, config, lcd, false);
                        status_of_rradio.position_and_duration
                            [crate::player_status::START_UP_DING_CHANNEL_NUMBER]
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
                    status_of_rradio
                        .all_4lines
                        .update_if_changed(the_channel_error_events.to_lcd_screen().as_str());
                    status_of_rradio.running_status = RunningStatus::LongMessageOnAll4Lines;
                }
            }
        }
    }
    if let Err(playbin_error_message) = playbin.play_track(status_of_rradio, config, lcd, true) {
        status_of_rradio.all_4lines.update_if_changed(
            format!(
                "When playing a track on channel {} got {playbin_error_message}",
                status_of_rradio.channel_number
            )
            .as_str(),
        );
        status_of_rradio.running_status = RunningStatus::LongMessageOnAll4Lines;
    } else {
        // play worked
        let line2 = generate_line2(status_of_rradio);
        status_of_rradio
            .line_2_data
            .update_if_changed(line2.as_str());
        write_message_to_web_page(
            line2,
            status_of_rradio.position_and_duration[channel_number]
                .artist
                .clone(),
            web_data_changed_tx,
        );
    }
}
