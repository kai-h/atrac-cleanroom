use crate::{Error, tables::lp2 as t};
#[derive(Clone, Default)]
pub(crate) struct Gain {
    pub points: Vec<(i32, usize)>,
    pub peak: f32,
}
fn peak(v: &[f32]) -> f32 {
    v.iter().map(|x| x.abs()).fold(0.0, f32::max)
}
// Thresholds remove dependence on platform libm logarithms. This implements
// nearest integer log2 for positive ratios, with upward half-way decisions.
fn log_level(r: f64) -> i32 {
    let base = f64::from_bits(0x3ff6a09e667f3bcd);
    let mut threshold = base;
    let mut n = 0;
    while r >= threshold && n < 31 {
        n += 1;
        threshold *= 2.0;
    }
    n
}
fn detect(h: &[f32; 768], previous: &Gain) -> Result<Gain, Error> {
    let block: [f32; 64] = std::array::from_fn(|i| peak(&h[256 + 8 * i..256 + 8 * i + 8]));
    let group: [f32; 16] = std::array::from_fn(|i| peak(&block[4 * i..4 * i + 4]));
    let pre = peak(&h[224..256]);
    let prior = previous.points.iter().map(|p| p.0 - 4).fold(0, i32::max);
    let mut ratios = [0.0f32; 40];
    let mut exact = [0.0f64; 40];
    let mut maximum = previous.peak as f64;
    for i in 0..32 {
        maximum = maximum.max(block[i] as f64);
        let value = block[i + 1] as f64;
        if value > 10.0 && value > 1.5 * maximum {
            exact[i] = value / maximum.max(4.0);
            ratios[i] = exact[i] as f32;
        }
    }
    maximum = block[63].max(peak(&group[8..])) as f64;
    for g in (0..8).rev() {
        maximum = maximum.max(group[g] as f64);
        let value = if g == 0 { pre } else { group[g - 1] } as f64;
        if value > 10.0 && value > 1.85f32 as f64 * maximum {
            exact[32 + g] = value / maximum.max(4.0);
            ratios[32 + g] = exact[32 + g] as f32;
        }
    }
    let mut chosen = [false; 40];
    for _ in 0..7 {
        let mut i = 0;
        for j in 1..40 {
            if ratios[j] > ratios[i] {
                i = j;
            }
        }
        if ratios[i] == 0.0 {
            break;
        }
        chosen[i] = true;
        ratios[i] = 0.0;
    }
    let mut attacks: Vec<(usize, i32)> = Vec::new();
    let mut total = prior;
    for i in 0..32 {
        if !chosen[i] {
            continue;
        }
        let mut step = log_level(exact[i]);
        step = step.min(10 - total);
        total += step;
        if step > 0 {
            attacks.push((i, step));
        }
        if attacks.len() == 7 {
            break;
        }
    }
    let mut releases = Vec::new();
    if !attacks.is_empty() || !previous.points.is_empty() {
        let cap = total.min(4);
        let mut released = 0;
        for g in (0..8).rev() {
            if !chosen[32 + g] {
                continue;
            }
            let step = log_level(exact[32 + g]).min(cap - released);
            released += step;
            if step > 0 && attacks.len() < 7 {
                releases.push((if g == 0 { 1 } else { 4 * g }, step));
            }
            if attacks.len() + releases.len() == 7 {
                break;
            }
        }
    }
    let release_sum = releases.iter().map(|p| p.1).sum::<i32>();
    for (events, cap) in [(&mut releases, 4), (&mut attacks, 11 + release_sum.min(4))] {
        let mut running = 0;
        for event in events.iter_mut().rev() {
            running = cap.min(running + event.1);
            event.1 = running;
        }
    }
    let mut profile = [0i32; 33];
    let mut pos = 0;
    for (end, value) in attacks {
        while pos <= end {
            profile[pos] += value;
            pos += 1;
        }
    }
    let mut pos = 32i32;
    for (start, value) in releases {
        while pos >= start as i32 {
            profile[pos as usize] += value;
            pos -= 1;
        }
    }
    let mut points: Vec<_> = (0..32)
        .filter(|&i| profile[i] != profile[i + 1])
        .map(|i| (profile[i] - profile[32] + 4, i))
        .collect();
    while points.last().is_some_and(|p| p.0 == 4) {
        points.pop();
    }
    if points.len() > 7 {
        return Err(Error::InvalidFrame);
    }
    Ok(Gain {
        points,
        peak: peak(&block[..32]),
    })
}
fn gain_window(previous: &Gain, current: &Gain) -> [f32; 512] {
    let mut exponents = [0i32; 64];
    for (points, shift, add) in [(&current.points, 32, false), (&previous.points, 0, true)] {
        let mut start = 0;
        for &(level, loc) in points {
            let end = loc + shift;
            for v in &mut exponents[start..=end] {
                if add {
                    *v += level - 4;
                } else {
                    *v = level - 4;
                }
            }
            start = end + 1;
        }
    }
    let mut out = [0.0; 512];
    let mut last = 0;
    for k in (0..64).rev() {
        let exponent = -exponents[k];
        for m in 0..8 {
            out[8 * k + 7 - m] =
                crate::lp4_gain::pow2_eighth((m as i32 + 1) * exponent + (7 - m as i32) * last);
        }
        last = exponent;
    }
    out
}
fn mdct(samples: &[f32; 512], odd: bool) -> [f32; 256] {
    let w: [f64; 512] = std::array::from_fn(|i| samples[i] as f64 * t::MDCT_WIN[i] as f64);
    let mut z = [0.0f32; 256];
    for n in 0..64 {
        z[n] = (-w[384 + 2 * n] - w[383 - 2 * n]) as f32;
        z[192 + n] = (w[256 + 2 * n] + w[511 - 2 * n]) as f32;
    }
    for m in 0..128 {
        z[64 + m] = (w[2 * m] - w[255 - 2 * m]) as f32;
    }
    let mut re = [0.0f32; 128];
    let mut im = [0.0f32; 128];
    for j in 0..128 {
        let i = t::BITREV7[j] as usize;
        let c = t::PRE_COS[i] as f64;
        let s = t::PRE_SIN[i] as f64;
        re[j] = (c * z[2 * i] as f64 - s * z[2 * i + 1] as f64) as f32;
        im[j] = (s * z[2 * i] as f64 + c * z[2 * i + 1] as f64) as f32;
    }
    let mut span = 2;
    while span <= 128 {
        let half = span / 2;
        for start in (0..128).step_by(span) {
            for b in 0..half {
                let top = start + b;
                let bottom = top + half;
                let idx = b * (128 / span);
                let c = t::FFT_COS[idx] as f64;
                let s = t::FFT_SIN[idx] as f64;
                let tr = c * re[bottom] as f64 - s * im[bottom] as f64;
                let ti = s * re[bottom] as f64 + c * im[bottom] as f64;
                let r = re[top] as f64;
                let i = im[top] as f64;
                re[bottom] = (r - tr) as f32;
                im[bottom] = (i - ti) as f32;
                re[top] = (tr + r) as f32;
                im[top] = (ti + i) as f32;
            }
        }
        span *= 2;
    }
    let mut y = [0.0f32; 256];
    for i in 0..128 {
        let r = re[i] as f64;
        let rr = re[127 - i] as f64;
        let v = im[i] as f64;
        let vr = im[127 - i] as f64;
        let p1 = t::POST1[i] as f64;
        let p2 = t::POST2[i] as f64;
        let p3 = t::POST3[i] as f64;
        let p4 = t::POST4[i] as f64;
        y[i] = (((rr * p2 + v * p3) + p1 * r) + p4 * vr) as f32 / 128.0;
        y[255 - i] = (((p3 * r - p4 * rr) - p1 * v) + p2 * vr) as f32 / 128.0;
    }
    if odd {
        y.reverse();
    }
    y
}
pub(crate) struct Analysis {
    delays: [[f32; 46]; 3],
    history: [[f32; 768]; 4],
    previous: [Gain; 4],
}
impl Default for Analysis {
    fn default() -> Self {
        Self {
            delays: [[0.0; 46]; 3],
            history: [[0.0; 768]; 4],
            previous: std::array::from_fn(|_| Gain::default()),
        }
    }
}
impl Analysis {
    fn split(&mut self, samples: &[f32], stage: usize) -> (Vec<f32>, Vec<f32>) {
        let mut combined = self.delays[stage].to_vec();
        combined.extend(samples);
        self.delays[stage].copy_from_slice(&samples[samples.len() - 46..]);
        let n = samples.len() / 2;
        let mut low = vec![0.0; n];
        let mut high = vec![0.0; n];
        for j in 0..n {
            let mut p = 0.0f64;
            let mut q = 0.0f64;
            for k in (1..48).step_by(2).rev() {
                p += t::QMF_WIN[k] as f64 * combined[k + 2 * j] as f64;
            }
            for k in (0..47).step_by(2).rev() {
                q = t::QMF_WIN[k] as f64 * combined[k + 2 * j] as f64 + q;
            }
            low[j] = (q + p) as f32;
            high[j] = (p - q) as f32;
        }
        (low, high)
    }
    pub(crate) fn process(
        &mut self,
        pcm: &[f32; 1024],
    ) -> Result<([f32; 1024], [f32; 1024], [Gain; 4], [Gain; 4]), Error> {
        let (low, high) = self.split(pcm, 0);
        let (b0, b1) = self.split(&low, 1);
        let (b3, b2) = self.split(&high, 2);
        for (h, b) in self.history.iter_mut().zip([b0, b1, b2, b3]) {
            h.copy_within(256.., 0);
            h[512..].copy_from_slice(&b);
        }
        let previous = self.previous.clone();
        let mut current = std::array::from_fn(|_| Gain::default());
        for b in 0..4 {
            current[b] = detect(&self.history[b], &previous[b])?;
        }
        if !current[1].points.is_empty() {
            let levels: Vec<i32> = current[1].points.iter().map(|p| p.0).collect();
            let minimum = levels.iter().copied().fold(4, i32::min);
            let end = levels
                .iter()
                .position(|&x| x == minimum)
                .map_or(levels.len(), |i| i + 1);
            let spread = levels[..end].iter().copied().fold(0, i32::max) - minimum;
            let loc = current[1].points[0].1;
            if spread >= 2
                && current[0].points.is_empty()
                && previous[0].points.is_empty()
                && previous[0].peak <= 16384.0
                && peak(&self.history[0][256..256 + 8 * (loc + 1)]) <= 16384.0
            {
                current[0].points = vec![(5, loc)];
            }
        }
        let mut a = [0.0; 1024];
        let mut b = [0.0; 1024];
        for band in 0..4 {
            let raw: [f32; 512] = self.history[band][..512].try_into().unwrap();
            let unmodified = mdct(&raw, band % 2 == 1);
            b[band * 256..band * 256 + 256].copy_from_slice(&unmodified);
            let compensated =
                if !previous[band].points.is_empty() || !current[band].points.is_empty() {
                    let w = gain_window(&previous[band], &current[band]);
                    let samples = std::array::from_fn(|i| (raw[i] as f64 / w[i] as f64) as f32);
                    mdct(&samples, band % 2 == 1)
                } else {
                    unmodified
                };
            a[band * 256..band * 256 + 256].copy_from_slice(&compensated);
        }
        self.previous = current.clone();
        Ok((a, b, current, previous))
    }
}
