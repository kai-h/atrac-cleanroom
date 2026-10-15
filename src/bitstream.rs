use crate::Error;
#[derive(Clone, Debug)]
pub(crate) struct Tone {
    pub position: usize,
    pub scale: i32,
    pub values: Vec<i32>,
}
#[derive(Clone)]
pub(crate) struct Group {
    pub selector: usize,
    pub count: usize,
    pub mode: usize,
    pub bands: [bool; 4],
    pub cells: [Vec<Tone>; 16],
}
impl Group {
    pub fn new(selector: usize, mode: usize) -> Self {
        Self {
            selector,
            count: 4,
            mode,
            bands: [false; 4],
            cells: std::array::from_fn(|_| Vec::new()),
        }
    }
}
pub(crate) struct Bits {
    pub bytes: Vec<u8>,
    pub count: usize,
}
impl Bits {
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            count: 0,
        }
    }
    pub fn put(&mut self, value: i32, width: usize) -> Result<(), Error> {
        if width > 31 || value < 0 || (value as u32 as u64) >= (1u64 << width) {
            return Err(Error::InvalidFrame);
        }
        for bit in (0..width).rev() {
            if self.count % 8 == 0 {
                self.bytes.push(0);
            }
            let i = self.bytes.len() - 1;
            self.bytes[i] |= (((value as u32 >> bit) & 1) as u8) << (7 - self.count % 8);
            self.count += 1;
        }
        Ok(())
    }
}
pub(crate) fn codes(
    values: &[i32],
    selector: usize,
    mode: usize,
    lp2: bool,
) -> Result<Vec<(i32, usize)>, Error> {
    if !(1..=7).contains(&selector) || mode > 1 {
        return Err(Error::InvalidFrame);
    }
    let table = if lp2 {
        crate::tables::lp2::codes(mode, selector)
    } else {
        crate::tables::lp4::codes(mode, selector)
    };
    let width = [0, 2, 3, 3, 4, 4, 5, 6][selector];
    let mask = (1 << width) - 1;
    if selector == 1 && values.len() % 2 != 0 {
        return Err(Error::InvalidFrame);
    }
    let indices: Vec<usize> = if selector == 1 {
        values
            .chunks_exact(2)
            .map(|v| (((v[0] & 3) << 2) | (v[1] & 3)) as usize)
            .collect()
    } else {
        values.iter().map(|v| (v & mask) as usize).collect()
    };
    indices
        .iter()
        .map(|&i| {
            let row = table.get(i).ok_or(Error::InvalidFrame)?;
            if row[1] <= 0 {
                Err(Error::InvalidFrame)
            } else {
                Ok((row[0], row[1] as usize))
            }
        })
        .collect()
}
pub(crate) fn pack(
    nbands: usize,
    gains: &[Vec<(i32, usize)>; 4],
    groups: &[Group],
    selectors: &[usize],
    scales: &[i32],
    values: &[Vec<i32>],
    lp2: bool,
    header: Option<(usize, [i32; 4])>,
) -> Result<(Vec<u8>, usize), Error> {
    let mut bits = Bits::new();
    if let Some((weight, matrices)) = header {
        bits.put(weight as i32, 4)?;
        for m in matrices {
            if m == 2 {
                return Err(Error::InvalidFrame);
            }
            bits.put(m, 2)?;
        }
        bits.put(3, 2)?;
    } else {
        bits.put(0x28, 6)?;
    }
    if !(1..=4).contains(&nbands)
        || selectors.is_empty()
        || selectors.len() > 32
        || scales.len() != selectors.len()
        || values.len() != selectors.len()
    {
        return Err(Error::InvalidFrame);
    }
    bits.put(nbands as i32 - 1, 2)?;
    for points in &gains[..nbands] {
        if points.len() > 7 || points.windows(2).any(|p| p[0].1 >= p[1].1) {
            return Err(Error::InvalidFrame);
        }
        bits.put(points.len() as i32, 3)?;
        for &(level, loc) in points {
            bits.put(level, 4)?;
            bits.put(loc as i32, 5)?;
        }
    }
    bits.put(groups.len() as i32, 5)?;
    if !groups.is_empty() {
        let mode = if groups.iter().all(|g| g.mode == groups[0].mode) {
            groups[0].mode
        } else {
            3
        };
        bits.put(mode as i32, 2)?;
        let mut count = 0;
        for g in groups {
            if g.bands[nbands..].iter().any(|b| *b) {
                return Err(Error::InvalidFrame);
            }
            for b in 0..nbands {
                bits.put(g.bands[b] as i32, 1)?;
            }
            bits.put(g.count as i32 - 1, 3)?;
            bits.put(g.selector as i32, 3)?;
            if mode == 3 {
                bits.put(g.mode as i32, 1)?;
            }
            for b in 0..nbands {
                if !g.bands[b] {
                    continue;
                }
                for cell in 4 * b..4 * b + 4 {
                    bits.put(g.cells[cell].len() as i32, 3)?;
                    for tone in &g.cells[cell] {
                        count += 1;
                        bits.put(tone.scale, 6)?;
                        bits.put((tone.position % 64) as i32, 6)?;
                        for (code, n) in codes(&tone.values, g.selector, g.mode, lp2)? {
                            bits.put(code, n)?;
                        }
                    }
                }
            }
        }
        if count > 64 {
            return Err(Error::InvalidFrame);
        }
    }
    bits.put(selectors.len() as i32 - 1, 5)?;
    bits.put(0, 1)?;
    for &s in selectors {
        bits.put(s as i32, 3)?;
    }
    for (i, &s) in selectors.iter().enumerate() {
        if s != 0 {
            bits.put(scales[i], 6)?;
        }
    }
    for (i, &s) in selectors.iter().enumerate() {
        if s != 0 {
            for (code, n) in codes(&values[i], s, 0, lp2)? {
                bits.put(code, n)?;
            }
        }
    }
    Ok((bits.bytes, bits.count))
}
