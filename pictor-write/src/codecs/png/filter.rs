use pictor_core::{
    codecs::{color_type::ColorType, png::filters::PngFilter},
    PictorResult,
};

use crate::codecs::png::{
    deflate::{CompressionLevel, DeflatedPng},
    EncodeRequest,
};

pub struct FilteredPng {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) color_type: ColorType,
    // pub(crate) row_len: usize, // width * pixel_size
    // pub(crate) stride: usize,  // row_len + 1
    pub(crate) compression: CompressionLevel,
    pub(crate) data: Vec<u8>,
}

impl FilteredPng {
    pub(crate) fn new(png: &EncodeRequest<'_>, payload: Vec<u8>) -> PictorResult<Self> {
        // let row_len = usize::try_from(png.width * png.color_type.pixel_size() as u32)?;
        // let stride = row_len + 1;

        Ok(Self {
            width: png.width,
            height: png.height,
            color_type: png.color_type,
            // row_len,
            // stride,
            compression: png.compression_level,
            data: payload,
        })
    }

    #[inline]
    pub(crate) fn compress(&self) -> PictorResult<DeflatedPng> {
        let zlib = DeflatedPng::compress(self.compression.id(), &self.data)?;
        Ok(DeflatedPng::new(self, zlib))
    }

    #[inline]
    fn encode_first_byte(png: &EncodeRequest<'_>, pos: usize, filter: PngFilter) -> u8 {
        let pixels = png.data;
        let byte = pixels[pos];
        let stride = png.stride;
        match filter {
            PngFilter::None => byte,
            PngFilter::Sub => byte,
            PngFilter::Up => {
                if png.vertical_flip {
                    byte.wrapping_sub(pixels[pos + stride])
                } else {
                    byte.wrapping_sub(pixels[pos - stride])
                }
            }
            PngFilter::Average => {
                if png.vertical_flip {
                    byte.wrapping_sub((pixels[pos + stride] as u16 >> 1) as u8)
                } else {
                    byte.wrapping_sub((pixels[pos - stride] as u16 >> 1) as u8)
                }
            }
            PngFilter::Paeth => {
                if png.vertical_flip {
                    byte.wrapping_sub(Self::paeth_encoding(0, pixels[pos + stride], 0))
                } else {
                    byte.wrapping_sub(Self::paeth_encoding(0, pixels[pos - stride], 0))
                }
            }
            PngFilter::AverageFirstRow => byte,
            PngFilter::PaethFirstRow => byte,
        }
    }

    #[inline]
    fn encode_byte(png: &EncodeRequest<'_>, pos: usize, filter: PngFilter) -> u8 {
        let pixels = png.data;
        let byte = pixels[pos];
        let stride = png.stride;
        let comp = png.color_type.pixel_size() as usize;
        let left = pixels[pos - comp];
        match filter {
            PngFilter::None => byte,
            PngFilter::Sub => byte.wrapping_sub(left),
            PngFilter::Up => {
                if png.vertical_flip {
                    byte.wrapping_sub(pixels[pos + stride])
                } else {
                    byte.wrapping_sub(pixels[pos - stride])
                }
            }
            PngFilter::Average => {
                let avg = if png.vertical_flip {
                    ((left as u16 + pixels[pos + stride] as u16) >> 1) as u8
                } else {
                    ((left as u16 + pixels[pos - stride] as u16) >> 1) as u8
                };
                byte.wrapping_sub(avg)
            }
            PngFilter::Paeth => {
                if png.vertical_flip {
                    byte.wrapping_sub(Self::paeth_encoding(
                        left,
                        pixels[pos + stride],
                        pixels[pos + stride - comp],
                    ))
                } else {
                    byte.wrapping_sub(Self::paeth_encoding(
                        left,
                        pixels[pos - stride],
                        pixels[pos - stride - comp],
                    ))
                }
            }
            PngFilter::AverageFirstRow => byte.wrapping_sub((left as u16 / 2) as u8),
            PngFilter::PaethFirstRow => byte.wrapping_sub(Self::paeth_encoding(left, 0, 0)),
        }
    }

    /// a = value of pos - 1
    /// b = value of pos - stride
    /// c = value of pos - 1 - stride
    /// pos = position of byte being encoded
    #[inline]
    pub(crate) fn paeth_encoding(a: u8, b: u8, c: u8) -> u8 {
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

    fn encode_line(
        png: &EncodeRequest<'_>,
        line_start: usize,
        out_line: &mut [u8],
        filter: PngFilter,
    ) {
        let comp = png.color_type.pixel_size() as usize;
        // Handle the first pixel in the scan line
        for (i, item) in out_line.iter_mut().enumerate().take(comp) {
            *item = FilteredPng::encode_first_byte(png, line_start + i, filter);
        }
        // Handle the rest of the scan line
        for (i, item) in out_line.iter_mut().enumerate().skip(comp) {
            *item = FilteredPng::encode_byte(png, line_start + i, filter);
        }
    }

    pub(crate) fn filter_fast_path(
        png: &EncodeRequest<'_>,
        line_start: usize,
        force_filter: PngFilter,
        filter_map: [PngFilter; 5],
        out_line: &mut [u8],
    ) {
        let target = &mut out_line[1..]; // skip the filter byte
        match force_filter {
            PngFilter::None => {
                for (i, item) in target.iter_mut().enumerate() {
                    *item = png.data[line_start + i];
                }
            }
            _ => {
                // the force filter may not always be possible to use
                let encoded_filter = filter_map[force_filter.id() as usize];
                Self::encode_line(png, line_start, target, encoded_filter);
            }
        }
        // Encode the force filter id for this line
        out_line[0] = force_filter.id();
    }

    pub(crate) fn filter_slow_path(
        png: &EncodeRequest<'_>,
        line_start: usize,
        filter_map: [PngFilter; 5],
        out_line: &mut [u8],
    ) {
        let mut best_filter = PngFilter::None;
        let mut best_filter_value: i32 = i32::MAX;
        let target = &mut out_line[1..]; // skip the filter byte
        for filter in filter_map {
            Self::encode_line(png, line_start, target, filter);
            let est: i32 = target.iter().map(|&b| (b as i8 as i32).abs()).sum();
            if est < best_filter_value {
                best_filter = filter;
                best_filter_value = est;
            }
        }
        Self::encode_line(png, line_start, out_line, best_filter);
    }
}
