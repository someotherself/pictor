use crate::codecs::qoi::TAG_MASK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoiTags {
    Index,
    Diff,
    Luma,
    Run,
    Rgb,
    Rgba,
    Other,
}

impl QoiTags {
    /// An index into an array of previously seen pixels
    ///
    /// The array is built and mantained by both the encoder and decoder
    /// As each pixel is seen, it is added to the array at the position
    /// formed by the hash function `idx_color_hash`.
    ///
    /// If the pixel in the array matches the current pixel, the index
    /// position is written into the stream as `QOI_OP_INDEX`.
    pub const QOI_OP_INDEX: u8 = 0x00; // 00xxxxxx

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
    pub const QOI_OP_DIFF: u8 = 0x40; // 01xxxxxx

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
    pub const QOI_OP_LUMA: u8 = 0x80; // 10xxxxxx

    /// A run is used when 2 or more (but not more than 62) consecutive pixels are identical.
    /// The length is stored with a bias of `-1`.
    pub const QOI_OP_RUN: u8 = 0xC0; // 11xxxxxx

    /// Stores the red / green / blue values directly, when do difference
    /// or run can be made.
    ///
    /// Alpha channel must be the same as previous pixel.
    /// First byte is used by the tag, and the following 3 bytes store the
    /// red / green / blue in that order.
    ///
    /// Value 11111110 is also reserved and cannot be used by any other tag.
    pub const QOI_OP_RGB: u8 = 0xFE; // 11111110

    /// Stores the red / green / blue / alpha values directly, when do difference
    /// or run can be made.
    /// First byte is used by the tag, and the following 4 bytes store the
    /// red / green / blue / alpha in that order.
    ///
    /// Value 11111111 is also reserved and cannot be used by any other tag.
    pub const QOI_OP_RGBA: u8 = 0xFF; // 11111111

    pub fn from_byte(byte: u8) -> Self {
        let masked = byte & TAG_MASK;
        match byte {
            Self::QOI_OP_RGB => Self::Rgb,
            Self::QOI_OP_RGBA => Self::Rgba,
            _ if masked == Self::QOI_OP_INDEX => Self::Index,
            _ if masked == Self::QOI_OP_DIFF => Self::Diff,
            _ if masked == Self::QOI_OP_LUMA => Self::Luma,
            _ if masked == Self::QOI_OP_RUN => Self::Run,
            _ => Self::Other,
        }
    }

    pub fn get_index(byte: u8) -> usize {
        (byte & 0x3F) as usize
    }
}
