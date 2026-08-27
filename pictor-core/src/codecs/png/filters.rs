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
    /// pos = position of the current byte
    #[inline]
    pub fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
        let a = a as i32;
        let b = b as i32;
        let c = c as i32;

        let p = a.wrapping_add(b).wrapping_sub(c);

        let pa = (p - a).abs();
        let pb = (p - b).abs();
        let pc = (p - c).abs();

        if pa <= pb && pa <= pc {
            a as u8
        } else if pb <= pc {
            b as u8
        } else {
            c as u8
        }
    }
}
