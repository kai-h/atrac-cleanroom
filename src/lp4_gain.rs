use crate::lp4_analysis::Bands;
use crate::tables::lp4 as t;

#[derive(Clone, Default)]
pub(crate) struct Band {
    pub points: Vec<(i32, usize)>,
    envelope: [f32; 32],
    peak: f64,
    last: f64,
}
fn level(z: f64) -> i32 {
    ((z * t::GC_SQRT2 as f64) as f32).to_bits().wrapping_shr(23) as i32 - 127
}
impl Band {
    fn detect(&mut self, newest: [f32; 32], mode: i32) -> [f32; 32] {
        let old = self.envelope;
        let mut a = [0.0f64; 64];
        for i in 0..32 {
            a[i] = old[i] as f64;
            a[i + 32] = newest[i] as f64;
        }
        let mut blocks = [0.0f64; 8];
        for j in 0..8 {
            blocks[j] = a[4 * j..4 * j + 4].iter().copied().fold(0.0, f64::max);
        }
        let before = self.last;
        self.last = blocks[7];
        let end = if mode <= 0 {
            64
        } else {
            8 * (8 - mode) as usize
        };
        let floor = t::GC_FLOOR as f64;
        let minimum = t::GC_MIN as f64;
        let ratio = t::GC_RATIO as f64;
        let mut reference = floor.max(blocks[7]);
        for &v in &a[32..end.max(32)] {
            reference = reference.max(v);
        }
        let mut threshold = (ratio * reference) as f32 as f64;
        let mut budget = 4i32;
        let mut decays: Vec<(usize, i32)> = Vec::new();
        for j in (-1i32..=7).rev() {
            let value = if j < 0 { before } else { blocks[j as usize] };
            if value < reference {
                continue;
            }
            if value > minimum && value > threshold {
                let e = 4 * (j + 1) as usize;
                let location = if j == -1 || a[e] >= threshold || a[e - 1] >= threshold {
                    e
                } else if a[e - 2] >= threshold {
                    e - 1
                } else {
                    e - 2
                };
                let step = level(value / reference).min(budget);
                budget -= step;
                decays.insert(0, (location, -step));
                if budget <= 0 || decays.len() == 2 {
                    break;
                }
            }
            reference = value;
            threshold = (value * ratio) as f32 as f64;
        }
        let stop = decays.first().map_or(32, |x| x.0);
        let mut room = 15.min(
            131 - ((self.peak * t::GC_EXPSCALE as f64) as f32)
                .to_bits()
                .wrapping_shr(23) as i32,
        ) - budget;
        let mut attacks: Vec<(usize, i32)> = Vec::new();
        if room > 0 {
            let factor = if mode == -1 { 2.0 } else { 1.6f32 as f64 };
            reference = self.peak.max(a[0]);
            threshold = factor * reference;
            for i in 0..stop {
                let value = a[i + 1];
                if value < reference {
                    continue;
                }
                if value > minimum && value > threshold {
                    let mut step = level(value / reference);
                    if let Some(&(p, s)) = attacks.last() {
                        if p + 1 == i && step >= s {
                            attacks.pop();
                            room += s;
                            step += s;
                        }
                    }
                    step = step.min(room);
                    room -= step;
                    attacks.push((i, step));
                    if attacks.len() == 7 - decays.len() || room <= 0 {
                        break;
                    }
                }
                reference = value;
                threshold = value * factor;
            }
        }
        attacks.extend(decays);
        let mut sum = 4;
        self.points.clear();
        for &(loc, step) in attacks.iter().rev() {
            sum += step;
            self.points.push((sum, loc));
        }
        self.points.reverse();
        self.peak = old.iter().map(|&x| x as f64).fold(floor, f64::max);
        self.envelope = newest;
        old
    }
}

pub(crate) struct GainControl {
    pub bands: [Band; 4],
}
impl Default for GainControl {
    fn default() -> Self {
        Self {
            bands: std::array::from_fn(|_| Band::default()),
        }
    }
}
impl GainControl {
    pub(crate) fn process(
        &mut self,
        newest: &Bands,
        previous: &Bands,
        joint: bool,
        sent: [i32; 4],
        current: [i32; 4],
    ) -> (Bands, usize) {
        let mut peaks = [[0.0f32; 32]; 4];
        for b in 0..4 {
            for j in 0..32 {
                for i in 0..8 {
                    peaks[b][j] = peaks[b][j].max(newest[8 * j + i][b].abs());
                }
            }
        }
        let old_count = self.bands[0].points.len();
        let mut old_band0 = [0.0f32; 32];
        for b in (0..4).rev() {
            let mode = if !joint {
                b as i32
            } else if sent[b] != current[b] {
                5
            } else if sent[b] != 3 {
                0
            } else {
                -1
            };
            let old = self.bands[b].detect(peaks[b], mode);
            if b == 0 {
                old_band0 = old;
            }
        }
        let bound = t::GC_BAND0_MAX as f64;
        if !joint
            && self.bands[0].points.is_empty()
            && old_count == 0
            && !self.bands[1].points.is_empty()
            && self.bands[0].peak <= bound
        {
            let mut lo = 4;
            let mut hi = 4;
            for &(v, _) in &self.bands[1].points {
                lo = lo.min(v);
                hi = hi.max(v);
            }
            let loc = self.bands[1].points[0].1;
            if hi - lo >= 2 && old_band0[..=loc].iter().all(|&x| x as f64 <= bound) {
                self.bands[0].points.push((5, loc));
            }
        }
        let mut modified = *previous;
        for b in 0..4 {
            let points = &self.bands[b].points;
            let mut start = 0;
            for (i, &(level, loc)) in points.iter().enumerate() {
                let next = points.get(i + 1).map_or(4, |p| p.0);
                let end = 8 * loc;
                for row in &mut modified[start..=end] {
                    row[b] *= pow2_eighth(8 * (level - 4));
                }
                for j in 1..8 {
                    modified[end + j][b] *=
                        pow2_eighth(8 * (level - 4) + j as i32 * (next - level));
                }
                start = end + 8;
            }
        }
        (modified, old_count)
    }
}

// Fractional powers are frozen; exact powers of two use exponent adjustment.
pub(crate) fn pow2_eighth(e: i32) -> f32 {
    const FRAC: [u32; 8] = [
        0x3f800000, 0x3f8b95c2, 0x3f9837f0, 0x3fa5fed7, 0x3fb504f3, 0x3fc5672a, 0x3fd744fd,
        0x3feac0c7,
    ];
    let exponent = e.div_euclid(8);
    let bits = FRAC[e.rem_euclid(8) as usize] as i64 + ((exponent as i64) << 23);
    f32::from_bits(bits as u32)
}
