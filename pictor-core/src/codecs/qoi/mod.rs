use crate::codecs::qoi::{color_type::Channels, tags::QoiTags};

pub mod color_type;
pub mod tags;

pub const QOI_MAX_PIXELS: usize = 400_000_000;
pub const END_MARKER: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];
pub(crate) const TAG_MASK: u8 = 0xC0; // 11000000

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QoiRbga {
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
    pub fn zeroed() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    pub fn set_values(&mut self, channels: Channels, data: &[u8]) {
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
    pub fn alpha_eq(&self, other: QoiRbga) -> bool {
        self.a == other.a
    }

    #[inline]
    pub fn write_bytes(&self, channels: Channels, out: &mut Vec<u8>) {
        let bytes = self.to_bytes();
        if channels.pixel_size() == 3 {
            out.extend_from_slice(&bytes[..3]);
        } else {
            out.extend_from_slice(&bytes);
        }
    }

    #[inline]
    pub fn to_bytes(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Calculates the difference in rgb values
    /// between current and previous pixel.
    #[inline]
    pub fn diff_from(&self, other: QoiRbga) -> PixelDiff {
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
    pub fn create_rbg_tag(&self) -> [u8; 4] {
        [QoiTags::QOI_OP_RGB, self.r, self.g, self.b]
    }

    #[inline]
    pub fn create_rbga_tag(&self) -> [u8; 5] {
        [QoiTags::QOI_OP_RGBA, self.r, self.g, self.b, self.a]
    }

    #[inline]
    pub fn from_rbg_tag(bits: [u8; 3]) -> Self {
        Self {
            r: bits[0],
            g: bits[1],
            b: bits[2],
            a: 255,
        }
    }

    #[inline]
    pub fn from_rbga_tag(bits: [u8; 4]) -> Self {
        Self {
            r: bits[0],
            g: bits[1],
            b: bits[2],
            a: bits[3],
        }
    }

    #[inline]
    pub fn from_diff(prev: QoiRbga, tag: u8) -> Self {
        let r_dif = ((tag & 0x30) >> 4) as i8 - 2;
        let g_dif = ((tag & 0x0F) >> 2) as i8 - 2;
        let b_dif = (tag & 0x03) as i8 - 2;
        let r = prev.r.wrapping_add(r_dif as u8);
        let g = prev.g.wrapping_add(g_dif as u8);
        let b = prev.b.wrapping_add(b_dif as u8);
        Self { r, g, b, a: prev.a }
    }

    #[inline]
    pub fn from_luma(prev: QoiRbga, bits: [u8; 2]) -> Self {
        let dg = ((bits[0] & 0x3F) as i8) - 32;
        let dr_dg = (((bits[1] >> 4) & 0x0F) as i8) - 8;
        let db_dg = ((bits[1] & 0x0F) as i8) - 8;
        Self {
            r: prev.r.wrapping_add_signed(dg + dr_dg),
            g: prev.g.wrapping_add_signed(dg),
            b: prev.b.wrapping_add_signed(dg + db_dg),
            a: prev.a,
        }
    }
}

#[inline]
/// Hash this struct, and return an index into a 64 length array
pub fn idx_color_hash(rgba: QoiRbga) -> usize {
    (rgba.r as usize * 3 + rgba.g as usize * 5 + rgba.b as usize * 7 + rgba.a as usize * 11)
        & (64 - 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelDiff {
    dr: i16,
    dg: i16,
    db: i16,
    dr_dg: i16,
    db_dg: i16,
}

pub enum QoiOperation {
    FitsDiff,
    FitsLuma,
    Rgb,
}

impl PixelDiff {
    pub fn check_op(&self) -> QoiOperation {
        if self.fits_qoi_diff() {
            QoiOperation::FitsDiff
        } else if self.fits_qoi_luma() {
            QoiOperation::FitsLuma
        } else {
            QoiOperation::Rgb
        }
    }

    #[inline]
    pub fn fits_qoi_diff(&self) -> bool {
        let range = -2..=1;
        range.contains(&self.dr) && range.contains(&self.dg) && range.contains(&self.db)
    }

    #[inline]
    pub fn fits_qoi_luma(&self) -> bool {
        (-8..=7).contains(&self.dr_dg)
            && (-32..=31).contains(&self.dg)
            && (-8..=7).contains(&self.db_dg)
    }

    #[inline]
    pub fn create_diff_tag(&self) -> [u8; 1] {
        [QoiTags::QOI_OP_DIFF
            | ((self.dr + 2) as u8) << 4
            | ((self.dg + 2) as u8) << 2
            | (self.db + 2) as u8]
    }

    #[inline]
    pub fn create_luma_tag(&self) -> [u8; 2] {
        let bit_1 = QoiTags::QOI_OP_LUMA | ((self.dg + 32) as u8); // wrapping_add?
        let bit_0: u8 = ((self.dr_dg + 8) as u8) << 4 | (self.db_dg + 8) as u8; // wrapping_add?
        [bit_1, bit_0]
    }
}
