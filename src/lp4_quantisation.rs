use crate::{
    Error,
    bitstream::{Tone, codes},
    tables::lp4 as t,
};
pub(crate) fn scale_index(values: &[f32]) -> i32 {
    let magnitude = values.iter().map(|x| x.to_bits() << 1).max().unwrap_or(0);
    let mantissa = magnitude & 0xffffff;
    let mut index = 3 * (magnitude >> 24) as i32 - 364;
    if mantissa > 0x965fe9 {
        index += 1;
    }
    if mantissa < 0x428a30 {
        index -= 1;
    }
    if (0..64).contains(&index) { index } else { 0 }
}
pub(crate) fn scale(sf: i32, selector: usize) -> Result<f32, Error> {
    if !(0..64).contains(&sf) || !(1..=7).contains(&selector) {
        return Err(Error::InvalidFrame);
    }
    let bits =
        (t::QSCALE_BITS[3 * selector - sf as usize % 3] as u32).wrapping_sub((sf as u32 / 3) << 23);
    Ok(f32::from_bits(bits))
}
pub(crate) fn quantise(values: &[f32], sf: i32, selector: usize) -> Result<Vec<i32>, Error> {
    let sc = scale(sf, selector)?;
    Ok(values
        .iter()
        .map(|&x| {
            let product = x * sc;
            let rounded = product + 12582912.0f32;
            rounded.to_bits().wrapping_sub(0x4b400000) as i32
        })
        .collect())
}
pub(crate) fn cost(values: &[f32], sf: i32, selector: usize) -> Result<i32, Error> {
    let q = quantise(values, sf, selector)?;
    Ok(6 + codes(&q, selector, 0, false)?
        .iter()
        .map(|c| c.1 as i32)
        .sum::<i32>())
}
fn low16(v: f64) -> i32 {
    (v as f32).to_bits() as i16 as i32
}
pub(crate) fn extract(
    x: &mut [f32; 1024],
    pos: usize,
    selector: usize,
) -> Result<(Tone, i32), Error> {
    let values: [f64; 4] = std::array::from_fn(|i| x[pos + i] as f64);
    let lo = scale_index(&x[pos..pos + 4]);
    let hi = (lo + 3).min(64) - 1;
    let mut best = 1e10;
    let mut sf = lo;
    for candidate in (lo..=hi).rev() {
        let sc = scale(candidate, selector)? as f64;
        let mut error = 0.0;
        let mut acc = 0i32;
        for &v in &values {
            let y = sc * v;
            let q = low16(y + 12582912.0);
            acc |= q;
            if (y - q as f64).abs() > error {
                error = (y - q as f64).abs() as f32 as f64;
            }
        }
        if sc * best > error {
            best = error / sc;
            sf = candidate;
            while acc % 2 == 0 && sf < 61 {
                acc >>= 1;
                sf += 3;
            }
        }
    }
    let sc = scale(sf, selector)? as f64;
    let inv = (1.0 / sc) as f32 as f64;
    let mut q = Vec::new();
    for i in 0..4 {
        let v = low16(sc * values[i] + 12582912.0);
        x[pos + i] = (values[i] - v as f64 * inv) as f32;
        q.push(v & t::QMASK[selector]);
    }
    let cost = 12
        + codes(&q, selector, 0, false)?
            .iter()
            .map(|c| c.1 as i32)
            .sum::<i32>();
    Ok((
        Tone {
            position: pos,
            scale: sf,
            values: q,
        },
        cost,
    ))
}
pub(crate) fn restore(x: &mut [f32; 1024], tone: &Tone, selector: usize) -> Result<(), Error> {
    let inv = 1.0 / scale(tone.scale, selector)? as f64;
    let width = [0, 2, 3, 3, 4, 4, 5, 6][selector];
    for (j, &v) in tone.values.iter().enumerate() {
        let signed = if v & (1 << (width - 1)) != 0 {
            v - (1 << width)
        } else {
            v
        };
        x[tone.position + j] = (signed as f64 * inv + x[tone.position + j] as f64) as f32;
    }
    Ok(())
}
