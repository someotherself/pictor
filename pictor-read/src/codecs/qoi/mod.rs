use std::{
    fs::OpenOptions,
    io::{BufReader, Read},
    path::Path,
};

use pictor_core::{
    codecs::qoi::{
        color_type::{Channels, ColorSpace},
        idx_color_hash,
        tags::QoiTags,
        QoiRbga, END_MARKER,
    },
    PictorError, PictorResult,
};

#[inline]
fn read_u8<R: Read>(reader: &mut R) -> PictorResult<u8> {
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

pub struct QoiDecoder {
    width: u32,
    height: u32,
    channels: Channels,
    color_space: ColorSpace,
}

impl QoiDecoder {
    pub fn decode_from(path: &Path, channels: Option<Channels>) -> PictorResult<DecodedQoi> {
        let file = OpenOptions::new().read(true).open(path)?;
        Self::decode_with(file, channels)
    }
}

impl QoiDecoder {
    pub fn decode_with<R: Read>(reader: R, channels: Option<Channels>) -> PictorResult<DecodedQoi> {
        let mut reader = BufReader::new(reader);
        let decoder = Self::read_header(&mut reader, channels)?;
        let data = decoder.decode(&mut reader)?;

        Ok(DecodedQoi {
            width: decoder.width,
            height: decoder.height,
            channels: decoder.channels,
            color_space: decoder.color_space,
            data,
        })
    }

    fn decode<R: Read>(&self, reader: &mut R) -> PictorResult<Vec<u8>> {
        let expected_len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|len| len.checked_mul(self.channels.pixel_size() as usize))
            .ok_or(PictorError::MulOverflow {
                op: "Total length of file excedes usize.",
            })?;

        let mut prev = QoiRbga::default();
        let mut cur = QoiRbga::default();
        let mut pixel_cache: [Option<QoiRbga>; 64] = [None; 64];
        let mut run = 0;
        let mut tag: u8;
        let mut output = Vec::with_capacity(expected_len);

        while output.len() < expected_len {
            tag = read_u8(reader)?;
            match QoiTags::from_byte(tag) {
                QoiTags::Index => {
                    let Some(px) = pixel_cache[QoiTags::get_index(tag)] else {
                        return Err(PictorError::InvalidArgument {
                            msg: "Invalid index tag",
                        });
                    };
                    cur = px;
                }
                QoiTags::Diff => cur = QoiRbga::from_diff(prev, tag),
                QoiTags::Luma => {
                    let byte_2 = read_u8(reader)?;
                    cur = QoiRbga::from_luma(prev, [tag, byte_2]);
                }
                QoiTags::Run => {
                    run = (tag & 0x3f) as usize;
                    cur = prev;
                }
                QoiTags::Rgb => {
                    let mut rgb = [0u8; 3];
                    reader.read_exact(&mut rgb)?;
                    cur = QoiRbga::from_rbg_tag(rgb);
                }
                QoiTags::Rgba => {
                    let mut rgba = [0u8; 4];
                    reader.read_exact(&mut rgba)?;
                    cur = QoiRbga::from_rbga_tag(rgba);
                }
                QoiTags::Other => {}
            };

            let cache_pos = idx_color_hash(cur);
            pixel_cache[cache_pos] = Some(cur);

            if run > 0 {
                while run > 0 {
                    cur.write_bytes(self.channels, &mut output);
                    run -= 1;
                }
            } else {
                cur.write_bytes(self.channels, &mut output);
                prev = cur;
            }
        }

        let mut end_marker = [0u8; 8];
        reader.read_exact(&mut end_marker)?;
        if end_marker != END_MARKER {
            return Err(PictorError::InvalidArgument {
                msg: "Invalid file length / end marker.",
            });
        }

        Ok(output)
    }

    fn read_header<R: Read>(reader: &mut R, channels: Option<Channels>) -> PictorResult<Self> {
        let mut tmp = [0u8; 14];
        reader.read_exact(&mut tmp)?;

        if tmp[0..4] != [b'q', b'o', b'i', b'f'] {
            return Err(PictorError::InvalidArgument {
                msg: "Invalid header magic",
            });
        }

        let width = u32::from_be_bytes(tmp[4..8].try_into().unwrap());
        let height = u32::from_be_bytes(tmp[8..12].try_into().unwrap());

        let channels = channels.unwrap_or(Channels::try_from(tmp[12])?);
        let color_space = ColorSpace::try_from(tmp[13])?;

        Ok(Self {
            width,
            height,
            channels,
            color_space,
        })
    }
}

pub struct DecodedQoi {
    pub width: u32,
    pub height: u32,
    pub channels: Channels,
    pub color_space: ColorSpace,
    pub data: Vec<u8>,
}
