use std::{
    fs::OpenOptions,
    io::{Cursor, Read},
    path::Path,
};

use pictor_core::{PictorError, PictorResult};

use crate::codecs::{
    jpeg::{DecodedJpeg, JpegDecodeRequest},
    png::{DecodedPng, PngDecodeRequest},
    qoi::{DecodedQoi, QoiDecodeRequest},
};

pub mod codecs;

pub enum DecodedFormat<'a> {
    Qoi(DecodedQoi<'a>),
    Png(DecodedPng<'a>),
    Jpeg(DecodedJpeg<'a>),
}

pub enum DecodedRequest {
    Qoi {
        request: QoiDecodeRequest,
        payload: Box<[u8]>, // data excluding the header
    },
    Png {
        request: PngDecodeRequest,
        payload: Box<[u8]>,
    },
    Jpeg {
        request: JpegDecodeRequest,
        payload: Box<[u8]>,
    },
}

pub fn try_decode<'a, P: AsRef<Path>>(path: P) -> PictorResult<DecodedFormat<'a>> {
    let mut file = OpenOptions::new().read(true).open(path)?;
    try_decode_with(&mut file)
}

pub fn try_decode_with<'a, R: Read>(reader: &mut R) -> PictorResult<DecodedFormat<'a>> {
    let req = try_load_with(reader)?;
    match req {
        DecodedRequest::Png { request, payload } => {
            let mut reader = payload.as_ref();
            let decoded = request.decode_internal(&mut reader)?;
            Ok(DecodedFormat::Png(decoded))
        }
        DecodedRequest::Qoi { request, payload } => {
            let mut reader = payload.as_ref();
            let decoded = request.decode_internal(&mut reader)?;
            Ok(DecodedFormat::Qoi(decoded))
        }
        DecodedRequest::Jpeg { request, payload } => {
            let mut reader = payload.as_ref();
            let decoded = request.decode_internal(&mut reader)?;
            Ok(DecodedFormat::Jpeg(decoded))
        }
    }
}

pub fn try_load<P: AsRef<Path>>(path: P) -> PictorResult<DecodedRequest> {
    let mut file = OpenOptions::new().read(true).open(path)?;
    try_load_with(&mut file)
}

pub fn try_load_with<R: Read>(reader: &mut R) -> PictorResult<DecodedRequest> {
    let mut contents = Vec::new();
    reader.read_to_end(&mut contents)?;
    try_load_internal(contents)
}

pub fn try_load_internal(bytes: Vec<u8>) -> PictorResult<DecodedRequest> {
    let mut bytes = bytes;
    // png
    {
        let mut reader = Cursor::new(bytes);
        if let Ok(png) = PngDecodeRequest::read_header(&mut reader) {
            let bytes_read = reader.position() as usize;
            let mut bytes = reader.into_inner();
            let payload = bytes.drain(bytes_read..).collect();
            return Ok(DecodedRequest::Png {
                request: png,
                payload,
            });
        }
        bytes = reader.into_inner();
    }

    // Qoi
    {
        let mut reader = Cursor::new(bytes);
        if let Ok(qoi) = QoiDecodeRequest::read_header(&mut reader, None) {
            let bytes_read = reader.position() as usize;
            let mut bytes = reader.into_inner();
            let payload = bytes.drain(bytes_read..).collect();
            return Ok(DecodedRequest::Qoi {
                request: qoi,
                payload,
            });
        }
        bytes = reader.into_inner();
    }

    // Jpeg
    {
        let mut reader = Cursor::new(bytes);
        if let Ok(jpeg) = JpegDecodeRequest::read_header(&mut reader) {
            let bytes_read = reader.position() as usize;
            let mut bytes = reader.into_inner();
            let payload = bytes.drain(bytes_read..).collect();
            return Ok(DecodedRequest::Jpeg {
                request: jpeg,
                payload,
            });
        }
    }

    Err(PictorError::InvalidFormat {
        msg: "Format not supported".to_string(),
    })
}
