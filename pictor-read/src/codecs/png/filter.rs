use pictor_core::{PictorError, PictorResult};

use crate::codecs::png::PngDecodeRequest;
use pictor_core::codecs::png::filters::PngFilter;

pub(crate) fn remove_filter(req: &PngDecodeRequest, data: &[u8]) -> PictorResult<Vec<u8>> {
    unfilter_scanlines(req, data)
}

fn unfilter_scanlines(req: &PngDecodeRequest, data: &[u8]) -> PictorResult<Vec<u8>> {
    let Some(out_size) = req
        .height
        .checked_mul(req.width)
        .and_then(|r| r.checked_mul(req.color_type.comp_per_pix() as u32))
        .and_then(|r| r.checked_mul(req.bit_depth.bytes_per_comp() as u32))
    else {
        return Err(PictorError::MulOverflow { op: "" });
    };

    let mut out = vec![0u8; out_size as usize];

    for scanline in 0..req.height {
        let in_line_start = req.stride * scanline as usize;

        let Some(out_line_start) = scanline
            .checked_mul(req.width)
            .and_then(|r| r.checked_mul(req.color_type.comp_per_pix() as u32))
            .and_then(|r| r.checked_mul(req.bit_depth.bytes_per_comp() as u32))
        else {
            return Err(PictorError::MulOverflow { op: "" });
        };

        let encoded_line = &data[in_line_start..in_line_start + req.stride];

        let map = if scanline == 0 {
            PngFilter::MAPPING_FIRST_ROW
        } else {
            PngFilter::MAPPING
        };

        remove_filter_scanline(req, &mut out, encoded_line, out_line_start as usize, map)?;
    }

    Ok(out)
}

#[inline]
fn remove_filter_scanline(
    req: &PngDecodeRequest,
    decoded_data: &mut [u8],
    encoded_line: &[u8],
    out_line_start: usize,
    map: [PngFilter; 5],
) -> PictorResult<()> {
    let filter_byte = encoded_line[0];
    if filter_byte > 4 {
        return Err(PictorError::InvalidFormat {
            msg: "Invalid filter type".to_string(),
        });
    }
    let encoded_line = &encoded_line[1..]; // skip the filter byte from now on
    let filter = map[filter_byte as usize];
    decode_line(req, decoded_data, encoded_line, out_line_start, filter);
    Ok(())
}

fn decode_line(
    req: &PngDecodeRequest,
    decoded_data: &mut [u8],
    encoded_line: &[u8],
    out_line_start: usize,
    filter: PngFilter,
) {
    let comp = (req.color_type.comp_per_pix() as usize) * req.bit_depth.bytes_per_comp() as usize;
    let row_bytes = comp * req.width as usize;

    for (i, &item) in encoded_line.iter().enumerate().take(comp) {
        decoded_data[out_line_start + i] =
            decode_first_byte(row_bytes, decoded_data, out_line_start + i, item, filter);
    }
    for (i, &item) in encoded_line.iter().enumerate().skip(comp) {
        decoded_data[out_line_start + i] = decode_byte(
            req,
            row_bytes,
            decoded_data,
            out_line_start + i,
            item,
            filter,
        );
    }
}

#[inline]
fn decode_first_byte(stride: usize, data: &[u8], pos: usize, byte: u8, filter: PngFilter) -> u8 {
    match filter {
        PngFilter::None => byte,
        PngFilter::Sub => byte,
        PngFilter::Up => byte.wrapping_add(data[pos - stride]),
        PngFilter::Average => byte.wrapping_add((data[pos - stride] as u16 >> 1) as u8),
        PngFilter::Paeth => byte.wrapping_add(PngFilter::paeth_predictor(0, data[pos - stride], 0)),
        PngFilter::AverageFirstRow => byte,
        PngFilter::PaethFirstRow => byte,
    }
}

#[inline]
fn decode_byte(
    req: &PngDecodeRequest,
    row_bytes: usize,
    decoded_data: &[u8],
    pos: usize,
    byte: u8,
    filter: PngFilter,
) -> u8 {
    let comp = (req.color_type.comp_per_pix() as usize) * req.bit_depth.bytes_per_comp() as usize;
    let left = decoded_data[pos - comp];
    match filter {
        PngFilter::None => byte,
        PngFilter::Sub => byte.wrapping_add(left),
        PngFilter::Up => byte.wrapping_add(decoded_data[pos - row_bytes]),
        PngFilter::Average => {
            let avg = ((left as u16 + decoded_data[pos - row_bytes] as u16) >> 1) as u8;
            byte.wrapping_add(avg)
        }
        PngFilter::Paeth => byte.wrapping_add(PngFilter::paeth_predictor(
            left,
            decoded_data[pos - row_bytes],
            decoded_data[pos - row_bytes - comp],
        )),
        PngFilter::AverageFirstRow => byte.wrapping_add((left as u16 / 2) as u8),
        PngFilter::PaethFirstRow => byte.wrapping_add(PngFilter::paeth_predictor(left, 0, 0)),
    }
}
