use crate::{
    Error,
    bitstream::{Group, Tone, codes, pack},
    lp2_analysis::Gain,
    tables::lp2 as t,
};
fn edge(i: usize) -> usize {
    t::SB_START[i] as usize
}
fn peak(x: &[f32]) -> f32 {
    x.iter().map(|v| v.abs()).fold(0.0, f32::max)
}
fn scale(v: f32) -> i32 {
    t::SF.iter().position(|&x| x > v).unwrap_or(63) as i32
}
fn groups(x: &[f32; 1024]) -> [i32; 256] {
    std::array::from_fn(|g| scale(peak(&x[4 * g..4 * g + 4])))
}
fn subbands(g: &[i32; 256], n: usize) -> Vec<i32> {
    (0..n)
        .map(|i| *g[edge(i) / 4..edge(i + 1) / 4].iter().max().unwrap())
        .collect()
}
fn quantise(x: &[f32], s: usize, dead: usize) -> Result<(Vec<i32>, i32), Error> {
    if s == 0 {
        return Ok((vec![0; x.len()], 0));
    }
    let max = t::MAXQ[s];
    let zone = if dead == 0 {
        0.0
    } else {
        (t::T1[dead.min(12)] as f64 / t::T2[s] as f64) as f32
    };
    let q: Vec<_> = x
        .iter()
        .map(|&v| {
            if v.abs() < zone {
                0
            } else {
                ((v as f64 * (max as f64 + 0.5) + 31.5).trunc() as i32 - 31).clamp(-max, max)
            }
        })
        .collect();
    let cost = 6 + codes(&q, s, 0, true)?
        .iter()
        .map(|c| c.1 as i32)
        .sum::<i32>();
    Ok((q, cost))
}
fn extract(x: &mut [f32; 1024], position: usize) -> Tone {
    let sf = scale(peak(&x[position..position + 4]));
    let step = (t::SF[sf as usize] as f64 / 31.5) as f32 as f64;
    let mut values = Vec::new();
    for j in 0..4 {
        let v = x[position + j] as f64;
        let q =
            ((31.5 * (v / t::SF[sf as usize] as f64) + 31.5).trunc() as i32 - 31).clamp(-31, 31);
        x[position + j] = (v - q as f64 * step) as f32;
        values.push(q);
    }
    Tone {
        position,
        scale: sf,
        values,
    }
}
fn order(keys: &[i32]) -> Vec<usize> {
    let mut v = keys.to_vec();
    let mut indices: Vec<_> = (0..keys.len()).collect();
    let mut gap = 1;
    while gap <= v.len() {
        gap = 3 * gap + 1;
    }
    loop {
        gap /= 3;
        if gap == 0 {
            return indices;
        }
        if gap >= v.len() {
            continue;
        }
        for t in 0..v.len() - gap {
            let value = v[gap + t];
            let mut j = t as i32;
            while j >= 0 && v[j as usize] < value {
                v[j as usize + gap] = v[j as usize];
                indices.swap(j as usize + gap, j as usize);
                j -= gap as i32;
            }
            v[(j + gap as i32) as usize] = value;
        }
    }
}
fn words(offsets: &[f32], scales: &[i32], slope: i32) -> (Vec<usize>, i32) {
    let peak = *scales[1..].iter().max().unwrap();
    let quiet = if peak >= 30 { 6 } else { (peak / 6).max(1) };
    let sel = offsets
        .iter()
        .enumerate()
        .map(|(i, &o)| {
            if scales[i] < quiet || (i.saturating_sub(8)..i).any(|j| scales[i] < scales[j] - slope)
            {
                0
            } else {
                ((o as f64 + 0.5).trunc() as i32).clamp(1, 7) as usize
            }
        })
        .collect();
    (sel, quiet)
}
fn argmax(g: &[i32]) -> usize {
    let max = g.iter().max().unwrap();
    g.iter().position(|v| v == max).unwrap()
}
fn tonal_class(g: &[i32; 256]) -> usize {
    let p = argmax(g);
    let band = (p / 64) as i32;
    let center = (p % 64) as i32;
    let neighbor = band + if center < 32 { -1 } else { 1 };
    let mut other = 0;
    for (i, &v) in g.iter().enumerate() {
        let b = (i / 64) as i32;
        let r = (i % 64) as i32;
        if (b == band && (r - center).abs() <= 2)
            || (b == neighbor && (r - (63 - center)).abs() <= 2)
        {
            continue;
        }
        other = other.max(v);
    }
    if other < (g[p] - 17).max(9) {
        if g[p] >= 36 {
            2
        } else if g[p] >= 21 {
            1
        } else {
            0
        }
    } else {
        0
    }
}
struct Fit<'a> {
    x: &'a [f32; 1024],
    sel: Vec<usize>,
    dead: Vec<usize>,
    values: Vec<Vec<i32>>,
    costs: Vec<i32>,
    used: i32,
}
impl Fit<'_> {
    fn trial(&self, i: usize, s: usize, d: usize) -> Result<(Vec<i32>, i32), Error> {
        quantise(&self.x[edge(i)..edge(i + 1)], s, d)
    }
    fn set(&mut self, i: usize, s: usize, d: usize, q: Vec<i32>, cost: i32) {
        self.sel[i] = s;
        self.dead[i] = d;
        self.values[i] = q;
        self.costs[i] = cost;
    }
    fn improve(&mut self, i: usize, s: usize, d: usize) -> Result<bool, Error> {
        let (q, c) = self.trial(i, s, d)?;
        if self.used + c - self.costs[i] > 1536 {
            return Ok(false);
        }
        self.used += c - self.costs[i];
        self.set(i, s, d, q, c);
        Ok(true)
    }
}
pub(crate) fn allocate(
    a: [f32; 1024],
    b: [f32; 1024],
    gains: &[Gain; 4],
    previous: &[Gain; 4],
) -> Result<Vec<u8>, Error> {
    let mut a = a;
    let mut ga = groups(&a);
    let mut gb = groups(&b);
    let class = tonal_class(&ga);
    let p = argmax(&gb);
    let (mut nsub, mut nbands) = (29, 3);
    if class > 0 {
        if p >= 160 {
            nsub = 32;
            nbands = 4;
        } else if p >= 96 {
            nsub = 30;
        }
    }
    let gain_bits = 3
        * (nbands
            + 3 * gains[..nbands]
                .iter()
                .map(|g| g.points.len())
                .sum::<usize>()) as i32;
    let mut budget = 1536 - 3 * nsub as i32 - 8 - gain_bits - 13;
    let mut groups: Vec<Group> = Vec::new();
    let mut single_bits = 0;
    if class > 0 {
        let band = (p / 64) as i32;
        let center = (p % 64) as i32;
        let neighbor = band + if center < 32 { -1 } else { 1 };
        single_bits = ((nbands + 6) * class) as i32;
        let mut go_on = true;
        for _ in 0..class {
            let mut group = Group::new(7, 1);
            if go_on {
                for (b, r) in [(band, center), (neighbor, 63 - center)] {
                    if b < 0 || b >= nbands as i32 {
                        continue;
                    }
                    group.bands[b as usize] = true;
                    single_bits += 12;
                    if single_bits >= budget {
                        group.bands[b as usize] = false;
                        single_bits -= 12;
                        continue;
                    }
                    let c = 64 * b + r;
                    for g in (c - 2).max(0)..=(c + 2).min(255) {
                        let g = g as usize;
                        let cell = g / 16;
                        if group.cells[cell].len() == 7 {
                            continue;
                        }
                        if single_bits + 36 > budget {
                            go_on = false;
                            continue;
                        }
                        group.cells[cell].push(extract(&mut a, 4 * g));
                        ga[g] = scale(peak(&a[4 * g..4 * g + 4]));
                        single_bits += 36;
                    }
                }
            }
            groups.push(group);
        }
        budget -= single_bits;
    }
    let slope = if single_bits > 0 { 60 } else { 30 };
    let group_limit = edge(nsub) / 4;
    let mean = (gb[..group_limit].iter().sum::<i32>() as f64 / group_limit as f64) as f32 as f64;
    let attacking = |g: &Gain| {
        let lo = g.points.iter().map(|p| p.0).fold(4, i32::min);
        let hi = g.points.iter().map(|p| p.0).fold(4, i32::max);
        hi - lo > 2
    };
    let attack = if gains[..nbands].iter().any(attacking) {
        0
    } else if previous[..nbands].iter().any(attacking) {
        1
    } else {
        -1
    };
    let mut tone_bits = 0;
    let mut tone_count = 0;
    let threshold = (mean + 32.0) as f32 as f64;
    for _ in 0..if single_bits > 0 { 0 } else { 2 } {
        let mut candidates = Vec::new();
        let mut counts = [0; 16];
        for g in 0..group_limit {
            let cell = g / 16;
            if gb[g] as f64 > threshold && counts[cell] < 7 && candidates.len() + tone_count < 64 {
                candidates.push(g);
                counts[cell] += 1;
            }
        }
        if candidates.is_empty() {
            continue;
        }
        if tone_bits + nbands as i32 + 6 > budget {
            break;
        }
        tone_bits += nbands as i32 + 6;
        let mut group = Group::new(7, 1);
        let mut stop = false;
        for g in candidates {
            let band = g / 64;
            let cell = g / 16;
            if !group.bands[band] {
                if tone_bits + 12 <= budget {
                    group.bands[band] = true;
                    tone_bits += 12;
                } else {
                    stop = true;
                }
            }
            if group.bands[band] {
                if tone_bits + 36 > budget {
                    stop = true;
                } else {
                    group.cells[cell].push(extract(&mut a, 4 * g));
                    tone_count += 1;
                    tone_bits += 36;
                    let residual = scale(peak(&a[4 * g..4 * g + 4]));
                    gb[g] += residual - ga[g];
                    ga[g] = residual;
                }
            }
        }
        groups.push(group);
        if stop {
            break;
        }
    }
    budget -= tone_bits;
    if groups.is_empty() {
        budget += 2;
    }
    let mut sa = subbands(&ga, nsub);
    let sb = subbands(&gb, nsub);
    let mut offsets: Vec<f32> = (0..nsub)
        .map(|i| ((sb[i] as f64 - mean) * t::OFFSET_FACTORS[i]) as f32)
        .collect();
    if attack != -1 {
        for v in &mut offsets[..8] {
            *v = (*v as f64 + 0.7) as f32;
        }
        for v in &mut offsets[8..18] {
            *v = (*v as f64 + 0.5) as f32;
        }
        offsets[0] = offsets[0].max(6.0);
    } else {
        offsets[0] = offsets[0].max(6.0);
        for v in &mut offsets[1..4] {
            *v = v.max(3.0);
        }
    }
    let (mut selectors, quiet) = words(&offsets, &sb, slope);
    let mut fixed = vec![false; nsub];
    for i in 0..nsub {
        if sa[i] < quiet {
            selectors[i] = 0;
            fixed[i] = true;
        }
        if tone_bits == 0 && (selectors[i] == 0 || (sa[i] as f64) < mean) {
            fixed[i] = true;
        }
    }
    let original = selectors.clone();
    for i in 0..nsub {
        let inverse = (1.0 / t::SF[sa[i] as usize] as f64) as f32 as f64;
        for v in &mut a[edge(i)..edge(i + 1)] {
            *v = (*v as f64 * inverse) as f32;
        }
    }
    let density = budget as f64 / (edge(nsub) + 256) as f64;
    let limit = if density > 1.15 {
        0
    } else {
        let r = density as f32 as f64;
        [(1.1, 3), (1.05, 5), (1.0, 7), (0.95, 9), (0.9, 11)]
            .into_iter()
            .find(|&(v, _)| r > v)
            .map_or(12, |p| p.1)
    };
    let mut f = Fit {
        x: &a,
        sel: selectors,
        dead: vec![0; nsub],
        values: vec![vec![]; nsub],
        costs: vec![0; nsub],
        used: 0,
    };
    for i in 0..nsub {
        let (q, c) = f.trial(i, f.sel[i], 0)?;
        f.values[i] = q;
        f.costs[i] = c;
    }
    let mut total = f.costs.iter().sum::<i32>();
    let mut offset = 2.0f64;
    let mut step = 4.0f64;
    let mut adjusted = offsets.clone();
    for iteration in 0..15 {
        for i in 0..nsub {
            adjusted[i] = (offsets[i] as f64
                + offset
                    * if offset > 0.0 {
                        1.0
                    } else {
                        t::NEGATIVE_OFFSET_WEIGHTS[i]
                    }) as f32;
        }
        let (mut fresh, _) = words(&adjusted, &sb, slope);
        for i in 0..nsub {
            if original[i] == 0 {
                fresh[i] = 0;
            }
            if fresh[i] != f.sel[i] {
                let (q, c) = f.trial(i, fresh[i], f.dead[i])?;
                total += c - f.costs[i];
                f.set(i, fresh[i], f.dead[i], q, c);
            }
        }
        if total > budget {
            offset = (offset - step) as f32 as f64;
        } else if iteration >= 6 {
            break;
        } else {
            offset = (if total < budget {
                offset + step
            } else {
                offset - step
            }) as f32 as f64;
        }
        step = (step * if iteration < 7 { 0.5 } else { 1.5 }) as f32 as f64;
    }
    for _ in 0..4 {
        for i in (0..nsub).rev() {
            if total <= budget {
                break;
            }
            if f.sel[i] != 0 && f.dead[i] < limit {
                let d = f.dead[i] + 1;
                let (q, c) = f.trial(i, f.sel[i], d)?;
                total += c - f.costs[i];
                f.set(i, f.sel[i], d, q, c);
            }
        }
    }
    if total > budget {
        let keys: Vec<_> = (0..nsub)
            .map(|i| {
                if single_bits > 0 {
                    (sb[i] + 1) * 32 - i as i32
                } else {
                    sb[i] - (i / 2) as i32
                }
            })
            .collect();
        for i in order(&keys).into_iter().rev() {
            while f.sel[i] != 0 && total > budget {
                let s = f.sel[i] - 1;
                let (q, c) = f.trial(i, s, f.dead[i])?;
                total += c - f.costs[i];
                f.set(i, s, f.dead[i], q, c);
            }
            if total <= budget {
                break;
            }
        }
    }
    let nused = f.sel.iter().rposition(|&s| s != 0).map_or(1, |i| i + 1);
    f.used = 1536 - (budget + 3 * (nsub - nused) as i32 - total);
    let margins: Vec<f32> = (0..nused)
        .map(|i| {
            if f.sel[i] != 0 {
                (adjusted[i] as f64 - f.sel[i] as f64) as f32
            } else {
                -9.99f32
            }
        })
        .collect();
    let low = margins.iter().copied().fold(f32::INFINITY, f32::min) as f64;
    let high = margins.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let mut ordered = Vec::new();
    for k in 1..=10 {
        let boundary = (high - k as f64 * (high - low) * 0.1) as f32;
        for i in (0..nused).rev() {
            if !ordered.contains(&i) && margins[i] >= boundary {
                ordered.push(i);
            }
        }
    }
    for i in ordered {
        if fixed[i] || f.sel[i] == 0 || f.sel[i] >= 7 {
            continue;
        }
        let accepted = if f.sel[i] < original[i] {
            f.improve(i, f.sel[i] + 1, f.dead[i])?
        } else if f.dead[i] > 0 {
            f.improve(i, f.sel[i], f.dead[i] - 1)?
        } else {
            if f.used > 1528 {
                break;
            }
            continue;
        };
        if accepted && f.used > 1528 {
            break;
        }
    }
    for i in 0..nused {
        if f.sel[i] != 0 && f.dead[i] != 0 {
            f.improve(i, f.sel[i], 0)?;
        }
    }
    for _ in 0..7 {
        for i in 0..nused {
            if f.used > 1534 {
                break;
            }
            if !fixed[i]
                && f.sel[i] > 0
                && f.sel[i] < original[i]
                && !f.improve(i, f.sel[i] + 1, f.dead[i])?
            {
                fixed[i] = true;
            }
        }
    }
    if nused > 1 {
        let keys: Vec<_> = (0..nused).map(|i| (sb[i] + 1) * 32 - i as i32).collect();
        let ordered = order(&keys);
        for p in 0..4 {
            for &i in &ordered {
                if f.used > 1534 {
                    break;
                }
                if f.sel[i] > 0 && f.sel[i] < p + 3 {
                    f.improve(i, f.sel[i] + 1, f.dead[i])?;
                }
            }
        }
    }
    for _ in 0..7 {
        for i in 0..nused {
            if f.used > 1534 {
                break;
            }
            if f.sel[i] > 0 && f.sel[i] < 7 {
                f.improve(i, f.sel[i] + 1, f.dead[i])?;
            }
        }
    }
    for i in 0..nused {
        if f.sel[i] == 0 {
            continue;
        }
        let sc = (1.0 / (t::MAXQ[f.sel[i]] as f64 + 0.5)) as f32 as f64;
        let mut eo = 0.0;
        let mut eq = 0.0;
        for (&x, &q) in a[edge(i)..edge(i + 1)].iter().zip(&f.values[i]) {
            eo += x as f64 * x as f64;
            let decoded = q as f64 * sc;
            eq += decoded * decoded;
        }
        if 1.25 * eo < eq && sa[i] > 0 {
            sa[i] -= 1;
        }
    }
    let g = std::array::from_fn(|b| gains[b].points.clone());
    let (out, bits) = pack(
        nbands,
        &g,
        &groups,
        &f.sel[..nused],
        &sa[..nused],
        &f.values[..nused],
        true,
        None,
    )?;
    if bits != f.used as usize {
        return Err(Error::InvalidFrame);
    }
    Ok(out)
}
