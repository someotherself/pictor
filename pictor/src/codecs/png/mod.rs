use std::{io::Write, marker::PhantomData, path::Path};

use pictor_core::{
    PictorResult,
    codecs::{
        color_type::{BitDepth, ColorType},
        png::filters::PngFilter,
    },
    samples::{Sample, SampleStorage},
};
use pictor_write::codecs::png::{PngEncodingRequest, deflate::CompressionLevel};

pub struct PngBuilderBorrowed<'a, S: Sample> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: Option<usize>,
    pub(crate) compression: CompressionLevel,
    pub(crate) color_type: ColorType,
    pub(crate) bit_depth: BitDepth,
    pub(crate) filter: Option<PngFilter>,
    pub(crate) data: SampleStorage<'a, S>,
    pub(crate) _format: PhantomData<S>,
}

impl<'a, S: Sample> PngBuilderBorrowed<'a, S> {
    pub fn compression(&mut self, compression: CompressionLevel) -> &mut Self {
        self.compression = compression;
        self
    }

    pub fn force_filter(&mut self, filter: PngFilter) -> &mut Self {
        self.filter = Some(filter);
        self
    }

    pub fn encode<P: AsRef<Path>>(&self, path: P) -> PictorResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.encode_with(&mut file)
    }

    pub fn encode_with<W: Write>(&self, writer: &mut W) -> PictorResult<()> {
        let stride = self.stride.unwrap_or(usize::try_from(
            self.width * self.color_type.comp_per_pix() as u32,
        )?);

        let req = PngEncodingRequest::new(
            self.width,
            self.height,
            stride,
            self.color_type,
            self.bit_depth,
            self.compression,
            self.filter,
            false,
            self.data.as_borrowed(),
        );
        req.encode(writer)
    }
}
