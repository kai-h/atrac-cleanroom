use atrac_cleanroom::{Encoder, Error, Mode};
use std::path::Path;
fn pcm(path: &Path) -> Vec<i16> {
    let bytes = std::fs::read(path).unwrap();
    let mut p = 12;
    while p + 8 <= bytes.len() {
        let n = u32::from_le_bytes(bytes[p + 4..p + 8].try_into().unwrap()) as usize;
        if &bytes[p..p + 4] == b"data" {
            return bytes[p + 8..p + 8 + n]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();
        }
        p += 8 + n + (n % 2);
    }
    panic!("no data");
}
#[test]
fn matches_all_golden_frames() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/vectors");
    for (mode, extension) in [(Mode::Lp2, "lp2.frames"), (Mode::Lp4, "lp4.frames")] {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().is_none_or(|x| x != "wav") {
                continue;
            }
            let expected = p.with_extension(extension);
            if !expected.exists() {
                continue;
            }
            let input = pcm(&p);
            let mut e = Encoder::new(mode).unwrap();
            let mut out = e.write(&input).unwrap();
            out.extend(e.finish().unwrap());
            let expected = std::fs::read(expected).unwrap();
            if out != expected {
                let first = out.iter().zip(&expected).position(|(a, b)| a != b);
                panic!(
                    "{}: first mismatch {first:?}, lengths {} / {}",
                    p.display(),
                    out.len(),
                    expected.len()
                );
            }
        }
    }
}
#[test]
fn actual_codec_split_invariance() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/vectors");
    let input = pcm(&dir.join("transients.wav"));
    for mode in [Mode::Lp2, Mode::Lp4, Mode::Stereo105] {
        let mut e = Encoder::new(mode).unwrap();
        let mut expected = e.write(&input).unwrap();
        expected.extend(e.finish().unwrap());
        for n in [1, 1023, 1024, 1025, 65536] {
            let mut e = Encoder::new(mode).unwrap();
            let mut out = Vec::new();
            for part in input.chunks(n * 2) {
                assert_eq!(e.write(&[1]), Err(Error::IncompleteSample));
                assert!(e.write(&[]).unwrap().is_empty());
                out.extend(e.write(part).unwrap());
            }
            out.extend(e.finish().unwrap());
            assert_eq!(out, expected, "mode={mode:?}, split={n}");
        }
    }
}

#[test]
fn short_streams_and_adversarial_pcm() {
    for mode in [Mode::Lp2, Mode::Stereo105, Mode::Lp4] {
        for samples in [
            0, 1, 2, 68, 69, 70, 885, 886, 1023, 1024, 1025, 2116, 2117, 6213, 7098, 7099,
        ] {
            let input: Vec<i16> = (0..samples * 2)
                .map(|i| if i % 2 == 0 { i16::MIN } else { i16::MAX })
                .collect();
            let mut e = Encoder::new(mode).unwrap();
            let mut expected = e.write(&input).unwrap();
            expected.extend(e.finish().unwrap());
            let mut e = Encoder::new(mode).unwrap();
            let mut actual = Vec::new();
            for part in input.chunks(2) {
                actual.extend(e.write(part).unwrap());
            }
            actual.extend(e.finish().unwrap());
            assert_eq!(actual, expected);
            assert_eq!(actual.len() % mode.frame_bytes(), 0);
        }
        let mut seed = 19u32;
        for pattern in 0..8 {
            let mut e = Encoder::new(mode).unwrap();
            for frame in 0..80 {
                let mut pcm = [0i16; 2048];
                for (i, v) in pcm.iter_mut().enumerate() {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    *v = match pattern {
                        0 => 0,
                        1 => i16::MIN,
                        2 => i16::MAX,
                        3 => {
                            if (i + frame) % 2 == 0 {
                                i16::MIN
                            } else {
                                i16::MAX
                            }
                        }
                        4 => (seed >> 16) as i16,
                        5 => {
                            if i == frame {
                                i16::MAX
                            } else {
                                0
                            }
                        }
                        6 => ((seed >> 16) as i16) % 3,
                        _ => {
                            if i % 2 == 0 {
                                (seed >> 16) as i16
                            } else {
                                0
                            }
                        }
                    };
                }
                let out = e.write(&pcm).unwrap();
                assert_eq!(out.len() % mode.frame_bytes(), 0);
            }
            e.finish().unwrap();
        }
    }
}
