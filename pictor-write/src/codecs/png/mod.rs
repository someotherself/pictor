use std::{io::Write, path::Path};

pub mod deflate;
pub mod filter;

use pictor_core::{
    PictorResult,
    codecs::{
        color_type::{BitDepth, ColorType},
        png::{PNG_SIG, filters::PngFilter, generate_crc},
    },
    samples::{Sample, SampleStorage},
};

use crate::codecs::png::{
    deflate::{CompressionLevel, DeflatedPng},
    filter::FilteredPng,
};

pub struct PngEncodingRequest<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
    pub(crate) color_type: ColorType,
    pub(crate) bit_depth: BitDepth,
    pub(crate) compression_level: CompressionLevel,
    pub(crate) filter: Option<PngFilter>,
    pub(crate) vertical_flip: bool,
    pub(crate) data: SampleStorage<'a, u8>, // must always be raw bytes for filtering
}

impl<'a> PngEncodingRequest<'a> {
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.stride
    }

    #[inline]
    pub fn color_type(&self) -> ColorType {
        self.color_type
    }

    #[inline]
    pub fn compression_level(&self) -> CompressionLevel {
        self.compression_level
    }

    #[inline]
    pub fn filter(&self) -> Option<PngFilter> {
        self.filter
    }

    #[inline]
    pub fn vertical_flip(&self) -> bool {
        self.vertical_flip
    }

    // #[inline]
    // pub fn data_raw(&self) -> &[u8] {
    //     &self.data
    // }

    #[allow(clippy::too_many_arguments)]
    #[doc(hidden)]
    pub fn new<S: Sample>(
        width: u32,
        height: u32,
        stride: usize,
        color_type: ColorType,
        bit_depth: BitDepth,
        compression_level: CompressionLevel,
        filter: Option<PngFilter>,
        vertical_flip: bool,
        data: SampleStorage<'a, S>,
    ) -> Self {
        let data = S::into_be_bytes(data);
        Self {
            width,
            height,
            stride,
            color_type,
            bit_depth,
            compression_level,
            filter,
            vertical_flip,
            data,
        }
    }

    /// Calculates the total buffer size needed after the filters get applied
    fn filtered_size(&self) -> usize {
        (self.width as usize
            * (self.color_type.comp_per_pix() as usize)
            * self.bit_depth.bytes_per_comp() as usize
            + 1)
            * self.height as usize
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
    pub fn filter_scanlines(&self) -> PictorResult<FilteredPng<S>> {
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
                || vec![0u8; self.stride],
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
        let mut out = vec![0_u8; self.filtered_size()];
        let mut scratch = vec![0u8; self.stride];

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

    pub fn encode<W: Write>(&self, writer: &mut W) -> PictorResult<()> {
        let filtered = self.filter_scanlines()?;
        let zlib = filtered.compress()?;
        let encoded = zlib.encode_in_memory_internal()?;
        writer.write_all(&encoded.0)?;
        Ok(())
    }
}

pub struct PngBuilder {
    width: u32,
    height: u32,
    stride: Option<usize>,
    compression: CompressionLevel,
    color_type: ColorType,
    #[allow(unused)]
    bit_depth: BitDepth,
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
            bit_depth: BitDepth::U8, // Temporaty value
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

    fn new_png_request<'a, S: Sample>(
        &self,
        data: SampleStorage<'a, S>,
    ) -> PictorResult<PngEncodingRequest<'a>> {
        let stride = self.stride.unwrap_or(usize::try_from(
            self.width * self.color_type.comp_per_pix() as u32 * S::BYTES_PER_SAMPLE as u32,
        )?);
        let bit_depth = if S::BYTES_PER_SAMPLE == 1 {
            BitDepth::U8
        } else {
            BitDepth::U16
        };
        let data = S::into_be_bytes(data);

        Ok(PngEncodingRequest {
            width: self.width,
            height: self.height,
            stride,
            color_type: self.color_type,
            bit_depth,
            compression_level: self.compression,
            filter: self.filter,
            vertical_flip: self.vertical_flip,
            data,
        })
    }

    pub fn encode_with<W: Write, S: Sample>(&self, data: &[S], writer: &mut W) -> PictorResult<()> {
        let png_req = self.new_png_request(SampleStorage::Borrow { data })?;

        let filtered = png_req.filter_scanlines()?;

        // Compress the filtered payload
        let zlib = filtered.compress()?;

        // Create the output with the header
        let encoded = zlib.encode_in_memory_internal()?;
        writer.write_all(&encoded.0)?;

        Ok(())
    }

    pub fn encode<S: Sample, P: AsRef<Path>>(&mut self, data: &[S], path: P) -> PictorResult<()> {
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

        let out_cap = 8 + 12 + 13 + 12 + zlib.len() + 12;
        let mut out: Vec<u8> = Vec::with_capacity(out_cap);

        // PNG signature
        out.extend_from_slice(&PNG_SIG);

        // IHDR Chunk
        let ihdr_chunk_len = 13;
        // Length of the header
        Self::write_be_bytes(&mut out, ihdr_chunk_len);
        out.extend_from_slice(b"IHDR");
        // Width of the image in pixels
        Self::write_be_bytes(&mut out, deflated.width);
        // Height of the image in pixels
        Self::write_be_bytes(&mut out, deflated.height);
        // Number of bits per sample
        out.push(deflated.bit_depth.bits_per_comp());
        // Color type byte
        out.push(deflated.color_type.id());
        // Compression method. 0 for deflate/inflate with 32768 window
        out.push(0);
        // Filter method. Only 0 is defined by the standard.
        out.push(0);
        // Interlace method. 0 defined by the standard
        out.push(0);
        // End of chunk. Write crc32
        let crc = generate_crc(&out, ihdr_chunk_len as usize);
        Self::write_be_bytes(&mut out, crc);

        // IDAT Chunk. payload
        let idat_chunk_len = zlib.len();
        Self::write_be_bytes(&mut out, idat_chunk_len as u32);
        out.extend_from_slice(b"IDAT");
        out.extend_from_slice(zlib);
        // End of chunk. Write crc32
        let crc = generate_crc(&out, idat_chunk_len);
        Self::write_be_bytes(&mut out, crc);

        // IEND Chunk
        Self::write_be_bytes(&mut out, 0); // Length of the chunk is 0
        out.extend_from_slice(b"IEND");
        let crc = generate_crc(&out, 0);
        Self::write_be_bytes(&mut out, crc);

        Ok(out)
    }

    fn write_be_bytes(out: &mut Vec<u8>, val: u32) {
        out.extend_from_slice(&(val).to_be_bytes());
    }

    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> PictorResult<()> {
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
