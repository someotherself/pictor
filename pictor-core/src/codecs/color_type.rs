use crate::{PictorError, codecs::qoi::color_type::Channels};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitDepth {
    U8 = 8,
    U16 = 16,
}

impl BitDepth {
    pub fn bytes_per_comp(&self) -> u8 {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
        }
    }

    pub fn bits_per_comp(&self) -> u8 {
        match self {
            Self::U8 => 8,
            Self::U16 => 16,
        }
    }
}

impl TryFrom<u8> for BitDepth {
    type Error = PictorError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            8 => Ok(Self::U8),
            16 => Ok(Self::U16),
            _ => Err(PictorError::InvalidFormat {
                msg: "Unknown bit depth".to_string(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorType {
    #[default]
    L,
    La,
    Rgb,
    Rgba,
}

impl TryFrom<u8> for ColorType {
    type Error = PictorError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::L),
            4 => Ok(Self::La),
            2 => Ok(Self::Rgb),
            6 => Ok(Self::Rgba),
            _ => Err(PictorError::InvalidFormat {
                msg: "Invalid ColorType".to_string(),
            }),
        }
    }
}

impl From<Channels> for ColorType {
    fn from(value: Channels) -> Self {
        match value {
            Channels::Rbg => ColorType::Rgb,
            Channels::Rbga => ColorType::Rgba,
        }
    }
}

impl ColorType {
    pub const fn comp_per_pix(&self) -> u8 {
        match self {
            Self::L => 1,
            Self::La => 2,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    pub const fn id(&self) -> u8 {
        match self {
            Self::L => 0,
            Self::La => 4,
            Self::Rgb => 2,
            Self::Rgba => 6,
        }
    }
}
