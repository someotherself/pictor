#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PngFilter {
    None,
    Sub,
    Up,
    Average,
    Paeth,
    AverageFirstRow,
    PaethFirstRow,
}

impl PngFilter {
    pub const MAPPING: [PngFilter; 5] = [
        PngFilter::None,
        PngFilter::Sub,
        PngFilter::Up,
        PngFilter::Average,
        PngFilter::Paeth,
    ];

    pub const MAPPING_FIRST_ROW: [PngFilter; 5] = [
        PngFilter::None,
        PngFilter::Sub,
        PngFilter::None,
        PngFilter::AverageFirstRow,
        PngFilter::PaethFirstRow,
    ];

    pub fn id(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Sub => 1,
            Self::Up => 2,
            Self::Average => 3,
            Self::Paeth => 4,
            Self::AverageFirstRow => 3,
            Self::PaethFirstRow => 4,
        }
    }

    /// a = value of pos - 1
    /// b = value of pos - stride
    /// c = value of pos - 1 - stride
    /// pos = position of byte being encoded
    #[inline]
    pub fn paeth_encoding(a: u8, b: u8, c: u8) -> u8 {
        let p = a.wrapping_add(b).wrapping_sub(c);
        let pa = p.abs_diff(a);
        let pb = p.abs_diff(b);
        let pc = p.abs_diff(c);
        if pa <= pb && pa <= pc {
            a
        } else if pb <= pc {
            b
        } else {
            c
        }
    }

    /// pixels: the entire payload
    /// pos: the pixel we are currenty encoding
    #[inline]
    pub fn encode_first_byte(pixels: &[u8], pos: usize, filter: PngFilter, stride: usize) -> u8 {
        let byte = pixels[pos];
        match filter {
            Self::None => byte,
            Self::Sub => byte,
            Self::Up => pixels[pos].wrapping_sub(pixels[pos - stride]),
            Self::Average => pixels[pos].wrapping_sub(pixels[pos - stride] >> 1),
            Self::Paeth => Self::paeth_encoding(0, pixels[pos - stride], 0),
            Self::AverageFirstRow => byte,
            Self::PaethFirstRow => byte,
        }
    }
}
