
use crate::extract_html::extract;
use super::{EpisodeDataForOnePodcastDownloaded, DataChanged, web};

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
/// write a status message to the web page after webpage use
pub fn write_status_to_web_page(
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