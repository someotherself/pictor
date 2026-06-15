use std::{io::Write, path::Path};

pub mod deflate;
pub mod filter;

use pictor_core::{
    codecs::{
        color_type::ColorType,
        png::{filters::PngFilter, CRC_TABLE},
    },
    PictorResult,
};

use crate::codecs::png::{
    deflate::{CompressionLevel, DeflatedPng},
    filter::FilteredPng,
};

pub struct EncodeRequest<'a> {
    width: u32,
    height: u32,
    stride: usize,
    color_type: ColorType,
    compression_level: CompressionLevel,
    filter: Option<PngFilter>,
    vertical_flip: bool,
    data: &'a [u8],
}

impl<'a> EncodeRequest<'a> {
    /// Calculates the total buffer size needed after the filters get applied
    fn filtered_size(&self) -> usize {
        (self.width as usize * self.color_type.pixel_size() as usize + 1) * self.height as usize
    }

    fn current_row_adjusted(&self, scanline: u32) -> usize {
        let scanline = scanline as usize;
        let height = self.height as usize;

        let y = if self.vertical_flip {
            height - 1 - scanline
        } else {
            scanline
        };
        self.stride * y
    }

    #[cfg(feature = "rayon")]
    pub fn filter_scanlines(&self) -> PictorResult<FilteredPng> {
        use rayon::{
            iter::{IndexedParallelIterator, ParallelIterator},
            slice::ParallelSliceMut,
        };

        let filtered_stride = self.stride + 1;
        let mut out = Vec::new();
        out.resize_with(self.filtered_size(), || 0_u8);

        out.par_chunks_mut(filtered_stride)
            .enumerate()
            .for_each_init(
                || vec![0u8; filtered_stride],
                |scratch, (scanline, out_line)| {
                    let in_line_start = self.current_row_adjusted(scanline as u32);
                    let map = if scanline == 0 {
                        PngFilter::MAPPING_FIRST_ROW
                    } else {
                        PngFilter::MAPPING
                    };

                    if let Some(force_filter) = self.filter {
                        FilteredPng::filter_fast_path(
                            self,
                            in_line_start,
                            force_filter,
                            map,
                            out_line,
                        );
                    } else {
                        // add rayon
                        FilteredPng::filter_slow_path(self, in_line_start, map, out_line, scratch);
                    }
                },
            );
        FilteredPng::new(self, out)
    }

    #[cfg(not(feature = "rayon"))]
    pub fn filter_scanlines(&self) -> PictorResult<FilteredPng> {
        let mut out_line_start: usize = 0;
        let filtered_stride = self.stride + 1;
        let mut out = Vec::new();
        out.resize_with(self.filtered_size(), || 0_u8);
        let mut scratch = vec![0u8; filtered_stride];

        for scanline in 0..self.height {
            let in_line_start = self.current_row_adjusted(scanline);
            let map = if scanline == 0 {
                PngFilter::MAPPING_FIRST_ROW
            } else {
                PngFilter::MAPPING
            };
            let out_line_end = out_line_start + filtered_stride;
            let out_line = &mut out[out_line_start..out_line_end];

            if let Some(force_filter) = self.filter {
                FilteredPng::filter_fast_path(self, in_line_start, force_filter, map, out_line);
            } else {
                FilteredPng::filter_slow_path(self, in_line_start, map, out_line, &mut scratch);
            }

            out_line_start = out_line_end;
        }

        FilteredPng::new(self, out)
    }
}

pub struct PngBuilder {
    width: u32,
    height: u32,
    stride: Option<usize>,
    compression: CompressionLevel,
    color_type: ColorType,
    vertical_flip: bool,
    filter: Option<PngFilter>,
}

impl PngBuilder {
    pub fn new(width: u32, height: u32, color_type: ColorType) -> Self {
        Self {
            width,
            height,
            stride: None,
            color_type,
            compression: CompressionLevel::Default,
            vertical_flip: false,
            filter: None,
        }
    }

    pub fn stride(&mut self, stride: usize) -> &mut Self {
        self.stride = Some(stride);
        self
    }

    pub fn compression(&mut self, compression: CompressionLevel) -> &mut Self {
        self.compression = compression;
        self
    }

    pub fn force_filter(&mut self, filter: PngFilter) -> &mut Self {
        self.filter = Some(filter);
        self
    }

    pub fn vertical_flip(&mut self, yes: bool) -> &mut Self {
        self.vertical_flip = yes;
        self
    }

    pub fn create_request<'a>(&mut self, data: &'a [u8]) -> PictorResult<EncodeRequest<'a>> {
        self.new_png_request(data)
    }

    pub fn filter_scanlines(&mut self, data: &[u8]) -> PictorResult<FilteredPng> {
        let req = self.new_png_request(data)?;
        req.filter_scanlines()
    }

    pub fn compress(&mut self, data: &[u8]) -> PictorResult<DeflatedPng> {
        let req = self.new_png_request(data)?;
        let filtered = req.filter_scanlines()?;
        filtered.compress()
    }

    pub fn encode_in_memory(&mut self, data: &[u8]) -> PictorResult<EncodedPng> {
        let req = self.new_png_request(data)?;
        let filtered = req.filter_scanlines()?;
        let zlib = filtered.compress()?;
        zlib.encode_in_memory_internal()
    }

    fn new_png_request<'a>(&self, data: &'a [u8]) -> PictorResult<EncodeRequest<'a>> {
        let stride = if let Some(stride) = self.stride {
            stride
        } else {
            usize::try_from(self.width * self.color_type.pixel_size() as u32)?
        };
        Ok(EncodeRequest {
            width: self.width,
            height: self.height,
            stride,
            color_type: self.color_type,
            compression_level: self.compression,
            filter: self.filter,
            vertical_flip: self.vertical_flip,
            data,
        })
    }

    pub fn encode_with<W: Write>(&self, data: &[u8], writer: &mut W) -> PictorResult<()> {
        let png_req = self.new_png_request(data)?;

        let filtered = png_req.filter_scanlines()?;

        // Compress the filtered payload
        let zlib = filtered.compress()?;

        // Create the output with the header
        let encoded = zlib.encode_in_memory_internal()?;
        writer.write_all(&encoded.0)?;

        Ok(())
    }

    pub fn encode(&mut self, data: &[u8], path: &Path) -> PictorResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.encode_with(data, &mut file)?;
        Ok(())
    }
}

pub struct EncodedPng(pub(crate) Vec<u8>);

impl EncodedPng {
    pub(crate) fn encode_in_memory(deflated: &DeflatedPng) -> PictorResult<Vec<u8>> {
        let zlib = &deflated.data;

        /// PNG Signature
        const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

        let out_cap = 8 + 12 + 13 + 12 + zlib.len() + 12;
        let mut out: Vec<u8> = Vec::with_capacity(out_cap);

        out.extend_from_slice(&SIG);

        // IHDR Chunk
        let ihdr_chunk_len = 13;
        // Length of the header
        Self::write_be_bytes(&mut out, ihdr_chunk_len);
        out.extend_from_slice(b"IHDR");
        // Width of the image in pixels
        Self::write_be_bytes(&mut out, deflated.width);
        // Height of the image in pixels
        Self::write_be_bytes(&mut out, deflated.height);
        // Number of bits per  sample
        out.push(8_u8);
        // Color type byte
        out.push(deflated.color_type.id());
        // Compression method. 0 for deflate/inflate with 32768 window
        out.push(0);
        // Filter method. Only 0 is defined by the standard.
        out.push(0);
        // Interlace method. 0 defined by the standard
        out.push(0);
        // End of chunk. Write crc32
        Self::final_crc(&mut out, ihdr_chunk_len as usize);

        // IDAT Chunk. payload
        let idat_chunk_len = zlib.len();
        Self::write_be_bytes(&mut out, idat_chunk_len as u32);
        out.extend_from_slice(b"IDAT");
        out.extend_from_slice(zlib);
        // End of chunk. Write crc32
        Self::final_crc(&mut out, idat_chunk_len);

        // IEND Chunk
        Self::write_be_bytes(&mut out, 0); // Length of the chunk is 0
        out.extend_from_slice(b"IEND");
        Self::final_crc(&mut out, 0);

        Ok(out)
    }

    fn final_crc(out: &mut Vec<u8>, len: usize) {
        let chunk_len = len + 4;
        let chunk_start = out.len() - chunk_len;

        let mut crc: u32 = !0;
        for &byte in out.iter().skip(chunk_start).take(chunk_len) {
            let index = (byte ^ (crc as u8)) as usize;
            crc = (crc >> 8) ^ CRC_TABLE[index];
        }
        Self::write_be_bytes(out, !crc);
    }

    fn write_be_bytes(out: &mut Vec<u8>, val: u32) {
        out.extend_from_slice(&(val).to_be_bytes());
    }

    pub fn write_to_file(&self, path: &Path) -> PictorResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.write_with(&mut file)
    }

    pub fn write_with<W: Write>(&self, writer: &mut W) -> PictorResult<()> {
        writer.write_all(&self.0)?;
        Ok(())
    }
}
