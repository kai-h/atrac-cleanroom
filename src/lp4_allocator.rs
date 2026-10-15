use crate::lp4_quantisation::{cost, extract, quantise, restore, scale_index};
use crate::{
    Error,
    bitstream::{Group, Tone, pack},
    tables::lp4 as t,
};
fn edge(i: usize) -> usize {
    t::SB_START[i] as usize
}
fn clamp(v: i32) -> usize {
    v.clamp(1, 7) as usize
}
pub(crate) struct Allocation {
    bands: usize,
    selectors: Vec<usize>,
    scales: Vec<i32>,
    groups: Vec<Group>,
    residual: [f32; 1024],
}
impl Allocation {
    pub fn pack(
        &self,
        gains: &[Vec<(i32, usize)>; 4],
        header: Option<(usize, [i32; 4])>,
    ) -> Result<(Vec<u8>, usize), Error> {
        let mut values = Vec::new();
        for (i, &s) in self.selectors.iter().enumerate() {
            values.push(if s == 0 {
                vec![]
            } else {
                quantise(&self.residual[edge(i)..edge(i + 1)], self.scales[i], s)?
            });
        }
        pack(
            self.bands,
            gains,
            &self.groups,
            &self.selectors,
            &self.scales,
            &values,
            false,
            header,
        )
    }
}
fn all_tonal(
    x: &mut [f32; 1024],
    nsub: usize,
    left: i32,
    gsf: &mut [i32; 256],
    sf: &mut [i32; 32],
    sel: &mut [usize; 32],
    priorities: &mut [i32; 32],
) -> Result<(Vec<Group>, i32), Error> {
    let mut groups = vec![Group::new(5, 1), Group::new(7, 1)];
    let mut cost = 22;
    let mut count = 0;
    let mut done = false;
    while !done {
        let initial = count;
        let mut k = 0;
        while k < edge(nsub) {
            if x[k].abs() < 2.0 {
                k += 1;
                continue;
            }
            let mut pos = k.min(1020);
            let scale = scale_index(&x[pos..pos + 4]);
            let mut gi = usize::from(scale > 30);
            let cell = k >> 6;
            let mut reuse = false;
            if groups[gi].cells[cell].len() == 7 {
                if groups[1 - gi].cells[cell].len() == 7 {
                    let weakest = groups[gi].cells[cell]
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.scale < scale)
                        .min_by_key(|(_, r)| r.scale)
                        .map(|(j, _)| j);
                    let Some(j) = weakest else {
                        k += 1;
                        continue;
                    };
                    let record = groups[gi].cells[cell].remove(j);
                    restore(x, &record, groups[gi].selector)?;
                    pos = record.position;
                    reuse = true;
                } else {
                    gi = 1 - gi;
                }
            }
            let g = &mut groups[gi];
            let band = cell >> 2;
            if !g.bands[band] {
                cost += 12;
                g.bands[band] = true;
            }
            let (record, _) = extract(x, pos, g.selector)?;
            g.cells[cell].push(record);
            if !reuse {
                count += 1;
            }
            cost += 28 + 8 * gi as i32;
            if cost > left - 200 || count >= 64 {
                done = true;
                break;
            }
            k += 4;
        }
        if count == initial {
            break;
        }
    }
    for i in 0..nsub {
        let start = edge(i) / 4;
        let end = edge(i + 1) / 4;
        for g in start..end {
            gsf[g] = scale_index(&x[4 * g..4 * g + 4]);
        }
        sf[i] = *gsf[start..end].iter().max().unwrap();
        sel[i] = usize::from(sf[i] != 0);
        priorities[i] = 341 * sf[i];
    }
    Ok((groups, cost))
}
fn normal_tones(
    x: &mut [f32; 1024],
    nsub: usize,
    nbands: usize,
    left: i32,
    sf: &mut [i32; 32],
    sel: &mut [usize; 32],
    p: &[i32; 32],
) -> Result<(Vec<Group>, i32), Error> {
    let mut group = Group::new(3, 0);
    let mut records: Vec<Tone> = Vec::new();
    let mut cells: [Vec<usize>; 16] = std::array::from_fn(|_| Vec::new());
    let mut header = 8;
    let mut bits = 0;
    let mut zeros = 0;
    for i in if left < 1100 { 8 } else { 0 }..nsub {
        if records.len() > 64 - t::SB_WIDTH[i] as usize / 16
            || 24 * records.len() as i32 + header > left - 600
        {
            break;
        }
        if sel[i] == 0 {
            continue;
        }
        let cls = if p[i] < 0x500 { 0 } else { sel[i] };
        let band = if t::SB_WIDTH[i] == 32 {
            1
        } else {
            edge(i) >> 8
        };
        let threshold = (sf[i] - t::TONE_SF_OFFSET[cls + 8 * band]).max(0) as usize;
        let mut start = edge(i);
        let end = edge(i + 1);
        if let Some(last) = records.last() {
            if last.position + 4 > start {
                sf[i] = -sf[i];
                let overlap = last.position as i32 - start as i32;
                sel[i] = (overlap + 4) as usize;
                sel[i - 1] = (sel[i - 1] as i32 + overlap) as usize;
                start = last.position + 4;
            }
        }
        let mut peaks = Vec::new();
        let mut k = start;
        loop {
            if x[k].abs() >= f32::from_bits(t::SF_BITS[threshold] as u32) {
                peaks.push(k);
                k += 3;
            }
            k += 1;
            if k >= end {
                break;
            }
        }
        if peaks.len() > (t::SB_WIDTH[i] as usize + 8) / 16 {
            peaks.clear();
        }
        if !peaks.is_empty() && sf[i] >= 0 {
            sf[i] = -sf[i];
            sel[i] = 0;
        }
        for peak in peaks {
            let mut position = peak.min(1020);
            let mut covered = 4;
            if records.last().is_some_and(|r| r.position + 4 == position) {
                while covered > 1 && x[position + 3].abs() <= x[position - 1].abs() {
                    position -= 1;
                    covered -= 1;
                }
            }
            if cells[position >> 6].len() == 7 {
                continue;
            }
            let (mut record, b) = extract(x, position, 3)?;
            if record.values[0] == 0 {
                let skip = (1..4).find(|&j| record.values[j] != 0).unwrap_or(4);
                if position + skip >= end {
                    restore(x, &record, 3)?;
                    break;
                }
                if position + skip <= 1020 {
                    record.position += skip;
                    record.values.drain(..skip);
                    record.values.resize(4, 0);
                }
            }
            bits += b;
            sel[i] += covered;
            let cell = record.position >> 6;
            if !group.bands[cell >> 2] {
                group.bands[cell >> 2] = true;
                header += 12;
            }
            cells[cell].push(records.len());
            zeros += i32::from(record.values[3] == 0);
            records.push(record);
        }
    }
    let mut count = records.len() as i32;
    if count == 0 {
        return Ok((vec![], 0));
    }
    if count == 1 && (records[0].position < 128 || left < 1100) {
        restore(x, &records[0], 3)?;
        return Ok((vec![], 0));
    }
    header += nbands as i32;
    let mut vlc = bits;
    let mut clc = 24 * count;
    let mut shrink = zeros == count;
    if !shrink {
        let nz = 2 * count - 3 * zeros;
        let fields = 12 * (count - zeros);
        let ve = nz + fields + bits;
        let ce = 3 * nz + fields + 24 * count;
        if bits.min(24 * count) > ve.min(ce) {
            let mut additions = Vec::new();
            let mut counts = cells.each_ref().map(Vec::len);
            let mut success = true;
            for (idx, r) in records.iter().enumerate() {
                if r.values[3] != 0 {
                    let pos = r.position + 2;
                    let cell = pos >> 6;
                    if pos > 1021
                        || !group.bands[cell >> 2]
                        || counts[cell] >= 7
                        || count as usize + additions.len() == 64
                    {
                        success = false;
                        break;
                    }
                    counts[cell] += 1;
                    additions.push((
                        idx,
                        Tone {
                            position: pos,
                            scale: r.scale,
                            values: vec![r.values[2], r.values[3], 0, 0],
                        },
                    ));
                }
            }
            if success {
                for (idx, r) in additions {
                    records[idx].values[2..].fill(0);
                    cells[r.position >> 6].push(records.len());
                    records.push(r);
                }
                count = records.len() as i32;
                vlc = count + ve;
                clc = 3 * count + ce;
                shrink = true;
            }
        }
    }
    if shrink {
        let mut v = 3;
        loop {
            let mut stop = false;
            for r in records.iter_mut().rev() {
                if r.values[v] != 0 {
                    r.position += 1;
                    for j in 0..v {
                        r.values[j] = r.values[j + 1];
                    }
                }
                if r.values[v - 1] != 0 && (r.values[0] != 0 || r.position % 64 == 63) {
                    stop = true;
                }
            }
            v -= 1;
            if stop {
                break;
            }
            if v < 1 {
                return Err(Error::SpecificationGap("LP4 §10.4 all-zero tone shrink"));
            }
        }
        let removed = (3 - v) as i32 * count;
        vlc -= removed;
        clc -= 3 * removed;
        group.count = v + 1;
        for r in &mut records {
            r.values.truncate(v + 1);
        }
    }
    group.mode = usize::from(vlc > clc);
    for cell in 0..16 {
        group.cells[cell] = cells[cell].iter().map(|&i| records[i].clone()).collect();
    }
    Ok((vec![group], header + vlc.min(clc)))
}
fn estimate(i: usize, sel: &[usize; 32], sf: &[i32; 32], gsf: &[i32; 256]) -> i32 {
    let s = sel[i];
    let quiet = gsf[edge(i) / 4..edge(i + 1) / 4]
        .iter()
        .filter(|&&g| g < sf[i] - t::EST_THR[s])
        .count() as i32;
    t::EST_BITS[s] * t::SB_WIDTH[i] + 60 - t::EST_SAVE[s] * quiet
}
fn bits(x: &[f32; 1024], i: usize, sf: i32, sel: usize) -> Result<i32, Error> {
    cost(&x[edge(i)..edge(i + 1)], sf, sel)
}
pub(crate) fn allocate(
    spectrum: [f32; 1024],
    budget: i32,
    nsub_max: usize,
    gains: &mut [Vec<(i32, usize)>; 4],
    previous: usize,
    joint: bool,
    weight: usize,
) -> Result<Allocation, Error> {
    let mut x = spectrum;
    if budget < 40 {
        gains[0].clear();
        return Ok(Allocation {
            bands: 1,
            selectors: vec![0],
            scales: vec![0],
            groups: vec![],
            residual: x,
        });
    }
    let mut gsf = std::array::from_fn(|g| scale_index(&x[4 * g..4 * g + 4]));
    let mut sf = [0i32; 32];
    let mut sums = [0i32; 32];
    let mut cumulative = [0i32; 32];
    let mut sel = [0usize; 32];
    let mut p = [0i32; 32];
    let mut nsub = nsub_max;
    let mut count = 0;
    let mut total = 0;
    let mut top = 0;
    for i in 0..32 {
        let part = &gsf[edge(i) / 4..edge(i + 1) / 4];
        let mut sum = part.iter().sum::<i32>();
        let mut max = *part.iter().max().unwrap();
        count += part.iter().filter(|&&v| v > 7).count() as i32;
        if top < max {
            top = max;
            if !joint {
                let multiple = if edge(i) & 0x80 != 0 && i < 22 { 16 } else { 8 };
                if count * multiple < budget {
                    nsub = nsub.max(if i <= 26 { 28 } else { (i + 2).min(32) });
                }
            }
        } else if sum < t::MIN_SFSUM[i] || max < 3 {
            sum = 0;
            max = 0;
        }
        sf[i] = max;
        sums[i] = sum;
        cumulative[i] = count;
        total += sum;
    }
    let shift = if 2 * budget < 12 * cumulative[nsub - 1] {
        11
    } else {
        10
    };
    let mut nbands = (edge(nsub) + 255) / 256;
    let mut left = budget
        - 3 * nsub as i32
        - 3 * (3 * gains[..nbands].iter().map(|g| g.len() as i32).sum::<i32>() + nbands as i32);
    let groups;
    if left > 32 * cumulative[nsub - 1] {
        let (g, c) = all_tonal(&mut x, nsub, left, &mut gsf, &mut sf, &mut sel, &mut p)?;
        groups = g;
        left -= c;
    } else {
        for i in 0..nsub {
            let mut m = sf[i];
            if m >= 3 {
                m = 256 * m - total;
                sel[i] = clamp(m >> shift);
            }
            p[i] = m;
        }
        let (g, c) = if joint {
            (vec![], 0)
        } else {
            normal_tones(&mut x, nsub, nbands, left, &mut sf, &mut sel, &p)?
        };
        groups = g;
        left -= c;
        for i in 0..nsub {
            let m = sf[i];
            if m < 0 {
                for g in edge(i) / 4..edge(i + 1) / 4 {
                    gsf[g] = scale_index(&x[4 * g..4 * g + 4]);
                }
                let part = &gsf[edge(i) / 4..edge(i + 1) / 4];
                let maximum = *part.iter().max().unwrap();
                let sum = part.iter().sum::<i32>();
                sf[i] = maximum;
                if sel[i] == 0 {
                    p[i] += 256;
                } else {
                    let k = ((sel[i] - 1) >> 1).min(4);
                    let mut penalty = t::TONE_PENALTY[k] * (maximum + m);
                    if budget < 1100 {
                        penalty *= 2;
                    } else if t::SB_WIDTH[i] <= 16 && p[i] > 0x1800 {
                        penalty = (2 * penalty).min(p[i] - 0x400);
                    }
                    if i > 1 {
                        p[i - 1] -= penalty >> 3;
                    }
                    if i < 31 {
                        p[i + 1] -= penalty >> 2;
                    }
                    p[i] -= penalty;
                }
                sel[i] =
                    if maximum > 7 && sum > t::MIN_SFSUM[i] && (p[i] >= 0x400 || budget > 0x708) {
                        clamp(p[i] >> shift)
                    } else {
                        0
                    };
            } else if shift == 11 {
                if t::SB_WIDTH[i] * m < 5 * sums[i] {
                    if m >= 62 {
                        continue;
                    }
                    sf[i] = m + 1;
                }
                if sf[i] < 62 {
                    sf[i] += 1;
                }
            } else if t::SB_WIDTH[i] * m > 8 * sums[i] {
                p[i] += 256;
            }
        }
    }
    let mut width = 0;
    let mut full_width = 0;
    let mut estimate_total = 0;
    for i in 0..nsub {
        if sel[i] == 0 {
            continue;
        }
        if (i > 1 && p[i] < p[i - 1] - 0xf00) || (i < nsub - 1 && p[i] < p[i + 1] - 0x1400) {
            sel[i] = 0;
        } else {
            width += t::SB_WIDTH[i];
            if sel[i] == 7 {
                full_width += t::SB_WIDTH[i];
            }
            estimate_total += estimate(i, &sel, &sf, &gsf);
        }
    }
    while nsub > 1 && sel[nsub - 1] == 0 {
        nsub -= 1;
        left += 3;
    }
    if !joint || weight & 7 == 7 {
        let old = nbands;
        while nbands > 1
            && gains[nbands - 1].is_empty()
            && !groups.iter().any(|g| g.bands[nbands - 1])
        {
            nbands -= 1;
        }
        left += (groups.len() as i32 + 3) * (old - nbands) as i32;
    }
    let mut l10 = 10 * left;
    let denominator = if l10 <= estimate_total {
        width
    } else {
        width - full_width
    };
    let mut offset = 0;
    if denominator != 0 {
        let diff = (if left < 600 { l10 } else { 9 * left }) - estimate_total;
        offset = (1024i64 * diff as i64 / (10 * denominator) as i64) as i32;
        if diff < 0 && shift == 10 {
            offset += offset >> 3;
            p[0] -= offset;
            p[1] -= (188 * offset) >> 8;
            for k in 2..8 {
                p[k] -= (97 * offset) >> 8;
            }
            for k in 8..18 {
                p[k] -= (74 * offset) >> 8;
            }
            for k in 0..18 {
                if p[k] + offset < 0 && sf[k] < 63 {
                    sf[k] += 1;
                }
            }
            for k in 18..nsub {
                if p[k] + offset < 1024 {
                    sf[k] = (sf[k] - ((p[k] + offset - 1024) >> 10)).min(63);
                }
            }
        }
    }
    for i in (0..nsub).rev() {
        if sel[i] != 0 {
            sel[i] = clamp((p[i] + offset) >> shift);
            l10 -= estimate(i, &sel, &sf, &gsf);
            p[i] = l10;
        }
    }
    let mut used = 0;
    let mut order = [-1i32; 32];
    for i in 0..nsub {
        if sel[i] == 0 {
            continue;
        }
        if t::SB_WIDTH[i] > 2 * (left - used) - 16 {
            sel[i] = 0;
            continue;
        }
        let diff = p[i] - 10 * used;
        if sel[i] < 7 && diff > 4000 {
            sel[i] += 1;
        } else if i != 0 && diff < 300 {
            if sel[i] > 1 {
                sel[i] -= 1;
            } else if sf[i] != 63 {
                sf[i] += 1;
            }
        }
        let b = bits(&x, i, sf[i], sel[i])?;
        p[i] = b;
        let zero = t::ZERO_LEN2[sel[i]] * (t::SB_WIDTH[i] >> 1) + 6;
        if b == zero {
            sel[i] = 0;
        } else {
            used += b;
            order[i] = sf[i];
        }
    }
    while nsub > 1 && sel[nsub - 1] == 0 {
        nsub -= 1;
        left += 3;
    }
    let mut slack = left - used;
    if !joint && previous + gains[0].len() > 0 {
        for v in &mut order[..5] {
            if (0..=64).contains(v) {
                *v = 64;
            }
        }
    }
    let mut spent = 0;
    loop {
        let max = *order.iter().max().unwrap();
        if max < 0 {
            break;
        }
        let i = order.iter().position(|&v| v == max).unwrap();
        order[i] = -1;
        let mut b = p[i];
        if spent + b > left {
            if nsub - 1 > i && order[nsub - 1] >= 0 {
                return Err(Error::SpecificationGap("LP4 §10.9 allocation clearing"));
            }
            let zero = t::ZERO_LEN2[sel[i]] * (t::SB_WIDTH[i] >> 1) + 6;
            while spent + b > left {
                sf[i] += 1;
                if sf[i] >= 64 || b <= zero {
                    break;
                }
                b = bits(&x, i, sf[i], sel[i])?;
            }
        } else if sel[i] < 7 && t::STEP_COST[sel[i]] * (t::SB_WIDTH[i] >> 1) < left - spent - 7 {
            let mut s = sel[i] + 1;
            if slack > 4 * t::SB_WIDTH[i] && s < 7 {
                s += 1;
            }
            let new_cost = bits(&x, i, sf[i], s)?;
            if new_cost - b <= slack.max(0) {
                sel[i] = s;
                slack += b - new_cost;
                b = new_cost;
            }
        }
        spent += b;
        if spent > left {
            spent -= b;
            slack += b;
            sel[i] = 0;
        }
    }
    Ok(Allocation {
        bands: nbands,
        selectors: sel[..nsub].to_vec(),
        scales: sf[..nsub].to_vec(),
        groups,
        residual: x,
    })
}
