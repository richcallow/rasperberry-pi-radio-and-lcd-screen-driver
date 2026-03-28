use super::{DataChanged, EpisodeDataForOnePodcastDownloaded, web};
use crate::{
    extract_html::extract,
    my_dbg,
    player_status::{self, PlayerStatus},
};

/// Work out if a podcast string is an RSS feed, & if it is, return the name of the podcast
/// after de-escaping it
pub fn is_rss(podcast_string: &str) -> Option<String> {
    if podcast_string.contains("<rss version") | podcast_string.contains("xmlns:atom") {
        Some(decode_html(extract(podcast_string, "<title>", "</title>")))
    } else {
        None
    }
}

/// de-escapes an HTML sequence if the input is valid HTML. If it is invalid, it returns the input string unchanged
pub fn decode_html(html_string: &str) -> String {
    extern crate htmlescape;
    if let Ok(new_value) = htmlescape::decode_html(html_string) {
        new_value
    } else {
        html_string.to_string()
    }
}
/// write a message to the web page after webpage use
pub fn write_message_to_web_page(
    main_message: String,
    secondary_message: String,
    web_data_changed_tx: &tokio::sync::broadcast::Sender<DataChanged>,
) {
    let _ = web_data_changed_tx.send(web::DataChanged::EpisodeDataForOnePodcast {
        episode_data_for_one_podcast: EpisodeDataForOnePodcastDownloaded {
            channel_title: main_message,
            description: secondary_message,
            data_for_multiple_episodes: vec![],
        },
    });
}

/// write the line_2_data output and line_34_data & TOML error messages if any to the web page
pub fn write_status_to_web_page(
    status_of_rradio: &PlayerStatus,
    web_data_changed_tx: &tokio::sync::broadcast::Sender<DataChanged>,
) {
    let mut secondary = if let Some(toml_message) = status_of_rradio.toml_error.clone() {
        format!("{} {}", toml_message, status_of_rradio.line_34_data.text)
    } else {
        status_of_rradio.line_34_data.text.clone()
    };

    if status_of_rradio.line_1_data.text.contains("Error")
        || status_of_rradio.line_1_data.text.contains("error")
    {
        my_dbg!(status_of_rradio.line_1_data.text.clone());
        secondary = format!("{} {}", status_of_rradio.line_1_data.text, secondary);
    }

    if !status_of_rradio.all_4lines.text.is_empty() {
        my_dbg!(status_of_rradio.line_1_data.text.clone());
        secondary = format!("{} {}", status_of_rradio.all_4lines.text, secondary);
    }

    if status_of_rradio.channel_number < player_status::NUMBER_OF_POSSIBLE_CHANNELS {
        write_message_to_web_page(
            format!(
                "channel {} {}",
                status_of_rradio.channel_number, status_of_rradio.line_2_data.text
            ),
            secondary,
            web_data_changed_tx,
        )
    } else {// do not output the channel number as it is not a real one.
        write_message_to_web_page(
            status_of_rradio.line_2_data.text.clone(),
            secondary,
            web_data_changed_tx,
        )
    }
}
