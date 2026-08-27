use std::marker::PhantomData;

use pictor_core::{
    codecs::color_type::{BitDepth, ColorType}, samples::{Sample, SampleStorage},
};
pub use pictor_read;
use pictor_read::codecs::{jpeg::DecodedJpeg, png::DecodedPng, qoi::DecodedQoi};
pub use pictor_write;
use pictor_write::codecs::png::deflate::CompressionLevel;

use crate::codecs::{jpeg::JpegBuilderBorrowed, png::PngBuilderBorrowed, qoi::QoiBuilderBorrowed};

pub mod codecs;

pub struct DecodedImage<'a, S: Sample> {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: BitDepth,
    pub data: SampleStorage<'a, S>,
}

pub trait Convert<'a> {
    type Sample: Sample;

    fn convert(self) -> DecodedImage<'a, Self::Sample>;
}

impl<'a> Convert<'a> for DecodedQoi<'a> {
    type Sample = u8;

    fn convert(self) -> DecodedImage<'a, Self::Sample> {
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
    type Sample = u8;

    fn convert(self) -> DecodedImage<'a, Self::Sample> {
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
    type Sample = u8;

    fn convert(self) -> DecodedImage<'a, Self::Sample> {
        DecodedImage {
            width: self.width,
            height: self.height,
            color_type: self.color_type,
            bit_depth: self.bit_depth,
            data: self.data,
        }
    }
}

impl<'a, S: Sample> DecodedImage<'a, S> {
    pub fn jpeg(self) -> JpegBuilderBorrowed<'a> {
        JpegBuilderBorrowed {
            width: self.width,
            height: self.height,
            color_type: self.color_type,
            quality: 90,
            data: S::downsample_to_u8_samples(self.data),
        }
    }

    pub fn qoi(self) -> QoiBuilderBorrowed<'a> {
        QoiBuilderBorrowed {
            width: self.width,
            height: self.height,
            native_channels: self.color_type,
            // Data received is borrowed
            // If the data is u16, it gets downsampled and turned into SampleStorage::Owned
            data: S::downsample_to_u8_samples(self.data),
        }
    }

    pub fn png(self) -> PngBuilderBorrowed<'a, S> {
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
            _format: PhantomData,
        }
    }
}
