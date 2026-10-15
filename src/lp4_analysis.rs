//! Chapter 8 §§5 and 8. Each binary32 operation is intentionally separate.
use crate::tables::lp4 as t;
pub(crate) type Bands = [[f32; 4]; 256];

pub(crate) struct Qmf {
    input: [f32; 46],
    vectors: [[f32; 4]; 23],
}
impl Default for Qmf {
    fn default() -> Self {
        Self {
            input: [0.0; 46],
            vectors: [[0.0; 4]; 23],
        }
    }
}
impl Qmf {
    pub(crate) fn process(&mut self, pcm: &[f32; 1024]) -> Bands {
        let mut h = [0.0f32; 1070];
        h[..46].copy_from_slice(&self.input);
        h[46..].copy_from_slice(pcm);
        let mut v = [[0.0f32; 4]; 279];
        v[..23].copy_from_slice(&self.vectors);
        let mut out = [[0.0f32; 4]; 256];
        for i in 0..256 {
            let base = 40 + 4 * i;
            let mut a = [0.0; 4];
            let mut b = [0.0; 4];
            for lane in 0..4 {
                a[lane] = t::QMF1[11][lane] * h[base + 4 + lane];
                b[lane] = t::QMF1[11][lane] * h[base + 6 + lane];
            }
            for k in 0..11 {
                for lane in 0..4 {
                    a[lane] += t::QMF1[k][lane] * h[base - 40 + 4 * k + lane];
                    b[lane] += t::QMF1[k][lane] * h[base - 40 + 4 * k + 2 + lane];
                }
            }
            let u = [a[2] + a[0], b[2] + b[0], a[3] + a[1], b[3] + b[1]];
            v[23 + i] = [u[0] + u[2], u[1] + u[3], u[2] - u[0], u[3] - u[1]];
            let mut w = [0.0; 4];
            for lane in 0..4 {
                w[lane] = t::QMF2[23][lane] * v[23 + i][lane];
            }
            for k in 0..23 {
                for lane in 0..4 {
                    w[lane] += t::QMF2[k][lane] * v[i + k][lane];
                }
            }
            out[i] = [w[0] + w[1], w[1] - w[0], w[3] - w[2], w[2] + w[3]];
        }
        self.input.copy_from_slice(&h[1024..]);
        self.vectors.copy_from_slice(&v[256..]);
        out
    }
}

pub(crate) struct Mdct {
    state: [[f32; 4]; 128],
}
impl Default for Mdct {
    fn default() -> Self {
        Self {
            state: [[0.0; 4]; 128],
        }
    }
}
impl Mdct {
    pub(crate) fn process(&mut self, previous: &Bands, first_levels: [usize; 4]) -> [f32; 1024] {
        let mut y = [[0.0f32; 4]; 256];
        let mut next = [[0.0f32; 4]; 128];
        for j in 0..128 {
            let p = t::MDCT_PERM[j] as usize;
            let [r0, r1, r2, r3] = t::MDCT_TW[p];
            for lane in 0..4 {
                let a = previous[128 + p][lane];
                let b = previous[127 - p][lane];
                let s = self.state[j][lane] * t::GAIN_POW2[first_levels[lane]];
                next[j][lane] = r1 * (a * r0 - b);
                let z = r2 * (b * r0 + a);
                let d = r3 * (z - s);
                y[2 * j][lane] = (z + s) + d;
                y[2 * j + 1][lane] = d;
            }
        }
        for j in 0..64 {
            let f = t::MDCT_FFT_TW[2 * j];
            for lane in 0..4 {
                let a = y[4 * j][lane];
                let b = y[4 * j + 1][lane];
                let c = y[4 * j + 2][lane];
                let d = y[4 * j + 3][lane];
                let e3 = f * (b - d);
                let e0 = f * (a - c);
                y[4 * j][lane] = (a + c) + e3;
                y[4 * j + 1][lane] = (b + d) + e0;
                y[4 * j + 2][lane] = e0;
                y[4 * j + 3][lane] = e3;
            }
        }
        for n in [4, 8, 16, 32, 64] {
            for k in 0..256 / (2 * n) {
                let start = 2 * n * k;
                let old = y;
                for i in 0..n {
                    for lane in 0..4 {
                        let high = t::MDCT_FFT_TW[n / 2 - 1 + n * k]
                            * (old[start + i][lane] - old[start + n + i][lane]);
                        let reverse = t::MDCT_FFT_TW[n / 2 - 1 + n * k]
                            * (old[start + n - 1 - i][lane] - old[start + 2 * n - 1 - i][lane]);
                        y[start + i][lane] =
                            (old[start + i][lane] + old[start + n + i][lane]) + reverse;
                        y[start + n + i][lane] = high;
                    }
                }
            }
        }
        for i in 0..128 {
            for lane in 0..4 {
                let a = y[i][lane];
                let b = y[i + 128][lane];
                y[i][lane] = a + b;
                y[i + 128][lane] = t::MDCT_SQRT_HALF * (a - b);
            }
        }
        let mut out = [0.0; 1024];
        for b in 0..4 {
            for i in 0..128 {
                let z = y[i][b] + y[255 - i][b];
                let tail = y[255 - i][b];
                out[b * 256 + i] = if b % 2 == 0 { z } else { tail };
                out[b * 256 + 255 - i] = if b % 2 == 0 { tail } else { z };
            }
        }
        self.state = next;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Frozen bit-pattern checkpoints, independently exported from the supplied
    // JSON fixtures. The exporter does not execute reference implementation code.
    #[allow(dead_code)]
    mod fixtures {
        include!("../tests/data/lp4_kernel_fixtures.rs");
    }
    use fixtures::*;
    #[test]
    fn qmf_matches_frozen_stages() {
        for channel in 0..2 {
            let mut q = Qmf::default();
            for call in 0..3 {
                let mut pcm = [0.0; 1024];
                for i in 0..1024 {
                    pcm[i] = PCM[(call * 1024 + i) * 2 + channel] as f32;
                }
                let out = q.process(&pcm);
                for i in 0..256 {
                    for b in 0..4 {
                        assert_eq!(
                            out[i][b].to_bits(),
                            QMF[call][channel][i * 4 + b],
                            "call={call} channel={channel} sample={i} band={b}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn mdct_matches_frozen_stages() {
        for channel in 0..2 {
            let mut m = Mdct::default();
            for call in 0..3 {
                let mut input = [[0.0; 4]; 256];
                for i in 0..256 {
                    for b in 0..4 {
                        input[i][b] = f32::from_bits(MDCT_INPUT[call][channel][i * 4 + b]);
                    }
                }
                let out = m.process(&input, FIRST_LEVELS[call][channel]);
                for i in 0..1024 {
                    assert_eq!(
                        out[i].to_bits(),
                        SPECTRUM[call][channel][i],
                        "call={call} channel={channel} coefficient={i}"
                    );
                }
            }
        }
    }
}
