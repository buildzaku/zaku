use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;

pub fn play_notif_sound() -> Result<(), std::io::Error> {
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
    let sound_file = File::open("assets/sounds/glass.wav")?;
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
