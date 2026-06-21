use crate::PictorError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    Rbg,
    Srbg,
}

impl Channels {
    pub const fn pixel_size(&self) -> u8 {
        match self {
            Self::Rbg => 3,
            Self::Srbg => 4,
        }
    }
}

impl TryFrom<u8> for Channels {
    type Error = PictorError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            3 => Ok(Self::Rbg),
            4 => Ok(Self::Srbg),
            _ => Err(PictorError::InvalidArgument {
                msg: "Invalid channels",
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Srbg, // // sRGB color, alpha linear
    Linear,
}

impl ColorSpace {
    pub const fn id(&self) -> u8 {
        match self {
            Self::Srbg => 0,
            Self::Linear => 1,
        }
    }
}

impl TryFrom<u8> for ColorSpace {
    type Error = PictorError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Srbg),
            1 => Ok(Self::Linear),
            _ => Err(PictorError::InvalidArgument {
                msg: "Invalid color space",
            }),
        }
    }
}
