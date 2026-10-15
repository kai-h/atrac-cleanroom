use crate::lp4_analysis::Bands;
use crate::tables::lp4 as t;
fn f32(x: f64) -> f64 {
    x as f32 as f64
}
pub(crate) struct Stereo {
    pub current: [i32; 4],
    pub sent: [i32; 4],
    pub weight: usize,
    pub sent_weight: usize,
    side: [f64; 4],
    energies: [[f32; 2]; 4],
}
impl Default for Stereo {
    fn default() -> Self {
        Self {
            current: [-3; 4],
            sent: [0; 4],
            weight: 15,
            sent_weight: 0,
            side: [1.0; 4],
            energies: [[0.0; 2]; 4],
        }
    }
}
impl Stereo {
    pub(crate) fn process(&mut self, mut left: Bands, mut right: Bands) -> [Bands; 2] {
        let mut accum = [f32(0.1); 2];
        for b in 0..4 {
            let sums = [
                left.iter().map(|v| (v[b] as f64).abs()).sum::<f64>(),
                right.iter().map(|v| (v[b] as f64).abs()).sum::<f64>(),
            ];
            let [e0, e1] = sums.map(f32);
            self.energies[b] = [e0 as f32, e1 as f32];
            let mut k0 = t::JS_K_SIDE as f64;
            let mut k1 = k0;
            if b == 0 {
                k0 = t::JS_K_MATRIX as f64;
                k1 = k0;
                if self.current[b] == 0 {
                    k0 = t::JS_K_HYST as f64;
                } else if self.current[b] == 1 {
                    k1 = t::JS_K_HYST as f64;
                }
            }
            let decision = if k0 * sums[0] < sums[1] {
                0
            } else if k1 * sums[1] < sums[0] {
                1
            } else if sums[0] + sums[1] < t::JS_QUIET as f64 {
                -3
            } else {
                3
            };
            self.sent[b] = self.current[b].abs();
            self.current[b] = decision;
            if b >= 1 && decision.abs() == 3 {
                accum[0] = 16.0 * accum[0] + e0;
                accum[1] = 16.0 * accum[1] + e1;
            }
        }
        self.sent_weight = self.weight;
        let total = f32(accum[1] + accum[0]);
        let (minimum, side) = if accum[0] <= accum[1] {
            (accum[0], 0)
        } else {
            (f32(accum[1]), 8)
        };
        self.weight =
            (((minimum / total * 14.0 + 12582912.0) as f32).to_bits() as usize & 7) + side;
        if self.weight == 0 || self.weight == 8 {
            self.weight += 1;
        }
        for b in 0..4 {
            let d = self.current[b].unsigned_abs() as usize;
            let p = self.sent[b] as usize;
            if b == 0 {
                let [e0, e1] = self.energies[b].map(|x| x as f64);
                let mut r = 1.0;
                let factor = t::JS_RATIO_GAIN as f64;
                if d == 3 {
                    if e1 > 3.94 * e0 && e0 * 256.0 > e1 {
                        r = f32((e1 - e0) * factor / (e1 + e0));
                    } else if e0 > 3.94 * e1 && e1 * 256.0 > e0 {
                        r = f32((e0 - e1) * factor / (e1 + e0));
                    } else if e1 == 0.0 || e0 == 0.0 {
                        r = 1.0;
                    } else if e1 * 256.0 <= e0 || e0 * 256.0 <= e1 {
                        r = f32(5.0 / 3.0);
                    }
                }
                let q = self.side[b];
                if r - q >= 0.05 {
                    r = f32(q + 0.05);
                }
                if q - r >= 0.05 {
                    r = f32(q - 0.05);
                }
                self.side[b] = r;
                let c0 = t::CHSW_TABLE[d] as f64;
                let c1 = t::CHSW_TABLE[d + 1] as f64;
                let p0 = t::CHSW_TABLE[p] as f64;
                let p1 = t::CHSW_TABLE[p + 1] as f64;
                let c1r = c1 * r;
                let dc = f32((p0 - c0) / 8.0);
                let dw = (p1 * q - c1r) / 8.0;
                for i in 0..256 {
                    let x0 = left[i][b] as f64;
                    let x1 = right[i][b] as f64;
                    let mid = (x0 + x1) * 0.5;
                    let difference = if i < 8 {
                        let k = i as f64 - 8.0;
                        (x0 - (c0 - dc * k) * mid) / (c1r - k * dw)
                    } else {
                        (x0 - mid * c0) * (1.0 / c1r)
                    };
                    left[i][b] = mid as f32;
                    right[i][b] = difference as f32;
                }
            } else {
                let idx = |w: usize| {
                    if d == 0 {
                        (w ^ 8) + 8
                    } else if d == 1 {
                        w + 8
                    } else {
                        w & 7
                    }
                };
                let tc = t::CHWT_TABLE[idx(self.weight)] as f64;
                let tp = t::CHWT_TABLE[idx(self.sent_weight)] as f64;
                let delta = (tp - tc) / 8.0;
                for i in 0..256 {
                    let x0 = left[i][b] as f64;
                    let x1 = right[i][b] as f64;
                    left[i][b] = if i < 8 {
                        ((x1 + x0) / (tc - (i as f64 - 8.0) * delta)) as f32
                    } else {
                        ((x1 + x0) * (1.0 / tc)) as f32
                    };
                }
            }
        }
        [left, right]
    }
}
pub(crate) fn split_budget(
    a: &[f32; 1024],
    b: &[f32; 1024],
    matrix: i32,
    gain_count: usize,
) -> i32 {
    let peak = |v: &[f32; 1024], i: usize| {
        v[t::SB_START[i] as usize..t::SB_START[i + 1] as usize]
            .iter()
            .map(|x| x.abs() as f64)
            .fold(0.0, f64::max)
    };
    let mut s0 = 0.0;
    for i in (0..19).rev() {
        s0 += t::SB_WIDTH[i] as f64 * peak(a, i);
    }
    s0 = f32(s0);
    let mut s1 = 0.0;
    for i in 0..12 {
        s1 += t::SB_WIDTH[i] as f64 * peak(b, i);
    }
    let mut side = f32(2.0 * s1);
    if matrix != 3 {
        side = f32(2.0 * side);
    }
    let (total, base) = (1477, 1133);
    if side < 1.0 || (matrix != 3 && side < 4.0) {
        return total;
    }
    if gain_count == 0 && ((3.0 * side < s0 && matrix == 3) || 6.0 * side < s0) {
        let mut spare = total - base;
        let extra = if 12.0 * side <= s0 {
            spare
        } else {
            if 10.0 * side < s0 {
                spare = (1.5 * spare as f64) as i32;
            } else if 5.0 * side < s0 {
                spare = (1.25 * spare as f64) as i32;
            }
            ((s0 - side) / (2.0 * s0) * spare as f64) as i32
        };
        return base + (extra & !7);
    }
    if side >= s0 || matrix != 3 {
        base
    } else {
        base + ((((s0 - side) / (10.0 * s0) * (total - base) as f64) as i32) & !7)
    }
}
