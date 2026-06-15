use pictor_core::{codecs::color_type::ColorType, PictorResult};

use crate::codecs::png::{filter::FilteredPng, EncodedPng};

#[cfg(feature = "stb-compress")]
mod stb_array {
    /// Base length for each length code
    ///
    /// The array is indexed by `huffman code - 257`.
    /// For example:
    ///
    /// - Deflate length code 257 -> index 0 -> base length 3
    /// - Deflate length code 285 -> index 28 -> base length 258
    pub const LENGTH_BASE: [usize; 30] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258, 259,
    ];

    /// Number of extra bits for the a length code
    pub const LENGTH_EXTRA_BITS: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];

    /// Base distance foor each distance code
    pub const DIST_BASE: [usize; 31] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577, 32768,
    ];

    /// Number of  extra bits for a distance code
    pub const DIST_EXTRA_BITS: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];

    pub const ZHASH_SIZE: usize = 16384; // 16K limit
    pub const DEFLATE_WINDOW: usize = 32768; // 32K deflate window
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionLevel {
    Default,
    Level1,
    Level2,
    Level3,
    Level4,
    Level5,
    Level6,
    Level7,
    Level8,
}

impl CompressionLevel {
    pub(crate) fn id(&self) -> u8 {
        match self {
            Self::Default => 5,
            Self::Level1 => 1,
            Self::Level2 => 2,
            Self::Level3 => 3,
            Self::Level4 => 4,
            Self::Level5 => 5,
            Self::Level6 => 6,
            Self::Level7 => 7,
            Self::Level8 => 8,
        }
    }
}

#[allow(dead_code)]
pub struct DeflatedPng {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) color_type: ColorType,
    pub(crate) data: Vec<u8>,
}

impl DeflatedPng {
    pub(crate) fn new(filtered: &FilteredPng, data: Vec<u8>) -> Self {
        Self {
            width: filtered.width,
            height: filtered.height,
            color_type: filtered.color_type,
            data,
        }
    }

    pub(crate) fn encode_in_memory_internal(&self) -> PictorResult<EncodedPng> {
        let zlib = EncodedPng::encode_in_memory(self)?;
        Ok(EncodedPng(zlib))
    }

    #[cfg(not(feature = "stb-compress"))]
    pub(crate) fn compress(quality: u8, data: &[u8]) -> PictorResult<Vec<u8>> {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        let level = Compression::new(quality as u32);

        let out = Vec::new();
        let mut encoder = ZlibEncoder::new(out, level);
        encoder.write_all(data)?;
        let compressed = encoder.finish()?;

        Ok(compressed)
    }

    #[cfg(feature = "stb-compress")]
    pub(crate) fn compress(quality: u8, data: &[u8]) -> PictorResult<Vec<u8>> {
        let mut writer = ZlibBitWriter::new();
        writer.start_fixed_huffman_block();

        let mut zhash_buff = Vec::new();
        zhash_buff.resize_with(stb_array::ZHASH_SIZE, || 0_u8);
        let mut hash_table: Vec<Vec<usize>> = vec![Vec::new(); stb_array::ZHASH_SIZE];

        let len = data.len() - 3;
        let mut i = 0;
        while i < len {
            let mut best_len = 3; // how many bytes matched
            let mut best_match_pos: Option<usize> = None; // the start of the matching range

            // Keep only the lower 14 bits of the hash
            let h = (zhash(&data[i..]) as usize) & (stb_array::ZHASH_SIZE - 1);
            // Bucket hash_table[hash] now contains candidates for a match with this byte
            for &prev in hash_table[h].iter() {
                if i - prev > stb_array::DEFLATE_WINDOW {
                    // entry does not fall within the 32K window
                    continue;
                }
                let len = zlib_count_match(&data[prev..], &data[i..], data.len() - 1);

                if len >= best_len {
                    best_len = len;
                    best_match_pos = Some(prev);
                }
            }
            // if list is too long, delete half the entries
            if hash_table[h].len() >= 2 * quality as usize {
                hash_table[h].drain(0..quality as usize);
            }

            // push the current match index to the bucket
            hash_table[h].push(i);

            // lazy match. Do this again for the next byte
            // and see if we get a better match
            if best_match_pos.is_some() {
                let h = (zhash(&data[i + 1..]) as usize) & (stb_array::ZHASH_SIZE - 1);

                for &prev in hash_table[h].iter() {
                    if i - prev > stb_array::DEFLATE_WINDOW {
                        // entry does not fall within the 32K window
                        continue;
                    }

                    let len = zlib_count_match(&data[prev..], &data[i + 1..], data.len() - 1);
                    if len > best_len {
                        best_match_pos = None; // remove the match for this entry
                        break;
                    }
                }
            }

            // we still have a best match for this byte
            //
            // A deflate stream is encoded as:
            // - length - Huffman symbol
            // - length - extra bits
            // - distance - Huffman symbol
            // - distance - extra bits
            if let Some(best_match_pos) = best_match_pos {
                let best_loc_distance = i - best_match_pos;
                assert!(best_loc_distance < stb_array::DEFLATE_WINDOW && best_len <= 258);
                debug_assert!(best_len >= 3);
                debug_assert!(best_loc_distance >= 1);
                debug_assert!(best_match_pos < i);
                debug_assert!(i + best_len <= data.len());

                debug_assert_eq!(
    &data[best_match_pos..best_match_pos + best_len],
    &data[i..i + best_len],
    "bad match: prev={best_match_pos}, i={i}, len={best_len}, dist={best_loc_distance}"
);
                // Find the Deflate length code bucket
                let mut j = 0;
                while best_len > stb_array::LENGTH_BASE[j + 1] - 1 {
                    j += 1;
                }

                // length - Huffman symbol
                //
                // Length symbols are 257..=285.
                writer.add_fixed_huff((j + 257) as u32);

                // length - extra bits (some length codes cover more than one lenght)
                if stb_array::LENGTH_EXTRA_BITS[j] != 0 {
                    writer.add_bits(
                        (best_len - stb_array::LENGTH_BASE[j]) as u32,
                        stb_array::LENGTH_EXTRA_BITS[j],
                    );
                }

                // Find the Deflate distance bucket
                // distance - Hufman symbol
                j = 0;
                while best_loc_distance > stb_array::DIST_BASE[j + 1] - 1 {
                    j += 1;
                }

                // Distance codes in fixed-Huffman blocks are always 5 bits.
                // stb bit-reverses this one before add_bits
                writer.add_bits(zlib_bitrev(j as u32, 5), 5);

                // distancee - extra bits (some distance codes more than one distance)
                if stb_array::DIST_EXTRA_BITS[j] != 0 {
                    writer.add_bits(
                        (best_loc_distance - stb_array::DIST_BASE[j]) as u32,
                        stb_array::DIST_EXTRA_BITS[j],
                    );
                }
                i += best_len;
            } else {
                writer.add_fixed_huff_byte(data[i]);
                i += 1;
            }
        }

        // Write the rest of the data
        while i < data.len() {
            writer.add_fixed_huff_byte(data[i]);
            i += 1;
        }

        writer.add_fixed_huff(256); // end of block

        // Add padding
        while writer.bitcount != 0 {
            writer.add_bits(0, 1);
        }

        // Check if compression gives improvements
        // If not, store uncompressed
        if writer.out.len() > data.len() + 2 + (data.len().div_ceil(32767) * 5) {
            writer.reset();
            let mut x = 0;
            while x < data.len() {
                let mut block_len = data.len() - x;
                if block_len > 32767 {
                    block_len = 32767;
                }
                writer.push((data.len() - x == block_len) as u8); // true == final block
                writer.push((block_len & 0xff) as u8);
                writer.push(((block_len >> 8) & 0xff) as u8);
                writer.push((!block_len & 0xff) as u8);
                writer.push(((!block_len >> 8) & 0xff) as u8);
                for y in 0..block_len {
                    writer.push(data[x + y]);
                }
                x += block_len;
            }
        }

        // adler32 checksum
        let mut s1: u32 = 1;
        let mut s2: u32 = 0;

        for chunk in data.chunks(5552) {
            for &byte in chunk {
                s1 += byte as u32;
                s2 += s1;
            }
            s1 %= 65521;
            s2 %= 65521;
        }

        writer.push(((s2 >> 8) & 0xff) as u8);
        writer.push((s2 & 0xff) as u8);
        writer.push(((s1 >> 8) & 0xff) as u8);
        writer.push((s1 & 0xff) as u8);

        Ok(writer.out)
    }
}

#[cfg(feature = "stb-compress")]
/// stbiw__zhash
fn zhash(data: &[u8]) -> u32 {
    assert!(data.len() >= 3);
    let mut hash: u32 = data[0] as u32 | ((data[1] as u32) << 8) | ((data[2] as u32) << 16);
    hash ^= hash << 3;
    hash = hash.wrapping_add(hash >> 5);
    hash ^= hash << 4;
    hash = hash.wrapping_add(hash >> 17);
    hash ^= hash << 25;
    hash = hash.wrapping_add(hash >> 6);
    hash
}

#[cfg(feature = "stb-compress")]
/// stbiw__zlib_countm
fn zlib_count_match(prev: &[u8], cur: &[u8], limit: usize) -> usize {
    const MAX_MATCH: usize = 258; // deflate max limit

    // prev should not extend past cur, or be longer than MAX_MATCH
    let limit = prev.len().min(cur.len()).min(limit).min(MAX_MATCH);

    for i in 0..limit {
        if prev[i] != cur[i] {
            // Stop when the slices are no longer identical
            return i;
        }
    }
    limit
}

#[cfg(feature = "stb-compress")]
/// bit-reversal (stbiw__zlib_bitrev)
/// code = input we want to reverse
/// codebits = how many bits we want to reverse
/// returns only the desired bits reversed
fn zlib_bitrev(mut code: u32, codebits: u8) -> u32 {
    let mut res = 0;

    for _ in 0..codebits {
        res = (res << 1) | (code & 1);
        code >>= 1;
    }
    res
}

#[cfg(feature = "stb-compress")]
struct ZlibBitWriter {
    bitbuf: u32,
    bitcount: u8,
    out: Vec<u8>,
}

#[cfg(feature = "stb-compress")]
impl ZlibBitWriter {
    fn new() -> Self {
        let mut writer = Self {
            bitbuf: 0,
            bitcount: 0,
            out: Vec::new(),
        };

        // 0x78 = CMF: deflate + 32K window
        writer.out.push(0x78); /* stbiw__sbpush(out, 0x78); */
        // 0x5e = FLG: check bits + compress level hint (FCHECK + FDICT + FLEVEL)
        // FCHECK = 30; FDICT = 0 (skip FDICTID); FLEVEL = 1 (fast)
        writer.out.push(0x5e); /* stbiw__sbpush(out, 0x5e); */

        writer
    }

    fn start_fixed_huffman_block(&mut self) {
        // BFINAL = 1 (first and final block)
        self.add_bits(1, 1);

        // BTYPE = 01, fixed Huffman.
        //
        // Because add_bits writes LSB-first, passing '1' with 2 bits emits:
        // bit 0 = 1
        // bit 1 = 0
        // This writes Deflate BTYPE = 01
        self.add_bits(1, 2);
    }

    /// stbiw__zlib_huff
    fn add_fixed_huff(&mut self, n: u32) {
        match n {
            0..=143 => self.add_huff(0x30 + n, 8),
            144..=255 => self.add_huff(0x190 + n - 144, 9),
            256..=279 => self.add_huff(n - 256, 7),
            280..=287 => self.add_huff(0xc0 + n - 280, 8),
            _ => panic!("Invalid fixed huffman symbon: {n}"),
        }
    }

    /// stbiw__zlib_huffb
    fn add_fixed_huff_byte(&mut self, byte: u8) {
        let n = byte as u32;

        match n {
            0..=143 => {
                self.add_huff(0x30 + n, 8);
            }
            144..=255 => {
                self.add_huff(0x190 + n - 144, 9);
            }
            _ => unreachable!(),
        }
    }

    /// stbiw__zlib_add
    fn add_bits(&mut self, code: u32, codebits: u8) {
        let mask = if codebits == 32 {
            u32::MAX
        } else {
            (1_u32 << codebits) - 1
        };
        self.bitbuf |= (code & mask) << self.bitcount;
        self.bitcount += codebits;
        self.flush_full_bytes()
    }

    /// stbiw__zlib_huff1, stbiw__zlib_huff2, stbiw__zlib_huff3, stbiw__zlib_huff4
    fn add_huff(&mut self, code: u32, codebits: u8) {
        let reversed = zlib_bitrev(code, codebits);

        debug_assert!(
            reversed < (1_u32 << codebits),
            "reversed code overflow: code={code:#x}, codebits={codebits}, reversed={reversed:#x}"
        );

        self.add_bits(reversed, codebits);
    }

    /// stbiw__zlib_flush
    fn flush_full_bytes(&mut self) {
        while self.bitcount >= 8 {
            self.out.push((self.bitbuf & 0xff) as u8);
            self.bitbuf >>= 8;
            self.bitcount -= 8;
        }
    }

    fn push(&mut self, byte: u8) {
        self.out.push(byte);
    }

    fn reset(&mut self) {
        self.bitbuf = 0;
        self.bitcount = 0;
        self.out.clear();

        self.out.push(0x78); // stbiw__sbpush(out, 0x78);
        self.out.push(0x5e); // stbiw__sbpush(out, 0x5e);
    }
}
