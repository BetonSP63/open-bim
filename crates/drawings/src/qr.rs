//! QR code encoder for the sheet identification code (ГОСТ Р 21.101-2026,
//! 5.2.9): byte mode, error correction level M, versions 1–9 (up to 182
//! bytes), fixed mask 0. Follows ISO/IEC 18004 module placement.

/// (data codewords per block, number of blocks) groups and EC codewords per
/// block for level M, versions 1–9.
const BLOCKS_M: [(&[(usize, usize)], usize); 9] = [
    (&[(16, 1)], 10),
    (&[(28, 1)], 16),
    (&[(44, 1)], 26),
    (&[(32, 2)], 18),
    (&[(43, 2)], 24),
    (&[(27, 4)], 16),
    (&[(31, 4)], 18),
    (&[(38, 2), (39, 2)], 22),
    (&[(36, 3), (37, 2)], 22),
];

const ALIGNMENT: [&[usize]; 9] = [
    &[],
    &[6, 18],
    &[6, 22],
    &[6, 26],
    &[6, 30],
    &[6, 34],
    &[6, 22, 38],
    &[6, 24, 42],
    &[6, 26, 46],
];

/// Square module matrix, `true` = dark, row-major from the top-left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrCode {
    pub size: usize,
    pub modules: Vec<bool>,
}

impl QrCode {
    pub fn dark(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }
}

fn gf_mul(a: u8, b: u8) -> u8 {
    let (mut a, mut b, mut r) = (a as u16, b, 0u16);
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        a <<= 1;
        if a & 0x100 != 0 {
            a ^= 0x11D;
        }
        b >>= 1;
    }
    r as u8
}

fn rs_divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree];
    result[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            result[j] = gf_mul(result[j], root);
            if j + 1 < degree {
                result[j] ^= result[j + 1];
            }
        }
        root = gf_mul(root, 2);
    }
    result
}

fn rs_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; divisor.len()];
    for &b in data {
        let factor = b ^ result.remove(0);
        result.push(0);
        for (r, d) in result.iter_mut().zip(divisor) {
            *r ^= gf_mul(*d, factor);
        }
    }
    result
}

struct Grid {
    size: usize,
    modules: Vec<bool>,
    function: Vec<bool>,
}

impl Grid {
    fn set(&mut self, x: usize, y: usize, dark: bool) {
        self.modules[y * self.size + x] = dark;
        self.function[y * self.size + x] = true;
    }
    fn finder(&mut self, cx: i32, cy: i32) {
        for dy in -4..=4i32 {
            for dx in -4..=4i32 {
                let (x, y) = (cx + dx, cy + dy);
                if (0..self.size as i32).contains(&x) && (0..self.size as i32).contains(&y) {
                    let dist = dx.abs().max(dy.abs());
                    self.set(x as usize, y as usize, dist != 2 && dist != 4);
                }
            }
        }
    }
    fn alignment(&mut self, cx: usize, cy: usize) {
        for dy in -2..=2i32 {
            for dx in -2..=2i32 {
                let dist = dx.abs().max(dy.abs());
                self.set(
                    (cx as i32 + dx) as usize,
                    (cy as i32 + dy) as usize,
                    dist != 1,
                );
            }
        }
    }
    fn format(&mut self, mask: u32) {
        // Level M has format bits 00.
        let data = mask;
        let mut rem = data;
        for _ in 0..10 {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        let bits = ((data << 10) | rem) ^ 0x5412;
        let bit = |i: u32| (bits >> i) & 1 != 0;
        let s = self.size;
        for i in 0..=5 {
            self.set(8, i, bit(i as u32));
        }
        self.set(8, 7, bit(6));
        self.set(8, 8, bit(7));
        self.set(7, 8, bit(8));
        for i in 9..15 {
            self.set(14 - i, 8, bit(i as u32));
        }
        for i in 0..8 {
            self.set(s - 1 - i, 8, bit(i as u32));
        }
        for i in 8..15 {
            self.set(8, s - 15 + i, bit(i as u32));
        }
        self.set(8, s - 8, true);
    }
    fn version(&mut self, version: usize) {
        if version < 7 {
            return;
        }
        let mut rem = version as u32;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        let bits = ((version as u32) << 12) | rem;
        for i in 0..18 {
            let dark = (bits >> i) & 1 != 0;
            let (a, b) = (self.size - 11 + i % 3, i / 3);
            self.set(a, b, dark);
            self.set(b, a, dark);
        }
    }
}

/// Encode `text` (UTF-8 bytes). Errors when it does not fit version 9-M.
pub fn encode(text: &str) -> Result<QrCode, String> {
    let bytes = text.as_bytes();
    let (version, (groups, ec)) = BLOCKS_M
        .iter()
        .enumerate()
        .map(|(i, b)| (i + 1, *b))
        .find(|(_, (groups, _))| {
            let capacity: usize = groups.iter().map(|(d, n)| d * n).sum();
            // Mode (4 bits) + 8-bit count + data must fit.
            4 + 8 + bytes.len() * 8 <= capacity * 8
        })
        .ok_or_else(|| format!("QR: {} байт не помещаются (до 182)", bytes.len()))?;
    let capacity: usize = groups.iter().map(|(d, n)| d * n).sum();
    // Bit stream: byte mode 0100, count, data, terminator, padding.
    let mut bits: Vec<bool> = Vec::new();
    let push = |value: u32, n: usize, bits: &mut Vec<bool>| {
        for i in (0..n).rev() {
            bits.push((value >> i) & 1 != 0);
        }
    };
    push(0b0100, 4, &mut bits);
    push(bytes.len() as u32, 8, &mut bits);
    for &b in bytes {
        push(b as u32, 8, &mut bits);
    }
    let terminator = (capacity * 8 - bits.len()).min(4);
    push(0, terminator, &mut bits);
    while !bits.len().is_multiple_of(8) {
        bits.push(false);
    }
    let mut data: Vec<u8> = bits
        .chunks(8)
        .map(|c| c.iter().fold(0u8, |a, b| (a << 1) | *b as u8))
        .collect();
    for pad in [0xEC, 0x11].iter().cycle() {
        if data.len() >= capacity {
            break;
        }
        data.push(*pad);
    }
    // Split into blocks, add Reed–Solomon codewords, interleave.
    let divisor = rs_divisor(ec);
    let mut blocks: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut at = 0;
    for (len, count) in groups.iter() {
        for _ in 0..*count {
            let block = data[at..at + len].to_vec();
            at += len;
            let ecc = rs_remainder(&block, &divisor);
            blocks.push((block, ecc));
        }
    }
    let longest = blocks.iter().map(|b| b.0.len()).max().unwrap_or(0);
    let mut codewords = Vec::new();
    for i in 0..longest {
        codewords.extend(blocks.iter().filter_map(|b| b.0.get(i)));
    }
    for i in 0..ec {
        codewords.extend(blocks.iter().map(|b| b.1[i]));
    }
    // Function patterns.
    let size = version * 4 + 17;
    let mut g = Grid {
        size,
        modules: vec![false; size * size],
        function: vec![false; size * size],
    };
    for i in 0..size {
        g.set(6, i, i % 2 == 0);
        g.set(i, 6, i % 2 == 0);
    }
    let s = size as i32;
    g.finder(3, 3);
    g.finder(s - 4, 3);
    g.finder(3, s - 4);
    let centres = ALIGNMENT[version - 1];
    let last = centres.len().saturating_sub(1);
    for (i, &cx) in centres.iter().enumerate() {
        for (j, &cy) in centres.iter().enumerate() {
            if (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0) {
                continue;
            }
            g.alignment(cx, cy);
        }
    }
    g.format(0);
    g.version(version);
    // Data in the two-column zigzag from the bottom-right corner.
    let total_bits = codewords.len() * 8;
    let mut i = 0;
    let mut right = size as i32 - 1;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        for vert in 0..size {
            for j in 0..2 {
                let x = (right - j) as usize;
                let upward = (right + 1) & 2 == 0;
                let y = if upward { size - 1 - vert } else { vert };
                if !g.function[y * size + x] && i < total_bits {
                    g.modules[y * size + x] = (codewords[i >> 3] >> (7 - (i & 7))) & 1 != 0;
                    i += 1;
                }
            }
        }
        right -= 2;
    }
    // Mask 0: invert where (x + y) is even.
    for y in 0..size {
        for x in 0..size {
            if !g.function[y * size + x] && (x + y) % 2 == 0 {
                g.modules[y * size + x] ^= true;
            }
        }
    }
    g.format(0);
    Ok(QrCode {
        size,
        modules: g.modules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reed_solomon_matches_the_standard_example() {
        // ISO/IEC 18004 annex: «01234567» 1-M data codewords and EC.
        let data = [
            0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11,
            0xEC, 0x11,
        ];
        let ecc = rs_remainder(&data, &rs_divisor(10));
        assert_eq!(
            ecc,
            [0xA5, 0x24, 0xD4, 0xC1, 0xED, 0x36, 0xC7, 0x87, 0x2C, 0x55]
        );
    }

    #[test]
    fn sizes_and_finder_patterns() {
        let small = encode("2345-КМ").unwrap();
        assert_eq!(small.size, 21);
        let big = encode(&"Ж".repeat(80)).unwrap();
        assert_eq!(big.size, 9 * 4 + 17, "160 bytes need version 9");
        assert!(encode(&"Ж".repeat(100)).is_err());
        for (x, y) in [(0, 0), (small.size - 7, 0), (0, small.size - 7)] {
            assert!(small.dark(x, y) && small.dark(x + 6, y + 6) && !small.dark(x + 1, y + 1));
        }
    }
}
