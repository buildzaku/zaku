use rodio::{Decoder, OutputStream, Sink};
use std::{fs::File, io::BufReader};
use tauri::{path::BaseDirectory, AppHandle, Manager};

pub fn play_notif_sound(app_handle: &AppHandle) -> Result<(), std::io::Error> {
    let (_stream, stream_handle) = OutputStream::try_default().map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Audio stream error: {}", err),
        )
    })?;
    let sink = Sink::try_new(&stream_handle).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Sink creation error: {}", err),
        )
    })?;
    let sound_filepath = app_handle
        .path()
        .resolve("assets/sounds/glass.wav", BaseDirectory::Resource)
        .map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Path resolution error: {}", err),
            )
        })?;
    let sound_file = File::open(sound_filepath)?;
    let source = Decoder::new(BufReader::new(sound_file)).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Audio decode error: {}", err),
        )
    })?;

    sink.append(source);
    sink.sleep_until_end();

    return Ok(());
}
