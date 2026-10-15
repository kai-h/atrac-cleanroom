//! Stateful scheduling, independently testable without substituting a codec.
//! Shared by both production codec backends.
use crate::{Error, FRAME_SAMPLES};

const CHANNELS: usize = 2;
const LEAD_IN: usize = 955;

pub(crate) trait FrameEncoder {
    fn encode(&mut self, pcm: &[i16; FRAME_SAMPLES * CHANNELS]) -> Result<Vec<u8>, Error>;
    fn frame_bytes(&self) -> usize;
}

pub(crate) struct Stream<B> {
    backend: B,
    pending: [i16; FRAME_SAMPLES * CHANNELS],
    filled: usize,
    calls: u64,
    samples: u64,
    finished: bool,
    failed: bool,
}

impl<B: FrameEncoder> Stream<B> {
    pub(crate) fn new(backend: B) -> Self {
        Self {
            backend,
            pending: [0; FRAME_SAMPLES * CHANNELS],
            filled: LEAD_IN * CHANNELS,
            calls: 0,
            samples: 0,
            finished: false,
            failed: false,
        }
    }

    fn run(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        let frame = match self.backend.encode(&self.pending) {
            Ok(f) if f.len() == self.backend.frame_bytes() => f,
            Ok(_) => {
                self.failed = true;
                return Err(Error::InvalidFrame);
            }
            Err(e) => {
                self.failed = true;
                return Err(e);
            }
        };
        if self.calls >= 2 {
            out.extend(frame);
        }
        self.calls += 1;
        self.filled = 0;
        Ok(())
    }

    pub(crate) fn write(&mut self, pcm: &[i16]) -> Result<Vec<u8>, Error> {
        // Validate before touching any state, including counters.
        if pcm.len() % CHANNELS != 0 {
            return Err(Error::IncompleteSample);
        }
        if self.finished || self.failed {
            return Err(Error::Finished);
        }
        let samples = self
            .samples
            .checked_add((pcm.len() / CHANNELS) as u64)
            .ok_or(Error::LengthOverflow)?;
        samples
            .checked_add(LEAD_IN as u64)
            .ok_or(Error::LengthOverflow)?;
        let mut out = Vec::new();
        let mut rest = pcm;
        while !rest.is_empty() {
            let n = rest.len().min(self.pending.len() - self.filled);
            self.pending[self.filled..self.filled + n].copy_from_slice(&rest[..n]);
            self.filled += n;
            rest = &rest[n..];
            if self.filled == self.pending.len() {
                self.run(&mut out)?;
            }
        }
        self.samples = samples;
        Ok(out)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if self.finished || self.failed {
            return Err(Error::Finished);
        }
        let remainder = self.filled / CHANNELS;
        let mut out = Vec::new();
        if remainder != 0 {
            self.pending[self.filled..].fill(0);
            self.run(&mut out)?;
        }
        self.pending.fill(0);
        for _ in 0..if remainder > 0 && remainder < 886 {
            2
        } else {
            3
        } {
            self.run(&mut out)?;
        }
        self.finished = true;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mode;

    // A stateful block fingerprint tests scheduling, not ATRAC conformance.
    // Production split-invariance must additionally run each real backend.
    struct Recorder {
        hash: u64,
        mode: Mode,
    }
    impl FrameEncoder for Recorder {
        fn frame_bytes(&self) -> usize {
            self.mode.frame_bytes()
        }
        fn encode(&mut self, pcm: &[i16; 2048]) -> Result<Vec<u8>, Error> {
            for &x in pcm {
                self.hash = self
                    .hash
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(x as u16 as u64);
            }
            let mut b = vec![0; self.frame_bytes()];
            b[..8].copy_from_slice(&self.hash.to_le_bytes());
            Ok(b)
        }
    }
    fn make(mode: Mode) -> Stream<Recorder> {
        Stream::new(Recorder { hash: 1, mode })
    }
    fn pcm(n: usize) -> Vec<i16> {
        (0..n * 2).map(|i| (i.wrapping_mul(1973)) as i16).collect()
    }
    #[test]
    fn scheduling_is_split_invariant() {
        for mode in [Mode::Lp2, Mode::Stereo105, Mode::Lp4] {
            for n in [
                0, 1, 68, 69, 70, 885, 886, 1024, 6144, 6213, 7098, 7099, 32769,
            ] {
                let input = pcm(n);
                let mut whole = make(mode);
                let mut expected = whole.write(&input).unwrap();
                expected.extend(whole.finish().unwrap());
                for split in [1, 1023, 1024, 1025, 65536] {
                    let mut stream = make(mode);
                    let mut actual = Vec::new();
                    for part in input.chunks(split * 2) {
                        assert!(stream.write(&[]).unwrap().is_empty());
                        actual.extend(stream.write(part).unwrap());
                    }
                    actual.extend(stream.finish().unwrap());
                    assert_eq!(actual, expected, "{mode:?}: {n} samples, split {split}");
                }
                let mut stream = make(mode);
                let mut actual = Vec::new();
                let mut i = 0;
                let mut seed = 17u32;
                while i < n {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let end = n.min(i + 1 + (seed as usize % 4097));
                    actual.extend(stream.write(&input[2 * i..2 * end]).unwrap());
                    i = end;
                }
                actual.extend(stream.finish().unwrap());
                assert_eq!(actual, expected);
            }
        }
    }
    #[test]
    fn bad_writes_are_atomic_and_finish_is_terminal() {
        let mut a = make(Mode::Lp2);
        let mut b = make(Mode::Lp2);
        assert_eq!(a.write(&[1]), Err(Error::IncompleteSample));
        assert_eq!(a.write(&pcm(6144)).unwrap(), b.write(&pcm(6144)).unwrap());
        assert_eq!(a.finish().unwrap(), b.finish().unwrap());
        assert_eq!(a.finish(), Err(Error::Finished));
        assert_eq!(a.write(&[]), Err(Error::Finished));
    }
    #[test]
    fn published_frame_counts() {
        for (samples, count) in [(6144, 8), (6213, 8), (7098, 8), (7099, 9)] {
            let mut s = make(Mode::Lp2);
            let a = s.write(&pcm(samples)).unwrap();
            let b = s.finish().unwrap();
            assert_eq!((a.len() + b.len()) / 384, count);
            assert_eq!(a.len() / 384, ((samples + 955) / 1024).saturating_sub(2));
        }
    }
}
