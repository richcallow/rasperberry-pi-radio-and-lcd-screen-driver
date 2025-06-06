use crate::player_status;
use glib::object::{Cast, ObjectExt};

use gstreamer::{
    glib,
    prelude::{ElementExt},
};
use gstreamer_audio::prelude::StreamVolumeExt;

/// The normal maximum for gstreamer that will not overload
pub const VOLUME_ZERO_DB: i32 = 100;
/// The lowest possible gstreamer volume
pub const VOLUME_MIN: i32 = 0;
/// The maximum possible gstreamer volume
pub const VOLUME_MAX: i32 = 120;

#[derive(Debug)] // we must not enable clone, as, if we do, the previous version is closed and stops playing
pub struct PlaybinElement {
    pub playbin_element: gstreamer::Element,
}

impl std::ops::Drop for PlaybinElement {
    /// When the playbin element is dropped for any reason, (which included panics) drop is called. Mouse over it for details
    /// if we do not set it to Null, a panic will occur when the playbin element is dropped
    fn drop(&mut self) {
        if self
            .playbin_element
            .set_state(gstreamer::State::Null)
            .is_err()
        {
            eprintln!("Failed to stop stream on shutdown\r");
        } else {
            println!("Shutdown success\r")
        }
    }
}

impl PlaybinElement {
    /// Sets the volume; returns an error message if it fails
    pub fn set_volume(&mut self, volume_wanted: i32) -> Result<(), String> {
        self.playbin_element
            .dynamic_cast_ref::<gstreamer_audio::StreamVolume>()
            .ok_or("Could not get the stream volume")? // return the string. no panick
            .set_volume(
                gstreamer_audio::StreamVolumeFormat::Db,
                f64::from(volume_wanted - VOLUME_ZERO_DB),
            );
        Ok(())
    }

    /// Sets up the playbin.
    /// Returns either a gstreamer "Element" & sets the inital volume & buffer size or it returns an error string
    /// Exceptionally if gstreamer cannot be initialised it panicks.
    pub fn setup(
        config: &crate::read_config::Config,
    ) -> Result<(PlaybinElement, gstreamer::bus::BusStream), String> {
        gstreamer::init() // returns a Result which is either OK with no data, or an error of type glib::error
        .map_err(|error_message| format!("When trying to initialize gstreamer got error {error_message:?}"))    // in this case map_err returns OK or maps it a different type of error, in the case a string
        ?; // returns early with the error as a string

        let playbin_element = gstreamer::ElementFactory::make("playbin") // ::make will panic if we have not yet called gstreamer::init
            .build()
            .map_err(|ye_error2| {
                format!("When trying to get a gstreamer playbin got error {ye_error2:?}")
            })?;

        if let Some(stream_volume) =
            playbin_element.dynamic_cast_ref::<gstreamer_audio::StreamVolume>()
        {
            let volume = config.initial_volume.clamp(VOLUME_MIN, VOLUME_MAX);

            stream_volume.set_volume(
                gstreamer_audio::StreamVolumeFormat::Db,
                f64::from(volume - VOLUME_ZERO_DB),
            );
        } else {
            return Err("Could not get the stream volume".to_string());
        }

        let current_flags: glib::Value = playbin_element.property("flags"); //this can panic as we have not checked that "flags" actually exists & has the expected type glib::Value

        let flags = glib::FlagsClass::with_type(current_flags.type_())
            .ok_or("failed to get the gstreamer flags class")? // Remember the question mark means return early with the errror message just to the left of it. IE we do not execute the next line if there is an error
            .builder_with_value(current_flags)
            .ok_or("failed to get the flags class builder")? // as above, do not execute the rest if there is an error
            .unset_by_nick("video") // remove the video flag, which means we cannot process video & do not was time trying to do so.
            .unset_by_nick("text") // remove the text flag, which means we cannot process subtitles & do not was time trying to do so.
            .build()
            .ok_or("Failed to unset the unwanted gstreamer flags")?; // question mark says "if there is an error,  return from here with the string specified, otherwise continue"

        playbin_element.set_property_from_value("flags", &flags);
        // at this point we have a playbin element with the wanted flags , ie the default with "text" & "video" removed (actually "Deinterlace video if necessary" & "Use software color balance" remain)

        if let Some(buffering_duration) = config.buffering_duration {
            // the duration is specified in the config file

            if let Ok(duration_as_nanos) = i64::try_from(buffering_duration.as_nanos()) {
                playbin_element.set_property("buffer-duration", duration_as_nanos);
                println!("Set the buffer duration to {duration_as_nanos} ns\r")
            } else {
                eprintln!("Failed to set the buffer duration")
            }
        }

        let bus = playbin_element
            .bus()
            .ok_or("The gstreamer playbin's message bus is missing")?
            .stream();

        Ok((PlaybinElement { playbin_element }, bus)) // this is the return value at the end the function, or put differently, it is rust shorthand for  "return Ok(playbin_element);" as it is after the last statement.
    }

    pub fn set_state(
        &self,
        new_state: gstreamer::State,
    ) -> Result<gstreamer::StateChangeSuccess, gstreamer::StateChangeError> {
        self.playbin_element.set_state(new_state)
    }

    /// Plays the first track aka station specified by player_status
    /// If it fails the error message is returned as Some ()
    pub fn play_track(&self, status_of_rradio: &player_status::PlayerStatus) -> Result<(), String> {
        println!(
            "playlist {:?}\r",
            status_of_rradio.channel_file_data.station_url
        );

        match self.playbin_element
               .set_state(gstreamer::State::Null)      // we need to set it to null before we can change the station 
        {
        Ok(_state_change_success) => {
                self.playbin_element.set_property(
                    "uri",              // if "uri" does not exist, it panics, but that does not seem to be anything that can be done about it.
                    &status_of_rradio.channel_file_data.station_url[status_of_rradio.index_to_current_track],
                );
                match self.playbin_element //clone here makes it stop working
                    .set_state(gstreamer::State::Playing)
                {
                    Ok(_ok_message) => Ok(()),
                    Err(error) => Err(format!(
                        "Error message setting gstreamer to play{:?}",
                        error
                    )),

                }
            },
            Err(error_message)=> {Err(format!("Failed to set the URL. Got error {:?}",error_message ).to_string())}
        }
    }
}
