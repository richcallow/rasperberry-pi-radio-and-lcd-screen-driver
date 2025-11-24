use crate::lcd::{LineNum, TextBuffer};
use crate::unmount::unmount_if_needed;
use crate::{PlayerStatus, mount_samba};
use crate::{
    get_channel_details::SourceType,
    lcd::RunningStatus,
    mount_usb::mount_usb,
    player_status::{self},
};
use glib::object::{Cast, ObjectExt};
use gstreamer::{
    SeekFlags, glib,
    prelude::{ElementExt, ElementExtManual},
};
use gstreamer_audio::prelude::StreamVolumeExt;

/// The normal maximum for gstreamer that will not overload
pub const VOLUME_ZERO_DB: i32 = 100;
/// The lowest possible gstreamer volume
pub const VOLUME_MIN: i32 = 0;
/// The maximum possible gstreamer volume
pub const VOLUME_MAX: i32 = 120;

#[derive(Debug)] // we must not enable clone, as, if we do, the previous version is closed and stops playing
/// The interface used to connect to gstreamer
pub struct PlaybinElement {
    pub playbin_element: gstreamer::Element,
}

fn mount_usb_if_necessary(
    status_of_rradio: &mut PlayerStatus,
    config: &crate::read_config::Config,
) -> Result<(), String> {
    if let Some(usb) = &config.usb
        && usb.channel_number == status_of_rradio.channel_number
        && !status_of_rradio.usb_mounted
    {
        //we must mount the usb device
        if let Err(error) = mount_usb(usb, status_of_rradio) {
            return Err(error.to_lcd_screen());
        }
    }

    Ok(())
}

fn mount_samba_if_necessary(
    status_of_rradio: &mut PlayerStatus,
    config: &crate::read_config::Config,
) -> Result<(), String> {
    if let Some(samba) = &config.samba
        && samba.channel_number == status_of_rradio.channel_number
        && !status_of_rradio.samba_mounted
    {
        //we must mount the samba device
        if let Err(error) = mount_samba::mount_samba(samba, status_of_rradio) {
            return Err(error.to_lcd_screen());
        }
    }
    Ok(())
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
    /// Sets the volume; returns an error string if it fails
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
            .unset_by_nick("video") // remove the video flag, which means we cannot process video & do not waste time trying to do so.
            .unset_by_nick("text") // remove the text flag, which means we cannot process subtitles & do not waste time trying to do so.
            .build()
            .ok_or("Failed to unset the unwanted gstreamer flags")?; // question mark says "if there is an error,  return from here with the string specified, otherwise continue"

        playbin_element.set_property_from_value("flags", &flags);
        // at this point we have a playbin element with the wanted flags , ie the default with "text" & "video" removed
        //(actually "Deinterlace video if necessary" & "Use software color balance" remain)

        if let Some(buffering_duration) = config.buffering_duration {
            // the duration is specified in the config file

            if let Ok(duration_as_nanos) = i64::try_from(buffering_duration.as_nanos()) {
                playbin_element.set_property("buffer-duration", duration_as_nanos);
            } else {
                eprintln!("Failed to set the buffer duration\r")
            }
        }

        let bus = playbin_element
            .bus()
            .ok_or("The gstreamer playbin's message bus is missing")?
            .stream();

        Ok((PlaybinElement { playbin_element }, bus))
    }

    /// set the state of gstreamer to be the one specified; we use Paused, Playing or Null
    pub fn set_state(
        &self,
        new_state: gstreamer::State,
    ) -> Result<gstreamer::StateChangeSuccess, gstreamer::StateChangeError> {
        self.playbin_element.set_state(new_state)
    }

    /// Plays the first track aka station specified by player_status
    /// seeks to the previous position if the media is seekable
    /// if status is channel not found, it plays a ding, if one has been specified
    /// If it fails the error message is returned as an Err(String)
    pub fn play_track(
        &self,
        status_of_rradio: &mut PlayerStatus,
        config: &crate::read_config::Config,
        lcd: &mut crate::lcd::Lc,
        seek_wanted_if_possible: bool,
    ) -> Result<(), String> {
        mount_usb_if_necessary(status_of_rradio, config)?; // returns the error value that mount_usb_if_necessary if mount_usb_if_necessary returns 

        if let Some(usb) = &config.usb
            && usb.channel_number != status_of_rradio.channel_number 
            && let Err (error) = unmount_if_needed(&usb.local_mount_folder, &mut status_of_rradio.usb_mounted) {
                eprintln!("when unmounting USB stick as it was not being used, got error {}\r", error)
        }

        mount_samba_if_necessary(status_of_rradio, config)?;

        if let Err(error) = self.set_state(gstreamer::State::Null) {
            return Err(format!(
                "Failed to set gstreamer to Null; got error {}",
                error
            ));
        }

        // next we might have to play a ding, but before we try, check that one has been specified
        match status_of_rradio.running_status {
            RunningStatus::NoChannel
            | RunningStatus::NoChannelRepeated
            | RunningStatus::LongMessageOnAll4Lines => {
                if config.aural_notifications.filename_error.is_none() {
                    return Ok(());
                }
            } // return without playing as we ought to play a ding, but one has not been specified
            _ => {}
        }
        if (status_of_rradio.channel_number == player_status::START_UP_DING_CHANNEL_NUMBER)
            && (status_of_rradio.position_and_duration[player_status::START_UP_DING_CHANNEL_NUMBER]
                .channel_data
                .station_urls
                .is_empty())
        {
            return Ok(()); // we ought to play a ding, but one has not been specified, so return OK as we cannot
        }
        let mut index_to_current_track = status_of_rradio.position_and_duration
            [status_of_rradio.channel_number]
            .index_to_current_track;
        let channel_number = match status_of_rradio.running_status {
            RunningStatus::NoChannel
            | RunningStatus::NoChannelRepeated
            | RunningStatus::LongMessageOnAll4Lines => {
                index_to_current_track = 0; //there is only one track
                player_status::START_UP_DING_CHANNEL_NUMBER
            }
            _ => status_of_rradio.channel_number,
        };

        //next check that the index is in bounds to stop panicks occuring if there is a bug
        if status_of_rradio.position_and_duration[channel_number].index_to_current_track
            >= status_of_rradio.position_and_duration[channel_number]
                .channel_data
                .station_urls
                .len()
        {
            // as index_to_current_track is a usize, there is no need to check it it is not negative
            eprintln!(
                "On channel {} Index to tracks out of bounds; it is {} and the list has {} elements\r",
                channel_number,
                status_of_rradio.position_and_duration[channel_number].index_to_current_track,
                status_of_rradio.position_and_duration[channel_number]
                    .channel_data
                    .station_urls
                    .len()
            );
            return Err(format!(
                "On channel {} Index to tracks out of bounds; it is {} and the list has {} elements",
                channel_number,
                index_to_current_track,
                status_of_rradio.position_and_duration[channel_number]
                    .channel_data
                    .station_urls
                    .len()
            ));
        }

        self.playbin_element.set_property(
            "uri",
            // if "uri" does not exist, it panics, but that does not seem to be anything that can be done about it.
            &status_of_rradio.position_and_duration[channel_number]
                .channel_data
                .station_urls[index_to_current_track],
        );

        if let Some(pause_before_playing_ms) = status_of_rradio.position_and_duration
            [channel_number]
            .channel_data
            .pause_before_playing_ms
        {
            if self
                .playbin_element
                .set_state(gstreamer::State::Paused)
                .is_err()
            {
                eprintln!("gsteamer pause failed\r"); // if it fails, there is not much we can do about it; but at least the message might be seen
            }

            let mut text_buffer = TextBuffer::new();
            text_buffer.write_text_to_lines("Filling buffer".bytes(), LineNum::Line1, 1);
            text_buffer.write_text_to_lines(
                format!("for channel {}", channel_number).bytes(),
                LineNum::Line2,
                3,
            );
            lcd.write_text_buffer_to_lcd(&text_buffer);
            std::thread::sleep(std::time::Duration::from_millis(pause_before_playing_ms));
        }
        match self
            .playbin_element //clone here makes it stop working
            .set_state(gstreamer::State::Playing)
        {
            Ok(_playing_worked_okf) => {
                if seek_wanted_if_possible {
                    match status_of_rradio.position_and_duration[channel_number]
                        .channel_data
                        .source_type
                    {
                        SourceType::Cd | SourceType::Usb | SourceType::Samba => {
                            let seek_time = gstreamer::ClockTime::from_seconds(
                                status_of_rradio.position_and_duration[channel_number]
                                    .position
                                    .num_seconds() as u64,
                            ); // the position we will seek to in the units needed.
                            // we use seconds as the unit as that is directly avaialble AND without an "Option"
                            let mut can_seek = false; // note we cannot perform a gstreamer seek yet 

                            for _count in 0..100 {
                                // wait till gstreamer is ready for a seek, or time out if eventually we do not get a position
                                if self
                                    .playbin_element
                                    .query_position::<gstreamer::ClockTime>()
                                    .is_some()
                                {
                                    can_seek = true; // as we got a postion, we can now seek
                                    break;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }

                            if self
                                .playbin_element
                                .seek_simple(
                                    SeekFlags::FLUSH
                                        | SeekFlags::KEY_UNIT
                                        | SeekFlags::SNAP_NEAREST,
                                    seek_time,
                                )
                                .is_err()
                            {
                                return Err(if can_seek  {"Failed to seek"} 
                                            else {
                                                "failed to seek, probably because could not get a gstreamer position"}.to_string());
                            }
                            return Ok(());
                        }
                        SourceType::UnknownSource | SourceType::UrlList => {
                            return Ok(()); // we are playing OK & not seeking , so there is nothing to do.
                        }
                    }
                }

                Ok(())
            }
            Err(error_message) => {
                Err(format!("Failed to set the URL. Got error {:?}", error_message).to_string())
            }
        }
    }
}
