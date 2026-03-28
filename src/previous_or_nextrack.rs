// plays next or previous track when selelcted by the user
// or when needed due to getting to end of track

use super::PlaybinElement;
use super::PlayerStatus;
use super::RunningStatus;
use super::get_channel_details::SourceType;
use super::get_mute_state;
use super::lcd;
use gstreamer::{SeekFlags, prelude::ElementExtManual};

/// Generates the text for line 2 for the normal running case, ie streaming, USB or CD. Adds the throttled state if the Pi is throttled
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
        SourceType::UnknownSource => match status_of_rradio.running_status {
            RunningStatus::NoChannel => {
                format!("Channel {} does not exist", status_of_rradio.channel_number)
            }
            RunningStatus::NoChannelRepeated => format!(
                "Channel {} really does not exist.",
                status_of_rradio.channel_number
            ),
            RunningStatus::LongMessageOnAll4Lines => format!(
                " got message {} {} {}",
                status_of_rradio.line_1_data.text,
                status_of_rradio.line_2_data.text,
                status_of_rradio.line_34_data.text
            ),
            RunningStatus::ShuttingDown => "shutting down".to_string(),
            _ => "Unknown source type".to_string(),
        },
    };
    let throttled_status = lcd::get_throttled::is_throttled();
    if throttled_status.pi_is_throttled {
        line2 = format!("{line2} {}", throttled_status.result)
    };

    format!("{} {}", line2, get_mute_state())
}

/// Plays the next track by modulo incrementing status_of_rradio.index_to_current_track
pub fn next_track(
    status_of_rradio: &mut PlayerStatus,
    playbin: &PlaybinElement,
    config: &crate::read_config::Config,
    lcd: &mut crate::lcd::Lc,
) {
    status_of_rradio.running_status = RunningStatus::RunningNormally; // at least hope that this is true
    status_of_rradio.ping_data.number_of_pings_to_this_channel = 0;
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

pub fn previous_track(
    status_of_rradio: &mut PlayerStatus,
    playbin: &PlaybinElement,
    config: &crate::read_config::Config,
    lcd: &mut crate::lcd::Lc,
) {
    status_of_rradio.initialise_for_new_station();
    if status_of_rradio.position_and_duration[status_of_rradio.channel_number].position
        > config.goto_previous_track_time_delta
    {
        // We have been playing for some time, so seek the start of the track
        let _ = playbin.playbin_element.seek_simple(
            SeekFlags::FLUSH | SeekFlags::KEY_UNIT | SeekFlags::SNAP_NEAREST,
            gstreamer::ClockTime::ZERO,
        );
    } else {
        // we have only just started, so user wants the previous track
        status_of_rradio.position_and_duration[status_of_rradio.channel_number]
            .index_to_current_track = (status_of_rradio.position_and_duration
            [status_of_rradio.channel_number]
            .index_to_current_track
            + status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .station_urls
                .len()
            - 1)
            % status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                .channel_data
                .station_urls
                .len(); // % is a remainder operator not modulo
        if let Err(playbin_error_message) = playbin.play_track(status_of_rradio, config, lcd, false)
        {
            status_of_rradio.all_4lines.update_if_changed(
                format!("When wanting to play the previous track got {playbin_error_message}")
                    .as_str(),
            );
            status_of_rradio.running_status = RunningStatus::LongMessageOnAll4Lines;
        } else {
            status_of_rradio.line_2_data.update_if_changed(
                status_of_rradio.position_and_duration[status_of_rradio.channel_number]
                    .channel_data
                    .organisation
                    .as_str(),
            );
        }
    }
    //qq
}
