use rodio::{Decoder, OutputStreamBuilder, Sink};
use std::{fs::File, io::BufReader, path::PathBuf};
use tauri::{AppHandle, Manager, path::BaseDirectory};

use crate::error::{Error, Result};

pub fn play_finish(app_handle: &AppHandle) -> Result<()> {
    let stream_handle = OutputStreamBuilder::open_default_stream()?;
    let sink = Sink::connect_new(stream_handle.mixer());
    let sound_filepath = app_handle
        .path()
        .resolve(
            PathBuf::from("assets").join("sounds").join("glass.wav"),
            BaseDirectory::Resource,
        )
        .map_err(|e| Error::FileNotFound(e.to_string()))?;
    let sound_file = File::open(sound_filepath)?;
    let source = Decoder::new(BufReader::new(sound_file))
        .map_err(|e| Error::FileReadError(e.to_string()))?;

    sink.append(source);
    sink.sleep_until_end();

    Ok(())
}
