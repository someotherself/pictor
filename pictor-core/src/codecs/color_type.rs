#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorType {
    Y,
    Ya,
    Rbg,
    Rbga,
}

impl ColorType {
    pub const fn pixel_size(&self) -> u8 {
        match self {
            Self::Y => 1,
            Self::Ya => 2,
            Self::Rbg => 3,
            Self::Rbga => 4,
        }
    }

    pub fn id(&self) -> u8 {
        match self {
            Self::Y => 0,
            Self::Ya => 4,
            Self::Rbg => 2,
            Self::Rbga => 6,
        }
    }
}
