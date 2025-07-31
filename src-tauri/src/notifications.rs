use rodio::{Decoder, OutputStream, Sink};
use std::{fs::File, io::BufReader, path::PathBuf};
use tauri::{path::BaseDirectory, AppHandle, Manager};

use crate::error::{Error, Result};

pub fn play_finish(app_handle: &AppHandle) -> Result<()> {
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;
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
