use std::{io::Write, path::Path};

use pictor_core::{PictorResult, codecs::color_type::ColorType, samples::SampleStorage};
use pictor_write::codecs::jpeg::JpegEncodingRequest;

pub struct JpegBuilderBorrowed<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) color_type: ColorType,
    pub(crate) quality: u32,
    pub(crate) data: SampleStorage<'a, u8>, // Only u8 supported for jpeg
}

impl<'a> JpegBuilderBorrowed<'a> {
    pub fn quality(&mut self, quality: u32) -> &mut Self {
        self.quality = quality.clamp(1, 100);
        self
    }

    // TODO
    // Decide what to do when width and height don't fit in a u16.
    // Add an enum as param instead
    // Variants: crop_to_fit, resize_to_fit
    fn _resize_to_fit_format_limits(_yes: bool) {}

    pub fn encode<P: AsRef<Path>>(&self, path: P) -> PictorResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.encode_with(&mut file)
    }

    pub fn encode_with<W: Write>(&self, writer: &mut W) -> PictorResult<()> {
        let width = u16::try_from(self.width)?;
        let height = u16::try_from(self.height)?;
        let quality_factor = if self.quality < 50 {
            5000 / self.quality
        } else {
            200 - self.quality * 2
        };
        let subsample = self.quality <= 90;
        let res = JpegEncodingRequest::new(
            width,
            height,
            self.color_type,
            subsample,
            quality_factor,
            self.data.get_data(),
        );
        res.encode(writer)
    }
}
