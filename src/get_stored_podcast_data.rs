use super::PodcastDataAllStations;

/// Given the name of the file that stores the list of podcasts that are wanted, returns the list of podcasts
pub fn get_stored_podcast_data(toml_data_path: &String) -> Result<PodcastDataAllStations, String> {
    match std::fs::read_to_string(toml_data_path) {
        Ok(podcast_data) => {
            let toml_result: Result<PodcastDataAllStations, toml::de::Error> =
                toml::from_str(podcast_data.trim_ascii_end());
            match toml_result {
                Ok(podcast) => Ok(podcast),
                Err(error) => Err(error.to_string()),
            }
        }

        Err(error) => Err(format!(
            "When looking for stored podcast data got error {:?})",
            error
        )),
    }
}

pub fn write_podcast_data_to_file(
    toml_data_path: &String,
    podcast_data_to_write_to_file: String,
) -> Result<(), String> {
    match std::fs::write(toml_data_path, podcast_data_to_write_to_file) {
        Ok(_) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/*
Example data for the podcastlists.toml file

[[podcast_data_for_all_stations]]
title = "The Archers"
url = "https://podcasts.files.bbci.co.uk/b006qpgr.rss"


[[podcast_data_for_all_stations]]
url = "https://feeds.megaphone.fm/ZOELIMITED9301524082"
title= "Zoe"
*/
