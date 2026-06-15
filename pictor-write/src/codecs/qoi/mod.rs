use std::{
    io::{BufWriter, Write},
    path::Path,
};

use pictor_core::{PictorError, PictorResult};

use crate::codecs::qoi::color_type::{Channels, ColorSpace};

pub mod color_type;

const QOI_MAX_PIXELS: usize = 400_000_000;
const END_MARKER: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];

/// An index into an array of previously seen pixels
///
/// The array is built and mantained by both the encoder and decoder
/// As each pixel is seen, it is added to the array at the position
/// formed by the hash function `idx_color_hash`.
///
/// If the pixel in the array matches the current pixel, the index
/// position is written into the stream as `QOI_OP_INDEX`.
const QOI_OP_INDEX: u8 = 0x00; // 00xxxxxx

/// A difference between the current and previous pixel
/// for the ref, green and blue. Alpha is unchanged from the previous pixel.
///
/// Differences are stored as 2-bit in order:
/// red | green | blue
///
/// Values are stored as unsigned integers with a bias of `-2`.
/// 00 -> -2
/// 01 -> -1
/// 10 ->  0
/// 11 ->  1
///
/// Values also wrap around.
const QOI_OP_DIFF: u8 = 0x40; // 01xxxxxx

/// While `QOI_OP_DIFF` stored the channel differences independently,
/// `QOI_OP_LUMA` stores a difference in the green channel with the previous pixel.
/// It then stores the red and blue differences relative to the green difference.
///
/// This is stored as 2 bytes in total.
/// First byte contains the tag and 6 bits dedicated for the green channel difference.
/// The second byte has no tag and 4 bits dedicated for the red / blue differences each.
///
/// The differences can wrap around and they are stored with a bias of 32 for the green
/// channel, and a bias of 8 for the red / blue channels.
///
///
/// Example:
/// prev = (100, 100, 100)
/// cur  = (105, 106, 104)
///
/// dr = +5
/// dg = +6 <- base
/// db = +4
///
/// dr_dg = dr - dg =  5 - 6 = -1
/// db_dg = db - dg =  4 - 6 = -2
///
/// Result:
/// 10100110 | 01110110
const QOI_OP_LUMA: u8 = 0x80; // 10xxxxxx

/// A run is used when 2 or more (but not more than 62) consecutive pixels are identical.
/// The length is stored with a bias of `-1`.
const QOI_OP_RUN: u8 = 0xC0; // 11xxxxxx

/// Stores the red / green / blue values directly, when do difference
/// or run can be made.
///
/// Alpha channel must be the same as previous pixel.
/// First byte is used by the tag, and the following 3 bytes store the
/// red / green / blue in that order.
///
/// Value 11111110 is also reserved and cannot be used by any other tag.
const QOI_OP_RGB: u8 = 0xFE; // 11111110

/// Stores the red / green / blue / alpha values directly, when do difference
/// or run can be made.
/// First byte is used by the tag, and the following 4 bytes store the
/// red / green / blue / alpha in that order.
///
/// Value 11111111 is also reserved and cannot be used by any other tag.
const QOI_OP_RGBA: u8 = 0xFF; // 11111111

#[inline]
/// Hash this struct, and return an index into a 64 length array
fn idx_color_hash(rgba: QoiRbga) -> usize {
    (rgba.r as usize * 3 + rgba.g as usize * 5 + rgba.b as usize * 7 + rgba.a as usize * 11)
        & (64 - 1)
}

#[inline]
fn op_index(index: usize) -> u8 {
    debug_assert!(index < 64);
    QOI_OP_INDEX | index as u8
}

#[inline]
fn op_run(run: u8) -> u8 {
    // QOI_RUN stores run length minus 1.
    // Valid run length: 1..=62
    debug_assert!((1..=62).contains(&run));

    QOI_OP_RUN | (run - 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QoiRbga {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Default for QoiRbga {
    fn default() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }
}

impl QoiRbga {
    fn zeroed() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    fn set_values(&mut self, channels: Channels, data: &[u8]) {
        self.r = data[0];
        self.g = data[1];
        self.b = data[2];
        if channels.pixel_size() == 4 {
            self.a = data[3];
        } else {
            self.a = 255;
        }
    }

    #[inline]
    fn alpha_eq(&self, other: QoiRbga) -> bool {
        self.a == other.a
    }

    /// Calculates the difference in rgb values
    /// between current and previous pixel.
    #[inline]
    fn diff_from(&self, other: QoiRbga) -> PixelDiff {
        let dr = self.r as i16 - other.r as i16;
        let dg = self.g as i16 - other.g as i16;
        let db = self.b as i16 - other.b as i16;
        PixelDiff {
            dr,
            dg,
            db,
            dr_dg: dr - dg,
            db_dg: db - dg,
        }
    }

    #[inline]
    fn create_rbg_tag(&self) -> [u8; 4] {
        [QOI_OP_RGB, self.r, self.g, self.b]
    }

    #[inline]
    fn create_rbga_tag(&self) -> [u8; 5] {
        [QOI_OP_RGBA, self.r, self.g, self.b, self.a]
    }
}

enum QoiOperation {
    FitsDiff,
    FitsLuma,
    Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelDiff {
    dr: i16,
    dg: i16,
    db: i16,
    dr_dg: i16,
    db_dg: i16,
}

impl PixelDiff {
    fn check_op(&self) -> QoiOperation {
        if self.fits_qoi_diff() {
            QoiOperation::FitsDiff
        } else if self.fits_qoi_luma() {
            QoiOperation::FitsLuma
        } else {
            QoiOperation::Rgb
        }
    }

    #[inline]
    fn fits_qoi_diff(&self) -> bool {
        let range = -2..=1;
        range.contains(&self.dr) && range.contains(&self.dg) && range.contains(&self.db)
    }

    #[inline]
    fn fits_qoi_luma(&self) -> bool {
        (-8..=7).contains(&self.dr_dg)
            && (-32..=31).contains(&self.dg)
            && (-8..=7).contains(&self.db_dg)
    }

    #[inline]
    fn create_diff_tag(&self) -> [u8; 1] {
        [QOI_OP_DIFF
            | ((self.dr + 2) as u8) << 4
            | ((self.dg + 2) as u8) << 2
            | (self.db + 2) as u8]
    }

    #[inline]
    fn create_luma_tag(&self) -> [u8; 2] {
        let bit_1 = QOI_OP_LUMA | ((self.dg + 32) as u8);
        let bit_0: u8 = ((self.dr_dg + 8) as u8) << 4 | (self.db_dg + 8) as u8;
        [bit_1, bit_0]
    }
}

pub struct EncodingRequest<'a> {
    width: u32,
    height: u32,
    channels: Channels,
    color_space: ColorSpace,
    data: &'a [u8],
}

impl<'a> EncodingRequest<'a> {
    fn encode<W: Write>(&self, writer: W) -> PictorResult<()> {
        let mut writer = BufWriter::new(writer);

        // QOI_MAGIC
        writer.write_all(b"qoif")?;

        writer.write_all(&self.width.to_be_bytes())?;
        writer.write_all(&self.height.to_be_bytes())?;
        writer.write_all(&[self.channels.pixel_size(), self.color_space.id()])?;

        let len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|len| len.checked_mul(self.channels.pixel_size() as usize))
            .ok_or(PictorError::MulOverflow {
                op: "Total length of file excedes usize.",
            })?;

        let last_pixel = len - self.channels.pixel_size() as usize;

        let mut cur = QoiRbga::zeroed();
        let mut prev = QoiRbga::default();
        let mut pixel_cache: [Option<QoiRbga>; 64] = [None; 64];
        let mut pixel = 0;
        let mut run = 0;
        while pixel < len {
            cur.set_values(self.channels, &self.data[pixel..]);

            if cur == prev {
                run += 1;
                if run == 62 || pixel == last_pixel {
                    writer.write_all(&[op_run(run)])?;
                    run = 0;
                }
            } else {
                if run > 0 {
                    writer.write_all(&[op_run(run)])?;
                    run = 0;
                }

                let index_pos = idx_color_hash(cur);

                if pixel_cache[index_pos] == Some(cur) {
                    writer.write_all(&[op_index(index_pos)])?;
                } else {
                    pixel_cache[index_pos] = Some(cur); /* Save cur in cache */

                    if cur.alpha_eq(prev) {
                        let diff = cur.diff_from(prev);
                        match diff.check_op() {
                            QoiOperation::FitsDiff => writer.write_all(&diff.create_diff_tag())?,
                            QoiOperation::FitsLuma => writer.write_all(&diff.create_luma_tag())?,
                            QoiOperation::Rgb => writer.write_all(&cur.create_rbg_tag())?,
                        };
                    } else {
                        writer.write_all(&cur.create_rbga_tag())?;
                    }
                }
            }

            prev = cur;
            pixel += self.channels.pixel_size() as usize;
        }

        writer.write_all(&END_MARKER)?;
        writer.flush()?;

        Ok(())
    }
}

pub struct QoiBuilder {
    width: u32,
    height: u32,
    channels: Channels,
    color_space: ColorSpace,
}

impl QoiBuilder {
    pub fn new(width: u32, height: u32, channels: Channels) -> Self {
        Self {
            width,
            height,
            channels,
            color_space: ColorSpace::Srbg,
        }
    }

    pub fn create_request<'a>(&self, data: &'a [u8]) -> PictorResult<EncodingRequest<'a>> {
        if self.height as usize >= QOI_MAX_PIXELS / self.width as usize {
            return Err(PictorError::FileSizeExceeded);
        };

        Ok(EncodingRequest {
            width: self.width,
            height: self.height,
            channels: self.channels,
            color_space: self.color_space,
            data,
        })
    }

    pub fn encode(&self, data: &[u8], path: &Path) -> PictorResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.encode_with(data, &mut file)
    }

    pub fn encode_with<W: Write>(&self, data: &[u8], writer: &mut W) -> PictorResult<()> {
        let request = self.create_request(data)?;
        request.encode(writer)
    }
}
