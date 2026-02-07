use super::PodcastDataAllStations;
use crate::player_status::PlayerStatus;

/// Given the name of the file that stores the list of podcasts that are wanted, returns the list of podcasts
pub fn get_stored_podcast_data(
    toml_data_path: &String,
) -> Result<PodcastDataAllStations, Result<(), String>> {
    match std::fs::read_to_string(toml_data_path) {
        Ok(podcast_data) => {
            let toml_result: Result<PodcastDataAllStations, toml::de::Error> =
                toml::from_str(podcast_data.trim_ascii_end());
            match toml_result {
                Ok(podcast) => Ok(podcast),
                Err(error) => Err(Err(error.to_string())),
            }
        }

        Err(error) => {
            const OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY: i32 = 2;
            if let Some(OS_ERROR_NO_SUCH_FILE_OR_DIRECTORY) = error.raw_os_error() {
                return Err(Ok(())); // file not found is a valid state; all it means is that the user has not yet stored any podcasts
            }

            Err(Err(format!(
                "When looking for stored podcast data got error {:?})",
                error
            )))
        }
    }
}

/// stores the list of podcasts in the file with the supplied name
/// if it fails, it puts the error message in status_of_rradio.toml_error
pub fn write_podcast_data_to_file(
    podcastlists_filename: &String,
    status_of_rradio: &mut PlayerStatus,
) -> Result<(), ()> {
    if let Ok(podcast_data_to_write_to_file) =
        toml::to_string(&status_of_rradio.podcast_data_from_toml)
    {
        match std::fs::write(podcastlists_filename, podcast_data_to_write_to_file) {
            Ok(_) => Ok(()),
            Err(error) => {
                status_of_rradio.toml_error = Some(error.to_string());
                Err(())
            }
        }
    } else {
        status_of_rradio.toml_error = Some("failed to convert TOML data to String".to_string());
        Err(())
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
