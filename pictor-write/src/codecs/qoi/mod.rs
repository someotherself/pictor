use std::{
    io::{BufWriter, Write},
    path::Path,
};

use pictor_core::{
    codecs::qoi::{
        color_type::{Channels, ColorSpace},
        idx_color_hash,
        tags::QoiTags,
        QoiOperation, QoiRbga, END_MARKER, QOI_MAX_PIXELS,
    },
    PictorError, PictorResult,
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

pub struct EncodingRequest<'a> {
    width: u32,
    height: u32,
    channels: Channels,
    color_space: ColorSpace,
    data: &'a [u8],
}

impl<'a> EncodingRequest<'a> {
    fn encode<W: Write>(&self, writer: W) -> PictorResult<()> {
        let mut writer = BufWriter::new(writer);

        // QOI_MAGIC
        writer.write_all(b"qoif")?;

        writer.write_all(&self.width.to_be_bytes())?;
        writer.write_all(&self.height.to_be_bytes())?;
        writer.write_all(&[self.channels.pixel_size(), self.color_space.id()])?;

        let len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|len| len.checked_mul(self.channels.pixel_size() as usize))
            .ok_or(PictorError::MulOverflow {
                op: "Total length of file excedes usize.",
            })?;

        let last_pixel = len - self.channels.pixel_size() as usize;

        let mut cur = QoiRbga::zeroed();
        let mut prev = QoiRbga::default();
        let mut pixel_cache: [Option<QoiRbga>; 64] = [None; 64];
        let mut pixel = 0;
        let mut run = 0;
        while pixel < len {
            cur.set_values(self.channels, &self.data[pixel..]);

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
            pixel += self.channels.pixel_size() as usize;
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

    pub fn create_request<'a>(&self, data: &'a [u8]) -> PictorResult<EncodingRequest<'a>> {
        if self.height as usize >= QOI_MAX_PIXELS / self.width as usize {
            return Err(PictorError::FileSizeExceeded);
        };

        Ok(EncodingRequest {
            width: self.width,
            height: self.height,
            channels: self.channels,
            color_space: self.color_space,
            data,
        })
    }

    pub fn encode(&self, data: &[u8], path: &Path) -> PictorResult<()> {
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
