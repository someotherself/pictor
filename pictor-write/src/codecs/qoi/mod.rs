use std::{
    io::{BufWriter, Write},
    path::Path,
};

use pictor_core::{
    PictorError, PictorResult,
    codecs::{
        color_type::ColorType,
        qoi::{
            END_MARKER, QOI_MAX_PIXELS, QoiOperation, QoiRbga,
            color_type::{Channels, ColorSpace},
            idx_color_hash,
            tags::QoiTags,
        },
    },
    samples::SampleStorage,
};

#[inline]
fn op_index(index: usize) -> u8 {
    debug_assert!(index < 64);
    QoiTags::QOI_OP_INDEX | index as u8
}

#[inline]
fn op_run(run: u8) -> u8 {
    // QOI_RUN stores run length minus 1.
    // Valid run length: 1..=62
    debug_assert!((1..=62).contains(&run));

    QoiTags::QOI_OP_RUN | (run - 1)
}

pub struct QoiEncodingRequest<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    // Compatiblity when converting formats.
    pub(crate) native_channels: Option<ColorType>,
    pub(crate) channels: Channels,
    pub(crate) color_space: ColorSpace,
    pub(crate) data: SampleStorage<'a, u8>,
}

impl<'a> QoiEncodingRequest<'a> {
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn channels(&self) -> Channels {
        self.channels
    }

    #[inline]
    pub fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        self.data.get_data()
    }

    #[doc(hidden)]
    pub fn new(
        width: u32,
        height: u32,
        color_type: ColorType,
        color_space: Option<ColorSpace>,
        data: SampleStorage<'a, u8>,
    ) -> Self {
        let (native_channels, channels) = match color_type {
            ColorType::L => (Some(ColorType::L), Channels::Rbg),
            ColorType::La => (Some(ColorType::La), Channels::Rbga),
            ColorType::Rgb => (None, Channels::Rbg),
            ColorType::Rgba => (None, Channels::Rbga),
        };
        let color_space = color_space.unwrap_or(ColorSpace::Srbg);
        Self {
            width,
            height,
            native_channels,
            channels,
            color_space,
            data,
        }
    }

    pub fn encode<W: Write>(&self, writer: W) -> PictorResult<()> {
        let mut writer = BufWriter::new(writer);

        // We handle differences in channels during format conversions
        let (input_channels, output_channels) = self.native_channels.map_or(
            (self.channels.pixel_size(), self.channels.pixel_size()),
            |n| {
                let px_size = n.comp_per_pix();
                if px_size == 1 || px_size == 3 {
                    (px_size, 3)
                } else {
                    (px_size, 4)
                }
            },
        );

        // QOI_MAGIC
        writer.write_all(b"qoif")?;

        writer.write_all(&self.width.to_be_bytes())?;
        writer.write_all(&self.height.to_be_bytes())?;
        writer.write_all(&[output_channels, self.color_space.id()])?;

        let len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|len| len.checked_mul(input_channels as usize))
            .ok_or(PictorError::MulOverflow {
                op: "Total length of file exceedes usize.",
            })?;

        let last_pixel = len - input_channels as usize;

        let mut cur = QoiRbga::zeroed();
        let mut prev = QoiRbga::default();
        let mut pixel_cache: [Option<QoiRbga>; 64] = [None; 64];
        let mut pixel = 0;
        let mut run = 0;
        while pixel < len {
            cur.set_values(input_channels, &self.data.get_data()[pixel..]);

            if cur == prev {
                run += 1;
                if run == 62 || pixel == last_pixel {
                    writer.write_all(&[op_run(run)])?;
                    run = 0;
                }
            } else {
                if run > 0 {
                    writer.write_all(&[op_run(run)])?;
                    run = 0;
                }

                let index_pos = idx_color_hash(cur);

                if pixel_cache[index_pos] == Some(cur) {
                    writer.write_all(&[op_index(index_pos)])?;
                } else {
                    pixel_cache[index_pos] = Some(cur); /* Save cur in cache */

                    if cur.alpha_eq(prev) {
                        let diff = cur.diff_from(prev);
                        match diff.check_op() {
                            QoiOperation::FitsDiff => writer.write_all(&diff.create_diff_tag())?,
                            QoiOperation::FitsLuma => writer.write_all(&diff.create_luma_tag())?,
                            QoiOperation::Rgb => writer.write_all(&cur.create_rbg_tag())?,
                        };
                    } else {
                        writer.write_all(&cur.create_rbga_tag())?;
                    }
                }
            }

            prev = cur;
            pixel += input_channels as usize;
        }

        writer.write_all(&END_MARKER)?;
        writer.flush()?;

        Ok(())
    }
}

pub struct QoiBuilder {
    width: u32,
    height: u32,
    channels: Channels,
    color_space: ColorSpace,
}

impl QoiBuilder {
    pub fn new(width: u32, height: u32, channels: Channels) -> Self {
        Self {
            width,
            height,
            channels,
            color_space: ColorSpace::Srbg,
        }
    }

    pub fn create_request<'a>(&self, data: &'a [u8]) -> PictorResult<QoiEncodingRequest<'a>> {
        if self.height as usize >= QOI_MAX_PIXELS / self.width as usize {
            return Err(PictorError::FileSizeExceeded);
        };

        Ok(QoiEncodingRequest {
            width: self.width,
            height: self.height,
            native_channels: None,
            channels: self.channels,
            color_space: self.color_space,
            data: SampleStorage::Borrow { data },
        })
    }

    pub fn color_space(&mut self, color_space: ColorSpace) -> &mut Self {
        self.color_space = color_space;
        self
    }

    pub fn encode<P: AsRef<Path>>(&self, data: &[u8], path: P) -> PictorResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.encode_with(data, &mut file)
    }

    pub fn encode_with<W: Write>(&self, data: &[u8], writer: &mut W) -> PictorResult<()> {
        let request = self.create_request(data)?;
        request.encode(writer)
    }
}
