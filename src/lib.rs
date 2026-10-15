#![forbid(unsafe_code)]

mod lp4_analysis;
mod lp4_frontend;
mod lp4_gain;
mod lp4_stereo;
mod stream;
mod tables;

/// All sample counts are per channel, never interleaved element counts.
pub const SAMPLE_RATE: usize = 44_100;
pub const FRAME_SAMPLES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Lp2,
    Stereo105,
    Lp4,
}
impl Mode {
    pub const fn frame_bytes(self) -> usize {
        match self {
            Self::Lp2 => 384,
            Self::Stereo105 => 304,
            Self::Lp4 => 192,
        }
    }
    pub fn from_bitrate(kbps: u32, channels: usize) -> Result<Self, Error> {
        if channels != 2 {
            return Err(Error::UnsupportedMode);
        }
        match kbps {
            132 => Ok(Self::Lp2),
            105 => Ok(Self::Stereo105),
            66 => Ok(Self::Lp4),
            _ => Err(Error::UnsupportedMode),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnsupportedMode,
    IncompleteSample,
    Finished,
    LengthOverflow,
    InvalidFrame,
    SpecificationGap(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

mod bitstream;
mod encoder;
mod lp4_allocator;
mod lp4_quantisation;
pub use encoder::Encoder;

mod lp2_allocator;
mod lp2_analysis;

#[cfg(target_arch = "wasm32")]
mod wasm;

/// Raw output from the default primed stream is delayed by this many decoded
/// samples per channel relative to the source (spec ATRAC3 §3.5).
pub const ENCODER_DELAY: u64 = 1093;

/// Complete frames available after write() has accepted `samples` per channel,
/// before finish(). Includes the one-time 955-sample lead-in and two discarded
/// analysis outputs. Returns None if the counter would overflow.
pub fn frames_after_write(samples: u64) -> Option<u64> {
    Some((samples.checked_add(955)? / 1024).saturating_sub(2))
}

/// Total frames after finish(), including zero-padding and tail drain.
pub fn frames_after_finish(samples: u64) -> Option<u64> {
    let total = samples.checked_add(955)?;
    let whole = total / 1024;
    let remainder = total % 1024;
    Some(whole + u64::from(remainder != 0) + u64::from(remainder == 0 || remainder >= 886))
}
