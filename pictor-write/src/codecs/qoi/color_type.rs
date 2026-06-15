#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    Rbg,
    Srbg,
}

impl Channels {
    pub(crate) const fn pixel_size(&self) -> u8 {
        match self {
            Self::Rbg => 3,
            Self::Srbg => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Srbg, // // sRGB color, alpha linear
    Linear,
}

impl ColorSpace {
    pub(crate) const fn id(&self) -> u8 {
        match self {
            Self::Srbg => 0,
            Self::Linear => 1,
        }
    }
}
