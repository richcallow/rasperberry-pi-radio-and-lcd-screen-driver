use super::gstreamer_interfaces::PlaybinElement;
use super::player_status;
use crate::ChannelFileDataDecoded;
use crate::PODCAST_CHANNEL_NUMBER;
use crate::RealTimeDataOnOneChannel;
use crate::RunningStatus;
use crate::SourceType;
use crate::generate_line2;
use crate::get_ip_address;
use gstreamer::ClockTime;

/// Given a playable URL, it plays it & updates the LCD screen accordingly. If not, puts an error message on the LCD screen  
/// A sample url is http://as-hls-ww-live.akamaized.net/pool_55057080/live/ww/bbc_radio_fourfm/bbc_radio_fourfm.isml/bbc_radio_fourfm-audio%3d128000.norewind.m3u8
pub fn play_url(
    new_podcast_text: String,
    status_of_rradio: &mut player_status::PlayerStatus,
    playbin: &mut PlaybinElement,
    config: &crate::read_config::Config,
    lcd: &mut crate::lcd::Lc,
) {
    status_of_rradio.running_status = RunningStatus::RunningNormally;
    status_of_rradio.position_and_duration[PODCAST_CHANNEL_NUMBER] = RealTimeDataOnOneChannel {
        artist: String::new(),
        address_to_ping: get_ip_address(new_podcast_text.clone()),
        index_to_current_track: 0,
        position: ClockTime::ZERO,
        duration: None,
        channel_data: ChannelFileDataDecoded {
            organisation: String::new(),
            source_type: SourceType::UrlList,
            last_track_is_a_ding: false,
            pause_before_playing_ms: None,
            station_urls: vec![new_podcast_text],
            media_details: None,
        },
    };

    status_of_rradio.channel_number = PODCAST_CHANNEL_NUMBER;
    status_of_rradio.initialise_for_new_station();
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
            .update_if_changed(line2.as_str())
    }
}
