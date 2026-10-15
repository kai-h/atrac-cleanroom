use atrac_cleanroom::{Encoder, Mode};
use std::{fs, io};
fn pcm(bytes: &[u8]) -> Result<Vec<i16>, Box<dyn std::error::Error>> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Expected stereo 44.1 kHz 16-bit PCM WAV or raw s16le",
        )
    };
    let data = if bytes.starts_with(b"RIFF") {
        if bytes.get(8..12) != Some(&b"WAVE"[..]) {
            return Err(invalid().into());
        }
        let mut pos = 12;
        let mut valid = false;
        let mut data = None;
        while pos + 8 <= bytes.len() {
            let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into()?) as usize;
            let end = pos
                .checked_add(8)
                .and_then(|p| p.checked_add(size))
                .ok_or_else(invalid)?;
            let chunk = bytes.get(pos + 8..end).ok_or_else(invalid)?;
            match &bytes[pos..pos + 4] {
                b"fmt " => {
                    valid = chunk.len() >= 16
                        && u16::from_le_bytes(chunk[0..2].try_into()?) == 1
                        && u16::from_le_bytes(chunk[2..4].try_into()?) == 2
                        && u32::from_le_bytes(chunk[4..8].try_into()?) == 44100
                        && u16::from_le_bytes(chunk[12..14].try_into()?) == 4
                        && u16::from_le_bytes(chunk[14..16].try_into()?) == 16;
                }
                b"data" => data = Some(chunk),
                b"smpl" => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "PSP loop metadata is not supported",
                    )
                    .into());
                }
                _ => {}
            }
            pos = end.checked_add(size % 2).ok_or_else(invalid)?;
        }
        if !valid {
            return Err(invalid().into());
        }
        data.ok_or_else(invalid)?
    } else {
        bytes
    };
    if data.len() % 4 != 0 {
        return Err(invalid().into());
    }
    Ok(data
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect())
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err(
            "Usage: atrac-encode <132|105|66> <input.wav|input.s16le> <output.frames>".into(),
        );
    }
    let mode = Mode::from_bitrate(args[1].parse()?, 2)?;
    let input = pcm(&fs::read(&args[2])?)?;
    let mut encoder = Encoder::new(mode)?;
    let mut output = Vec::new();
    for part in input.chunks(32768) {
        output.extend(encoder.write(part)?);
    }
    output.extend(encoder.finish()?);
    fs::write(&args[3], output)?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
