use std::{
    fs::OpenOptions,
    io::{BufReader, Read},
    path::Path,
};

use pictor_core::{
    PictorError, PictorResult,
    codecs::{
        color_type::{BitDepth, ColorType},
        png::{PNG_SIG, generate_crc},
    },
    samples::SampleStorage,
};

pub mod filter;

#[derive(Debug)]
pub struct PngDecodeRequest {
    width: u32,
    height: u32,
    bit_depth: BitDepth,
    color_type: ColorType,
    stride: usize,
}

struct ChunkTag(String);

struct ChunkMeta {
    length: usize,
    tag: ChunkTag,
}

impl ChunkMeta {
    fn from_bytes<R: Read>(reader: &mut R) -> PictorResult<Self> {
        let mut buff = vec![0_u8; 8];
        reader.read_exact(&mut buff)?;
        let length = u32::from_be_bytes(buff[..4].try_into().unwrap()) as usize;
        let tag = str::from_utf8(&buff[4..]).unwrap_or("").to_string();
        Ok(Self {
            length,
            tag: ChunkTag(tag),
        })
    }
}

impl PngDecodeRequest {
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn color_type(&self) -> ColorType {
        self.color_type
    }

    pub fn decode<'a, P: AsRef<Path>>(path: P) -> PictorResult<DecodedPng<'a>> {
        let file = OpenOptions::new().read(true).open(path)?;
        Self::decode_with(file)
    }

    pub(crate) fn decode_internal<'a, R: Read>(
        &self,
        reader: &mut R,
    ) -> PictorResult<DecodedPng<'a>> {
        let mut pallete: Option<Vec<[u8; 3]>> = None;
        let mut payload: Vec<u8> = vec![];
        let mut seen_idat: bool = false;
        let mut cons_idat: bool = false;
        let result: Result<(), PictorError>;

        loop {
            let meta = ChunkMeta::from_bytes(reader)?;
            if meta.tag.0.as_str() != "IDAT" {
                cons_idat = false
            };
            match meta.tag.0.as_str() {
                "IDAT" => {
                    if seen_idat && !cons_idat {
                        result = Err(PictorError::InvalidFormat {
                            msg: "IDAT chunk out of order".to_string(),
                        });
                        break;
                    }
                    if let Err(e) = self.check_idat(reader, &mut payload, meta.length) {
                        result = Err(e);
                        break;
                    }
                    cons_idat = true;
                    seen_idat = true;
                    continue;
                }
                "CgBI" => {
                    if let Err(e) = self.skip_marker(reader, meta.length) {
                        result = Err(e);
                        break;
                    }
                    continue;
                }
                "PLTE" => match self.check_plte(reader, seen_idat, meta.length) {
                    Ok(plte) => {
                        pallete = Some(plte);
                        continue;
                    }
                    Err(e) => {
                        result = Err(e);
                        break;
                    }
                },
                "tRNS" => {
                    if let Err(e) = self.check_trns(seen_idat) {
                        result = Err(e);
                        break;
                    }
                    continue;
                }
                "IEND" => {
                    result = self.check_iend(reader, seen_idat, meta.length);
                    break;
                }
                "sBIT" => {
                    if let Err(e) = self.skip_marker(reader, meta.length) {
                        result = Err(e);
                        break;
                    }
                    continue;
                }

                _ => {
                    if let Err(e) = self.skip_marker(reader, meta.length) {
                        result = Err(e);
                        break;
                    }
                    continue;
                }
            }
        }

        result?;

        let inflated = Self::inflate(&payload)?;
        let decoded = filter::remove_filter(self, &inflated)?;

        Ok(DecodedPng {
            width: self.width,
            height: self.height,
            color_type: self.color_type,
            bit_depth: self.bit_depth,
            pallete,
            data: SampleStorage::Owned { data: decoded },
        })
    }

    pub fn decode_with<'a, R: Read>(reader: R) -> PictorResult<DecodedPng<'a>> {
        let mut reader = BufReader::new(reader);
        let decoded = Self::read_header(&mut reader)?;
        decoded.decode_internal(&mut reader)
    }

    fn check_idat<R: Read>(
        &self,
        reader: &mut R,
        out: &mut Vec<u8>,
        length: usize,
    ) -> PictorResult<()> {
        let mut buffer = vec![0_u8; length + 4];
        reader.read_exact(&mut buffer)?;

        // Check crc
        let crc = u32::from_be_bytes(buffer[length..].try_into().unwrap());

        let crc_check = generate_crc(&[b"IDAT", &buffer[..length]].concat(), length);
        if crc != crc_check {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid crc".to_string(),
            });
        }

        out.try_reserve(length)
            .map_err(|_| PictorError::InvalidFormat {
                msg: "IDAT payload is too large".to_string(),
            })?;

        out.extend_from_slice(&buffer[..length]);

        Ok(())
    }

    fn check_trns(&self, seen_idat: bool) -> PictorResult<()> {
        if self.color_type == ColorType::La || self.color_type == ColorType::Rgba || seen_idat {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid TRNS chunk".to_string(),
            });
        }

        // TODO: Store and expand this into the alpha channel
        Ok(())
    }

    fn check_plte<R: Read>(
        &self,
        reader: &mut R,
        seen_idat: bool,
        length: usize,
    ) -> PictorResult<Vec<[u8; 3]>> {
        if seen_idat {
            return Err(PictorError::InvalidFormat {
                msg: "PLTE chunk found after IDAT".to_string(),
            });
        }

        if self.color_type == ColorType::L || self.color_type == ColorType::La {
            return Err(PictorError::InvalidFormat {
                msg: "PLTE chunk not allowed".to_string(),
            });
        }

        if length == 0 || !length.is_multiple_of(3) {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid chunk length.".to_string(),
            });
        }
        let mut buffer = vec![0_u8; length + 4];
        reader.read_exact(&mut buffer)?;

        let data: Vec<[u8; 3]> = buffer[..length]
            .chunks_exact(3)
            .map(|rgb| [rgb[0], rgb[1], rgb[2]])
            .collect();

        if data.len() > 256 {
            return Err(PictorError::InvalidFormat {
                msg: "PLTE max length is 256 entries".to_string(),
            });
        }

        // Check crc
        let crc = u32::from_be_bytes(buffer[length..].try_into().unwrap());

        let crc_check = generate_crc(&[b"PLTE", &buffer[..length]].concat(), length);
        if crc != crc_check {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid crc".to_string(),
            });
        }

        Ok(data)
    }

    fn skip_marker<R: Read>(&self, reader: &mut R, length: usize) -> PictorResult<()> {
        let mut buffer = vec![0_u8; length + 4];
        reader.read_exact(&mut buffer)?;
        Ok(())
    }

    fn check_iend<R: Read>(
        &self,
        reader: &mut R,
        seen_idat: bool,
        length: usize,
    ) -> PictorResult<()> {
        if !seen_idat {
            return Err(PictorError::InvalidFormat {
                msg: "No data. Corrupt PNG.".to_string(),
            });
        }

        let buffer_len = 4;
        let mut buffer = vec![0_u8; buffer_len];

        reader.read_exact(&mut buffer)?;
        if length != 0 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid chunk length.".to_string(),
            });
        }

        // Check crc
        let crc = u32::from_be_bytes(buffer[..4].try_into().unwrap());

        let crc_check = generate_crc(b"IEND", 0);
        if crc != crc_check {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid crc".to_string(),
            });
        }

        Ok(())
    }

    pub(crate) fn read_header<R: Read>(reader: &mut R) -> PictorResult<Self> {
        // PNG signature + IHDR length field + IHDR tag + IHDR length
        let header_len = 8 + 4 + 4 + 13;
        let buffer_len = header_len + 4; // header + crc
        let mut buffer = vec![0_u8; buffer_len];

        reader.read_exact(&mut buffer)?;

        // Check Signature
        if PNG_SIG != buffer[..8] {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid signature".to_string(),
            });
        }

        let idhr_length = u32::from_be_bytes(buffer[8..12].try_into().unwrap());
        if idhr_length != 13 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid chunk length".to_string(),
            });
        }

        if *b"IHDR" != buffer[12..16] {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid Chunk tag".to_string(),
            });
        }

        let width = u32::from_be_bytes(buffer[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(buffer[20..24].try_into().unwrap());
        let bit_depth: BitDepth = buffer[24].try_into()?;
        let color_type: ColorType = buffer[25].try_into()?;

        if buffer[26] != 0 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid compression method".to_string(),
            });
        };
        if buffer[27] != 0 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid filter type".to_string(),
            });
        };
        if buffer[28] != 0 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid interlace method".to_string(),
            });
        }

        // Check crc
        let crc = u32::from_be_bytes(buffer[29..33].try_into().unwrap());

        let crc_check = generate_crc(&buffer[..29], 13);
        if crc != crc_check {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid crc".to_string(),
            });
        }

        let stride =
            width * color_type.comp_per_pix() as u32 * bit_depth.bytes_per_comp() as u32 + 1;

        Ok(PngDecodeRequest {
            width,
            height,
            bit_depth,
            color_type,
            stride: stride as usize,
        })
    }

    pub fn inflate(data: &[u8]) -> PictorResult<Vec<u8>> {
        use flate2::read::ZlibDecoder;
        use std::io::Read;

        let mut decoder = ZlibDecoder::new(data);
        let mut out = Vec::new();

        decoder.read_to_end(&mut out)?;

        Ok(out)
    }
}

pub struct DecodedPng<'a> {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: BitDepth,
    pub pallete: Option<Vec<[u8; 3]>>,
    pub data: SampleStorage<'a, u8>,
    // pub data: Vec<u8>, // Raw bytes
}

impl<'a> DecodedPng<'a> {
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn color_type(&self) -> ColorType {
        self.color_type
    }

    #[inline]
    pub fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }

    pub fn pallete(&self) -> &Option<Vec<[u8; 3]>> {
        &self.pallete
    }

    pub fn to_u8(&self) {
        let _ = self.data;
    }

    pub fn to_u16(&self) {}
}
