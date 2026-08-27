use std::{io::Write, path::Path};

use pictor_core::{PictorResult, codecs::color_type::ColorType, samples::SampleStorage};
use pictor_write::codecs::qoi::QoiEncodingRequest;

pub struct QoiBuilderBorrowed<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) native_channels: ColorType,
    pub(crate) data: SampleStorage<'a, u8>,
}

impl<'a> QoiBuilderBorrowed<'a> {
    pub fn encode<P: AsRef<Path>>(&self, path: P) -> PictorResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.encode_with(&mut file)
    }

    pub fn encode_with<W: Write>(&self, writer: &mut W) -> PictorResult<()> {
        let req = QoiEncodingRequest::new(
            self.width,
            self.height,
            self.native_channels,
            None,
            self.data.as_borrowed(),
        );
        req.encode(writer)
    }
}
