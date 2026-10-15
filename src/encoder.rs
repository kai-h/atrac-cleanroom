use crate::{
    Error, Mode,
    lp4_allocator::allocate,
    lp4_frontend::Analysis,
    stream::{FrameEncoder, Stream},
};
pub(crate) struct Algorithm0 {
    analysis: Analysis,
    joint: bool,
}
impl Algorithm0 {
    pub fn new(joint: bool) -> Self {
        Self {
            analysis: Analysis::new(joint),
            joint,
        }
    }
}
impl FrameEncoder for Algorithm0 {
    fn frame_bytes(&self) -> usize {
        if self.joint { 192 } else { 304 }
    }
    fn encode(&mut self, pcm: &[i16; 2048]) -> Result<Vec<u8>, Error> {
        let (spectra, budget, old) = self.analysis.process(pcm);
        let limits = if self.joint { [27, 12] } else { [28, 28] };
        let mut gains: [Vec<(i32, usize)>; 4] =
            std::array::from_fn(|b| self.analysis.gain[0].bands[b].points.clone());
        let a = allocate(spectra[0], budget, limits[0], &mut gains, old[0], false, 7)?;
        let (mut u0, bits) = a.pack(&gains, None)?;
        for b in 0..4 {
            self.analysis.gain[0].bands[b].points = gains[b].clone();
        }
        if bits > budget as usize + 19 {
            return Err(Error::InvalidFrame);
        }
        let b1 = if self.joint {
            (192 - u0.len()) as i32 * 8 - 27
        } else {
            1197
        };
        let mut gains = std::array::from_fn(|b| self.analysis.gain[1].bands[b].points.clone());
        let a = allocate(
            spectra[1],
            b1,
            limits[1],
            &mut gains,
            old[1],
            self.joint,
            self.analysis.stereo.sent_weight,
        )?;
        let header = if self.joint {
            Some((self.analysis.stereo.sent_weight, self.analysis.stereo.sent))
        } else {
            None
        };
        let (mut u1, bits) = a.pack(&gains, header)?;
        for b in 0..4 {
            self.analysis.gain[1].bands[b].points = gains[b].clone();
        }
        if bits > b1 as usize + if self.joint { 27 } else { 19 } {
            return Err(Error::InvalidFrame);
        }
        if self.joint {
            if u0.len() + u1.len() > 192 {
                return Err(Error::InvalidFrame);
            }
            u0.resize(192 - u1.len(), 0);
            u1.reverse();
            u0.extend(u1);
        } else {
            if u0.len() > 152 || u1.len() > 152 {
                return Err(Error::InvalidFrame);
            }
            u0.resize(152, 0);
            u1.resize(152, 0);
            u0.extend(u1);
        }
        Ok(u0)
    }
}
enum Backend {
    Algorithm0(Algorithm0),
    Algorithm1([crate::lp2_analysis::Analysis; 2]),
}
impl FrameEncoder for Backend {
    fn frame_bytes(&self) -> usize {
        match self {
            Self::Algorithm0(a) => a.frame_bytes(),
            Self::Algorithm1(_) => 384,
        }
    }
    fn encode(&mut self, pcm: &[i16; 2048]) -> Result<Vec<u8>, Error> {
        match self {
            Self::Algorithm0(a) => a.encode(pcm),
            Self::Algorithm1(channels) => {
                let mut out = Vec::with_capacity(384);
                for (c, analysis) in channels.iter_mut().enumerate() {
                    let samples = std::array::from_fn(|i| pcm[2 * i + c] as f32);
                    let (a, b, current, previous) = analysis.process(&samples)?;
                    let mut unit = crate::lp2_allocator::allocate(a, b, &current, &previous)?;
                    if unit.len() > 192 {
                        return Err(Error::InvalidFrame);
                    }
                    unit.resize(192, 0);
                    out.extend(unit);
                }
                Ok(out)
            }
        }
    }
}
/// Streaming 44.1 kHz interleaved stereo encoder. Counts are per channel.
pub struct Encoder {
    stream: Stream<Backend>,
    mode: Mode,
}
impl Encoder {
    pub fn new(mode: Mode) -> Result<Self, Error> {
        let backend = match mode {
            Mode::Lp2 => Backend::Algorithm1(std::array::from_fn(|_| {
                crate::lp2_analysis::Analysis::default()
            })),
            _ => Backend::Algorithm0(Algorithm0::new(mode == Mode::Lp4)),
        };
        Ok(Self {
            stream: Stream::new(backend),
            mode,
        })
    }
    pub fn frame_bytes(&self) -> usize {
        self.mode.frame_bytes()
    }
    pub fn write(&mut self, pcm: &[i16]) -> Result<Vec<u8>, Error> {
        self.stream.write(pcm)
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        self.stream.finish()
    }
}
