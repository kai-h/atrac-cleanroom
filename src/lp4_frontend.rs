use crate::lp4_analysis::{Bands, Mdct, Qmf};
use crate::lp4_gain::GainControl;
use crate::lp4_stereo::{Stereo, split_budget};

pub(crate) struct Analysis {
    qmf: [Qmf; 2],
    mdct: [Mdct; 2],
    pub gain: [GainControl; 2],
    previous: [Bands; 2],
    pub stereo: Stereo,
    joint: bool,
}
impl Analysis {
    pub(crate) fn new(joint: bool) -> Self {
        Self {
            qmf: std::array::from_fn(|_| Qmf::default()),
            mdct: std::array::from_fn(|_| Mdct::default()),
            gain: std::array::from_fn(|_| GainControl::default()),
            previous: [[[0.0; 4]; 256]; 2],
            stereo: Stereo::default(),
            joint,
        }
    }
    pub(crate) fn process(&mut self, pcm: &[i16; 2048]) -> ([[f32; 1024]; 2], i32, [usize; 2]) {
        let raw = std::array::from_fn(|c| {
            let p = std::array::from_fn(|i| pcm[2 * i + c] as f32);
            self.qmf[c].process(&p)
        });
        let converted = if self.joint {
            self.stereo.process(raw[0], raw[1])
        } else {
            raw
        };
        let mut spectra = [[0.0; 1024]; 2];
        let mut old = [0; 2];
        for c in 0..2 {
            let (modified, count) = self.gain[c].process(
                &converted[c],
                &self.previous[c],
                self.joint && c == 1,
                self.stereo.sent,
                self.stereo.current,
            );
            let levels = std::array::from_fn(|b| {
                self.gain[c].bands[b]
                    .points
                    .first()
                    .map_or(4, |p| p.0 as usize)
            });
            spectra[c] = self.mdct[c].process(&modified, levels);
            old[c] = count;
        }
        self.previous = converted;
        let budget = if self.joint {
            split_budget(
                &spectra[0],
                &spectra[1],
                self.stereo.sent[0],
                self.gain[1].bands[0].points.len(),
            )
        } else {
            1197
        };
        (spectra, budget, old)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[allow(dead_code)]
    mod fixtures {
        include!("../tests/data/lp4_kernel_fixtures.rs");
    }
    use fixtures::*;
    #[test]
    fn integrated_analysis_matches_frozen_stages() {
        let mut a = Analysis::new(true);
        for call in 0..3 {
            let p = std::array::from_fn(|i| PCM[call * 2048 + i]);
            let (s, budget, _) = a.process(&p);
            for c in 0..2 {
                for i in 0..1024 {
                    assert_eq!(
                        s[c][i].to_bits(),
                        SPECTRUM[call][c][i],
                        "call={call} c={c} i={i}"
                    );
                }
            }
            assert_eq!(budget, BUDGETS[call]);
            assert_eq!(a.stereo.sent, SENT[call]);
            assert_eq!(a.stereo.sent_weight, WEIGHT[call]);
            for c in 0..2 {
                for b in 0..4 {
                    assert_eq!(a.gain[c].bands[b].points, GAINS[call][c * 4 + b]);
                }
            }
        }
    }
}
