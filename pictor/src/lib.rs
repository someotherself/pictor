use std::{fs::OpenOptions, io::Read, path::Path};

use pictor_core::{
    PictorResult,
    codecs::color_type::{BitDepth, ColorType},
    samples::{Sample, SampleStorage},
};
pub use pictor_read;
use pictor_read::{
    DecodedFormat,
    codecs::{jpeg::DecodedJpeg, png::DecodedPng, qoi::DecodedQoi},
    try_decode_with,
};
pub use pictor_write;
use pictor_write::codecs::png::deflate::CompressionLevel;

use crate::codecs::{jpeg::JpegBuilderBorrowed, png::PngBuilderBorrowed, qoi::QoiBuilderBorrowed};

pub mod codecs;

pub fn convert<'a, P: AsRef<Path>>(path: P) -> PictorResult<DecodedImage<'a>> {
    let file = OpenOptions::new().read(true).open(path)?;
    convert_with(file)
}

pub fn convert_with<'a, R: Read>(reader: R) -> PictorResult<DecodedImage<'a>> {
    let mut reader = reader;
    Ok(try_decode_with(&mut reader)?.convert())
}

pub struct DecodedImage<'a> {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: BitDepth,
    // All the formats are decoded to u8
    // Png can be u16, but it is also stored as u8
    pub data: SampleStorage<'a, u8>,
}

pub trait Convert<'a> {
    fn convert(self) -> DecodedImage<'a>;
}

impl<'a> Convert<'a> for DecodedFormat<'a> {
    fn convert(self) -> DecodedImage<'a> {
        match self {
            DecodedFormat::Jpeg(jpeg) => DecodedImage {
                width: jpeg.width,
                height: jpeg.height,
                color_type: jpeg.color_type,
                bit_depth: jpeg.bit_depth,
                data: jpeg.data,
            },
            DecodedFormat::Png(png) => DecodedImage {
                width: png.width,
                height: png.height,
                color_type: png.color_type,
                bit_depth: png.bit_depth,
                data: png.data,
            },
            DecodedFormat::Qoi(qoi) => DecodedImage {
                width: qoi.width,
                height: qoi.height,
                color_type: qoi.channels.into(),
                bit_depth: qoi.bit_depth,
                data: qoi.data,
            },
        }
    }
}

impl<'a> Convert<'a> for DecodedQoi<'a> {
    fn convert(self) -> DecodedImage<'a> {
        DecodedImage {
            width: self.width,
            height: self.height,
            color_type: self.channels.into(),
            bit_depth: self.bit_depth,
            data: self.data,
        }
    }
}

impl<'a> Convert<'a> for DecodedJpeg<'a> {
    fn convert(self) -> DecodedImage<'a> {
        DecodedImage {
            width: self.width,
            height: self.height,
            color_type: self.color_type,
            bit_depth: self.bit_depth,
            data: self.data,
        }
    }
}

impl<'a> Convert<'a> for DecodedPng<'a> {
    fn convert(self) -> DecodedImage<'a> {
        DecodedImage {
            width: self.width,
            height: self.height,
            color_type: self.color_type,
            bit_depth: self.bit_depth,
            data: self.data,
        }
    }
}

impl<'a> DecodedImage<'a> {
    pub fn jpeg(self) -> JpegBuilderBorrowed<'a> {
        let data = match self.bit_depth {
            BitDepth::U8 => self.data,
            BitDepth::U16 => {
                let u16_samples = <u16 as Sample>::from_be_bytes(self.data.get_data());
                let data = <u16 as Sample>::downsample_to_u8_samples(u16_samples).into_vec();
                SampleStorage::Owned { data }
            }
        };
        JpegBuilderBorrowed {
            width: self.width,
            height: self.height,
            color_type: self.color_type,
            quality: 90,
            data,
        }
    }

    pub fn qoi(self) -> QoiBuilderBorrowed<'a> {
        let data = match self.bit_depth {
            BitDepth::U8 => self.data,
            BitDepth::U16 => {
                let u16_samples = <u16 as Sample>::from_be_bytes(self.data.get_data());
                let data = <u16 as Sample>::downsample_to_u8_samples(u16_samples).into_vec();
                SampleStorage::Owned { data }
            }
        };
        QoiBuilderBorrowed {
            width: self.width,
            height: self.height,
            native_channels: self.color_type,
            // Data received is borrowed
            // If the data is u16, it gets downsampled and turned into SampleStorage::Owned
            data,
        }
    }

    pub fn png(self) -> PngBuilderBorrowed<'a> {
        // The png encoder supports both u8 and u16. No need to downsample
        PngBuilderBorrowed {
            width: self.width,
            height: self.height,
            stride: None,
            compression: CompressionLevel::Default,
            color_type: self.color_type,
            bit_depth: self.bit_depth,
            filter: None,
            data: self.data,
        }
    }
}
