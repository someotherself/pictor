use std::{
    fs::OpenOptions,
    io::{BufReader, Read},
    path::Path,
};

use pictor_core::{
    PictorError, PictorResult,
    codecs::{
        color_type::{BitDepth, ColorType},
        // jpeg::JPEG_ZIGZAG,
    },
    samples::SampleStorage,
};

const JPEG_ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

#[derive(Default)]
pub struct JpegDecodeRequest {
    width: u16,
    height: u16,
    color_type: ColorType,
    components: Vec<JpegComponent>,
    quantization_tables: Box<[Option<[u16; 64]>; 4]>,
    dc_huffman_tables: Box<[Option<JpegHuffmanTable>; 4]>,
    ac_huffman_tables: Box<[Option<JpegHuffmanTable>; 4]>,
    scan_components: Vec<JpegScanComponent>,
}

// The Y, Cb and Cr components
struct JpegComponent {
    id: u8,
    horizontal_sampling: u8,
    vertical_sampling: u8,
    quantization_table_id: u8,
}

pub(crate) struct JpegHuffmanTable {
    /// Number of codes of lengths 1 through 16. (Li table)
    pub(crate) code_counts: [u8; 16],
    /// Symbols ordered by increasing code length, as stored in the DHT segment. (Vi table)
    pub(crate) symbols: Vec<u8>,
}

pub(crate) struct JpegScanComponent {
    pub(crate) component_id: u8,
    pub(crate) dc_table_id: u8,
    pub(crate) ac_table_id: u8,
}

impl JpegDecodeRequest {
    #[inline]
    pub fn width(&self) -> u16 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u16 {
        self.height
    }

    #[inline]
    pub fn color_type(&self) -> ColorType {
        self.color_type
    }

    pub fn decode<'a, P: AsRef<Path>>(path: P) -> PictorResult<DecodedJpeg<'a>> {
        let file = OpenOptions::new().read(true).open(path)?;
        Self::decode_with(file)
    }

    pub(crate) fn decode_internal<'a, R: Read>(
        &self,
        reader: &mut R,
    ) -> PictorResult<DecodedJpeg<'a>> {
        let mut bit_reader = JpegBitReader::new(reader);

        let mut planes = ComponentPlanes::new(self)?;
        let mut dc_predictors = vec![0i32; self.components.len()];

        for mcu_y in 0..planes.mcu_rows {
            for mcu_x in 0..planes.mcu_columns {
                for scan in &self.scan_components {
                    let component_index = self
                        .components
                        .iter()
                        .position(|component| component.id == scan.component_id)
                        .ok_or_else(|| PictorError::InvalidFormat {
                            msg: "SOS references an unknown JPEG component".to_string(),
                        })?;

                    let component = &self.components[component_index];

                    let dc_table = self
                        .dc_huffman_tables
                        .get(usize::from(scan.dc_table_id))
                        .and_then(Option::as_ref)
                        .ok_or_else(|| PictorError::InvalidFormat {
                            msg: "Missing JPEG DC Huffman table".to_string(),
                        })?;

                    let ac_table = self
                        .ac_huffman_tables
                        .get(usize::from(scan.ac_table_id))
                        .and_then(Option::as_ref)
                        .ok_or_else(|| PictorError::InvalidFormat {
                            msg: "Missing JPEG AC Huffman table".to_string(),
                        })?;

                    let quantization_table = self
                        .quantization_tables
                        .get(usize::from(component.quantization_table_id))
                        .and_then(Option::as_ref)
                        .ok_or_else(|| PictorError::InvalidFormat {
                            msg: "Missing JPEG quantization table".to_string(),
                        })?;

                    for block_y in 0..usize::from(component.vertical_sampling) {
                        for block_x in 0..usize::from(component.horizontal_sampling) {
                            let block = bit_reader.decode_block(
                                dc_table,
                                ac_table,
                                quantization_table,
                                &mut dc_predictors[component_index],
                            )?;

                            let samples = Self::idct_block(&block);

                            let destination_x =
                                mcu_x * usize::from(component.horizontal_sampling) + block_x;
                            let destination_y =
                                mcu_y * usize::from(component.vertical_sampling) + block_y;

                            planes.planes[component_index].write_block(
                                destination_x,
                                destination_y,
                                &samples,
                            );
                        }
                    }
                }
            }
        }

        let width = usize::from(self.width);
        let height = usize::from(self.height);

        let output = if self.components.len() == 1 {
            let plane = &planes.planes[0];
            let mut output = Vec::with_capacity(width * height);

            for y in 0..height {
                let row_start = y * plane.width;
                output.extend_from_slice(&plane.samples[row_start..row_start + width]);
            }

            output
        } else {
            let mut output = Vec::with_capacity(width * height * 3);

            for y in 0..height {
                for x in 0..width {
                    let luma = Self::sample_component(
                        &planes.planes[0],
                        x,
                        y,
                        self.components[0].horizontal_sampling,
                        self.components[0].vertical_sampling,
                        planes.max_h,
                        planes.max_v,
                    );
                    let cb = Self::sample_component(
                        &planes.planes[1],
                        x,
                        y,
                        self.components[1].horizontal_sampling,
                        self.components[1].vertical_sampling,
                        planes.max_h,
                        planes.max_v,
                    );
                    let cr = Self::sample_component(
                        &planes.planes[2],
                        x,
                        y,
                        self.components[2].horizontal_sampling,
                        self.components[2].vertical_sampling,
                        planes.max_h,
                        planes.max_v,
                    );
                    let [r, g, b] = Self::ycbcr_to_rgb(luma, cb, cr);
                    output.extend_from_slice(&[r, g, b]);
                }
            }

            output
        };

        Ok(DecodedJpeg {
            width: u32::from(self.width),
            height: u32::from(self.height),
            color_type: self.color_type,
            bit_depth: BitDepth::U8,
            data: SampleStorage::Owned { data: output },
        })
    }

    pub fn decode_with<'a, R: Read>(reader: R) -> PictorResult<DecodedJpeg<'a>> {
        let mut reader = BufReader::new(reader);
        let decoder = Self::read_header(&mut reader)?;
        decoder.decode_internal(&mut reader)
    }

    pub(crate) fn read_header<R: Read>(reader: &mut R) -> PictorResult<Self> {
        let mut decoder = JpegReader::new(reader);
        // Start of the jpeg header. Always first.
        decoder.expect_marker(Marker::SOI)?;

        let mut request = JpegDecodeRequest::default();

        loop {
            let marker = decoder.read_marker()?;

            match marker {
                Marker::DQT => {
                    decoder.read_dqt(&mut request)?;
                }
                Marker::SOF0 => {
                    decoder.read_sof0(&mut request)?;
                }
                Marker::DHT => {
                    decoder.read_dht(&mut request)?;
                }
                Marker::SOS => {
                    decoder.read_sos(&mut request)?;
                    // This is the end of the header. Validate what we've read so far,
                    // since some of the fields in JpegDeodeRequest are Options.
                    decoder.validate(&request)?;
                    return Ok(request);
                }
                marker if marker.is_skippable_segment() => {
                    decoder.skip_segment()?;
                }
                marker if marker.is_sof() => {
                    // SOF1, SOF2.. etc not supported for now.
                    // To be implemented in a later release
                    return Err(PictorError::InvalidFormat {
                        msg: format!("Unsupported JPEG frame marker: {marker:?}"),
                    });
                }
                _ => {
                    return Err(PictorError::InvalidFormat {
                        msg: format!("Unexpected marker FF{:X?}", marker),
                    });
                }
            }
        }
    }

    fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
        let y = f32::from(y);
        let cb = f32::from(cb) - 128.0;
        let cr = f32::from(cr) - 128.0;

        [
            Self::clamp_u8(y + 1.40200 * cr),
            Self::clamp_u8(y - 0.34414 * cb - 0.71414 * cr),
            Self::clamp_u8(y + 1.77200 * cb),
        ]
    }

    fn clamp_u8(value: f32) -> u8 {
        value.round().clamp(0.0, 255.0) as u8
    }

    fn sample_component(
        plane: &ComponentPlane,
        x: usize,
        y: usize,
        h: u8,
        v: u8,
        max_h: u8,
        max_v: u8,
    ) -> u8 {
        let source_x = x.saturating_mul(h as usize) / max_h as usize;

        let source_y = y.saturating_mul(v as usize) / max_v as usize;

        let source_x = source_x.min(plane.width.saturating_sub(1));
        let source_y = source_y.min(plane.height.saturating_sub(1));

        plane.samples[source_y * plane.width + source_x]
    }

    fn idct_block(coefficients: &[i32; 64]) -> [u8; 64] {
        let mut output = [0u8; 64];
        let pi = std::f32::consts::PI;

        for y in 0..8 {
            for x in 0..8 {
                let mut value = 0.0f32;

                for v in 0..8 {
                    for u in 0..8 {
                        let coefficient = coefficients[v * 8 + u] as f32;

                        let cu = if u == 0 { 1.0 / 2.0f32.sqrt() } else { 1.0 };

                        let cv = if v == 0 { 1.0 / 2.0f32.sqrt() } else { 1.0 };

                        let x_angle = ((2 * x + 1) * u) as f32 * pi / 16.0;
                        let y_angle = ((2 * y + 1) * v) as f32 * pi / 16.0;

                        value += cu * cv * coefficient * x_angle.cos() * y_angle.cos();
                    }
                }

                let value = (value / 4.0 + 128.0).round();
                output[y * 8 + x] = value.clamp(0.0, 255.0) as u8;
            }
        }

        output
    }
}

struct JpegBitReader<'a, R: Read> {
    reader: &'a mut R,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a, R: Read> JpegBitReader<'a, R> {
    fn new(reader: &'a mut R) -> Self {
        Self {
            reader,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    fn read_bit(&mut self) -> PictorResult<u8> {
        if self.bit_count == 0 {
            self.fill()?;
        }

        self.bit_count -= 1;
        Ok(((self.bit_buf >> self.bit_count) & 1) as u8)
    }

    fn read_bits(&mut self, count: u8) -> PictorResult<u16> {
        debug_assert!(count <= 16);

        let mut value = 0u16;

        for _ in 0..count {
            value = (value << 1) | u16::from(self.read_bit()?);
        }

        Ok(value)
    }

    fn fill(&mut self) -> PictorResult<()> {
        self.bit_buf = u32::from(self.read_entropy_byte()?);
        self.bit_count = 8;
        Ok(())
    }

    fn receive_extend(&mut self, size: u8) -> PictorResult<i32> {
        if size == 0 {
            return Ok(0);
        }

        let bits = self.read_bits(size)? as i32;
        let threshold = 1_i32 << (size - 1);

        if bits < threshold {
            Ok(bits + 1 - (1_i32 << size))
        } else {
            Ok(bits)
        }
    }

    fn decode_block(
        &mut self,
        dc_table: &JpegHuffmanTable,
        ac_table: &JpegHuffmanTable,
        quantization_table: &[u16; 64],
        previous_dc: &mut i32,
    ) -> PictorResult<[i32; 64]> {
        let mut coefficients = [0i32; 64];

        let dc_size = self.decode_huffman_symbol(dc_table)?;

        if dc_size > 11 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid JPEG DC coefficient size".to_string(),
            });
        }

        let dc_delta = self.receive_extend(dc_size)?;
        *previous_dc =
            previous_dc
                .checked_add(dc_delta)
                .ok_or_else(|| PictorError::InvalidFormat {
                    msg: "JPEG DC predictor overflow".to_string(),
                })?;

        coefficients[0] = *previous_dc * i32::from(quantization_table[0]);

        let mut zigzag_index = 1usize;

        while zigzag_index < 64 {
            let symbol = self.decode_huffman_symbol(ac_table)?;
            let run = usize::from(symbol >> 4);
            let size = symbol & 0x0F;

            if size == 0 {
                if run == 0 {
                    break;
                }

                if run == 15 {
                    zigzag_index += 16;

                    if zigzag_index > 64 {
                        return Err(PictorError::InvalidFormat {
                            msg: "Invalid JPEG AC run length".to_string(),
                        });
                    }

                    continue;
                }

                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG AC coefficient symbol".to_string(),
                });
            }

            zigzag_index =
                zigzag_index
                    .checked_add(run)
                    .ok_or_else(|| PictorError::InvalidFormat {
                        msg: "JPEG AC run length overflow".to_string(),
                    })?;

            if zigzag_index >= 64 {
                return Err(PictorError::InvalidFormat {
                    msg: "JPEG AC coefficient exceeds block boundary".to_string(),
                });
            }

            if size > 10 {
                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG AC coefficient size".to_string(),
                });
            }

            let value = self.receive_extend(size)?;
            let natural_index = JPEG_ZIGZAG[zigzag_index];

            coefficients[natural_index] = value * i32::from(quantization_table[natural_index]);

            zigzag_index += 1;
        }

        Ok(coefficients)
    }

    fn decode_huffman_symbol(&mut self, table: &JpegHuffmanTable) -> PictorResult<u8> {
        let mut code = 0u16;
        let mut symbol_offset = 0usize;

        for code_length in 1..=16 {
            code = (code << 1) | u16::from(self.read_bit()?);

            let count = usize::from(table.code_counts[code_length - 1]);

            if count == 0 {
                continue;
            }

            let first_code = Self::first_code_for_length(&table.code_counts, code_length);
            let first_symbol = symbol_offset;

            if code >= first_code && usize::from(code - first_code) < count {
                let index = first_symbol + usize::from(code - first_code);

                return table.symbols.get(index).copied().ok_or_else(|| {
                    PictorError::InvalidFormat {
                        msg: "JPEG Huffman symbol index is out of bounds".to_string(),
                    }
                });
            }

            symbol_offset += count;
        }

        Err(PictorError::InvalidFormat {
            msg: "Invalid JPEG Huffman code".to_string(),
        })
    }

    fn first_code_for_length(counts: &[u8; 16], length: usize) -> u16 {
        let mut code = 0u16;

        for current_length in 1..length {
            code = (code + u16::from(counts[current_length - 1])) << 1;
        }

        code
    }

    fn read_entropy_byte(&mut self) -> PictorResult<u8> {
        let mut byte = [0u8; 1];
        self.reader.read_exact(&mut byte)?;

        if byte[0] != 0xFF {
            return Ok(byte[0]);
        }

        let mut next = [0u8; 1];
        self.reader.read_exact(&mut next)?;

        match next[0] {
            0x00 => Ok(0xFF),
            0xD9 => Err(PictorError::InvalidArgument {
                msg: "Unexpected EOI",
            }),
            0xD0..=0xD7 => Err(PictorError::InvalidFormat {
                msg: "JPEG restart markers are not supported".into(),
            }),
            marker => Err(PictorError::InvalidFormat {
                msg: format!("unexpected marker in JPEG entropy data: FF{marker:02X}"),
            }),
        }
    }
}

struct ComponentPlane {
    width: usize,
    height: usize,
    samples: Vec<u8>,
}

impl ComponentPlane {
    fn write_block(&mut self, block_x: usize, block_y: usize, samples: &[u8; 64]) {
        let origin_x = block_x * 8;
        let origin_y = block_y * 8;

        for y in 0..8 {
            for x in 0..8 {
                let dst_x = origin_x + x;
                let dst_y = origin_y + y;

                if dst_x < self.width && dst_y < self.height {
                    self.samples[dst_y * self.width + dst_x] = samples[y * 8 + x];
                }
            }
        }
    }
}

struct ComponentPlanes {
    planes: Vec<ComponentPlane>,
    max_h: u8,
    max_v: u8,
    mcu_columns: usize,
    mcu_rows: usize,
}

impl ComponentPlanes {
    fn new(request: &JpegDecodeRequest) -> PictorResult<Self> {
        let max_h = request
            .components
            .iter()
            .map(|component| component.horizontal_sampling)
            .max()
            .ok_or_else(|| PictorError::InvalidFormat {
                msg: "JPEG contains no components".to_string(),
            })?;

        let max_v = request
            .components
            .iter()
            .map(|component| component.vertical_sampling)
            .max()
            .ok_or_else(|| PictorError::InvalidFormat {
                msg: "JPEG contains no components".to_string(),
            })?;

        let mcu_width = 8 * (max_h as usize);
        let mcu_height = 8 * (max_v as usize);

        let image_width = request.width as usize;
        let image_height = request.height as usize;

        let mcu_columns = image_width.div_ceil(mcu_width);
        let mcu_rows = image_height.div_ceil(mcu_height);

        let planes = request
            .components
            .iter()
            .map(|component| {
                let width = mcu_columns * 8 * usize::from(component.horizontal_sampling);

                let height = mcu_rows * 8 * usize::from(component.vertical_sampling);

                ComponentPlane {
                    width,
                    height,
                    samples: vec![0; width * height],
                }
            })
            .collect();

        Ok(Self {
            planes,
            max_h,
            max_v,
            mcu_columns,
            mcu_rows,
        })
    }
}

pub struct DecodedJpeg<'a> {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: BitDepth,
    pub data: SampleStorage<'a, u8>,
}

impl<'a> DecodedJpeg<'a> {
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
        BitDepth::U8
    }
}

pub(crate) struct JpegReader<'a, R: Read> {
    reader: &'a mut R,
}

impl<'a, R: Read> JpegReader<'a, R> {
    pub(crate) fn new(reader: &'a mut R) -> Self {
        Self { reader }
    }

    pub(crate) fn read_u8(&mut self) -> PictorResult<u8> {
        let mut buffer = [0_u8; 1];
        self.reader.read_exact(&mut buffer)?;

        Ok(buffer[0])
    }

    pub(crate) fn read_u16_be(&mut self) -> PictorResult<u16> {
        let mut buffer = [0_u8; 2];
        self.reader.read_exact(&mut buffer)?;

        Ok(u16::from_be_bytes(buffer))
    }

    pub(crate) fn read_marker(&mut self) -> PictorResult<Marker> {
        let marker = self.read_u8()?;
        if marker != 0xFF {
            return Err(PictorError::InvalidFormat {
                msg: format!("Unexpected JPEG marker: {:X?}", marker),
            });
        }

        let byte = self.read_u8()?;

        if byte == 0xFF {
            return Err(PictorError::InvalidFormat {
                msg: format!("Unexpected JPEG marker: {:X?}", marker),
            });
        }

        Ok(Marker::from_byte(byte))
    }

    pub(crate) fn expect_marker(&mut self, expected: Marker) -> PictorResult<()> {
        let prefix = self.read_u8()?;

        if prefix != 0xFF {
            return Err(PictorError::InvalidFormat {
                msg: "Expected JPEG marker".to_string(),
            });
        }

        let marker = Marker::from_byte(self.read_u8()?);

        if marker != expected {
            return Err(PictorError::InvalidFormat {
                msg: format!("Expected JPEG marker {:02X}", expected.0),
            });
        }

        Ok(())
    }

    fn read_sof0(&mut self, request: &mut JpegDecodeRequest) -> PictorResult<()> {
        // SOF0 payload:
        //   length (already read by this method)
        //   sample precision: 1 byte
        //   image height:      2 bytes
        //   image width:       2 bytes
        //   component count:   1 byte
        //   component data:    3 bytes per component
        // Length - Lf
        // u16 - big endian
        let length = self.read_u16_be()?;

        if length < 8 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid JPEG SOF0 segment length".to_string(),
            });
        }

        // Sample precision (8 or 16 bits) - P - 1 byte
        let precision = self.read_u8()?;
        if precision != 8 {
            return Err(PictorError::InvalidFormat {
                msg: "Only 8-bit baseline JPEG images are supported".to_string(),
            });
        }

        // Height (Y) and wigth (X) in u16 big endiant
        let height = self.read_u16_be()?;
        let width = self.read_u16_be()?;
        if width == 0 || height == 0 {
            return Err(PictorError::InvalidFormat {
                msg: "JPEG image dimensions must be non-zero".to_string(),
            });
        }

        request.width = width;
        request.height = height;

        // Coumponent count - Nf (1 byte, 1 for greyscale, 3 for rgb)
        let component_count = self.read_u8()?;
        let expected_length = 8_u16
            .checked_add(u16::from(component_count) * 3)
            .ok_or_else(|| PictorError::InvalidFormat {
                msg: "Invalid JPEG SOF0 segment length".to_string(),
            })?;

        if length != expected_length {
            return Err(PictorError::InvalidFormat {
                msg: "JPEG SOF0 segment length does not match its component count".to_string(),
            });
        }

        let color_type = match component_count {
            1 => ColorType::L,
            3 => ColorType::Rgb,
            _ => {
                return Err(PictorError::InvalidFormat {
                    msg: "Baseline JPEG must have one or three components".to_string(),
                });
            }
        };

        request.color_type = color_type;

        // Consume each component specification. The IDs and quantization selectors
        // are needed by the entropy decoder, but are not part of this request yet.
        for _ in 0..component_count {
            // Comp identifier - Ci - 1 byte
            let component_id = self.read_u8()?;
            // H and V sampling factors (Hi / Vi)
            // With chroma subsampling: 0x22
            // Without chroma subsampling: 0x11
            let sampling_factors = self.read_u8()?;
            // Tqi
            let quantization_table_id = self.read_u8()?;
            // example -> [0x01, 0x11, 0x01] -> Table 1, no subsampling, quant table 1

            let horizontal = sampling_factors >> 4;
            let vertical = sampling_factors & 0x0F;
            if horizontal == 0 || vertical == 0 {
                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG component sampling factors".to_string(),
                });
            }

            request.components.push(JpegComponent {
                id: component_id,                // Ci
                horizontal_sampling: horizontal, // Tq, 1 or 2
                vertical_sampling: vertical,     // Tq, 1 or 2
                quantization_table_id,           // Tqi
            });
        }

        Ok(())
    }

    // The quantization tables. DQT -> X’FFDB’ (ITU-T T.81)
    fn read_dqt(&mut self, request: &mut JpegDecodeRequest) -> PictorResult<()> {
        // Length: Lq (includes the length field itself)
        // Big endian u16
        let segment_length = self.read_u16_be()?;

        if segment_length < 2 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid JPEG DQT segment length".to_string(),
            });
        }

        let mut remaining = usize::from(segment_length - 2);

        while remaining > 0 {
            // Table precision - Pq (8 or 18 bit)
            // Stored as 4 bits (most significant)
            let table_info = self.read_u8()?;
            remaining -= 1;

            let precision = table_info >> 4;

            // Table id - Tq
            // Stored as 4 bits (least significant)
            let table_id = usize::from(table_info & 0x0F);

            if precision > 1 {
                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG quantization-table precision".to_string(),
                });
            }

            if table_id >= 4 {
                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG quantization-table identifier".to_string(),
                });
            }

            let bytes_per_value = if precision == 0 { 1 } else { 2 };
            let table_size = 64 * bytes_per_value;

            if remaining < table_size {
                return Err(PictorError::InvalidFormat {
                    msg: "Truncated JPEG quantization table".to_string(),
                });
            }

            // The tables are built and stored in zig zag order
            // We need to reverse this
            let mut zigzag_table = [0_u16; 64];

            for value in &mut zigzag_table {
                *value = if precision == 0 {
                    u16::from(self.read_u8()?)
                } else {
                    self.read_u16_be()?
                };
            }

            remaining -= table_size;

            // JPEG stores DQT values in zigzag order. Convert them to
            // natural row-major 8x8 order for the IDCT code.
            let mut natural_table = [0_u16; 64];

            for (zigzag_index, &natural_index) in JPEG_ZIGZAG.iter().enumerate() {
                natural_table[natural_index] = zigzag_table[zigzag_index];
            }

            request.quantization_tables[table_id] = Some(natural_table);
        }
        Ok(())
    }

    fn read_dht(&mut self, request: &mut JpegDecodeRequest) -> PictorResult<()> {
        // Length: Lh (includes the length field itself)
        // Big endian u16
        let segment_length = self.read_u16_be()?;

        if segment_length < 2 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid JPEG DHT segment length".to_string(),
            });
        }

        let mut remaining = usize::from(segment_length - 2);

        while remaining > 0 {
            // One table specification byte is required.
            if remaining < 17 {
                return Err(PictorError::InvalidFormat {
                    msg: "Truncated JPEG Huffman table".to_string(),
                });
            }

            // Table class (Tc) 4 bits most significant (0 = DC / 1 = AC)
            // Dest identifier (Th) 4 bits least significant
            let table_info = self.read_u8()?;
            remaining -= 1;

            let table_class = table_info >> 4;
            let table_id = usize::from(table_info & 0x0F);

            if table_class > 1 {
                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG Huffman table class".to_string(),
                });
            }

            if table_id >= 4 {
                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG Huffman table identifier".to_string(),
                });
            }

            // Huffman codes table - Li
            let mut code_counts = [0_u8; 16];
            let mut symbol_count = 0_usize;

            for count in &mut code_counts {
                // Populate the Li table
                *count = self.read_u8()?;
                symbol_count = symbol_count.checked_add(*count as usize).ok_or_else(|| {
                    PictorError::InvalidFormat {
                        msg: "Invalid JPEG Huffman symbol count".to_string(),
                    }
                })?;
            }
            // Finished reading the Li table (16 values)

            remaining -= 16;

            if symbol_count > 256 {
                return Err(PictorError::InvalidFormat {
                    msg: "JPEG Huffman table contains too many symbols".to_string(),
                });
            }

            if remaining < symbol_count {
                return Err(PictorError::InvalidFormat {
                    msg: "Truncated JPEG Huffman table symbols".to_string(),
                });
            }

            // Vi: Huffman symbols associated with the codes
            // ordered by increasing code length.
            // The sum of all elements in the Li table,
            // is the count of elements in the Vi table
            let mut symbols = Vec::with_capacity(symbol_count);

            // Populate Vi table
            for _ in 0..symbol_count {
                symbols.push(self.read_u8()?);
            }

            remaining -= symbol_count;

            let table = JpegHuffmanTable {
                code_counts, // Li
                symbols,     // Vi
            };

            let destination = if table_class == 0 {
                &mut request.dc_huffman_tables[table_id]
            } else {
                &mut request.ac_huffman_tables[table_id]
            };

            if destination.is_some() {
                return Err(PictorError::InvalidFormat {
                    msg: "Duplicate JPEG Huffman table".to_string(),
                });
            }

            *destination = Some(table);
        }

        Ok(())
    }

    fn read_sos(&mut self, request: &mut JpegDecodeRequest) -> PictorResult<()> {
        // The length includes the two length bytes.
        let segment_length = self.read_u16_be()?;

        if segment_length < 6 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid JPEG SOS segment length".to_string(),
            });
        }

        // Image components - Ns
        let scan_component_count = self.read_u8()?;

        if scan_component_count == 0 {
            return Err(PictorError::InvalidFormat {
                msg: "JPEG SOS must contain at least one component".to_string(),
            });
        }

        // (Csj + (Tdj + Taj)) * Ss + Se + (Ah + Al)
        let expected_length = 6_u16
            .checked_add(u16::from(scan_component_count) * 2)
            .ok_or_else(|| PictorError::InvalidFormat {
                msg: "Invalid JPEG SOS segment length".to_string(),
            })?;

        if segment_length != expected_length {
            return Err(PictorError::InvalidFormat {
                msg: "JPEG SOS segment length does not match its component count".to_string(),
            });
        }

        if usize::from(scan_component_count) != request.components.len() {
            return Err(PictorError::InvalidFormat {
                msg: "Only non-interleaved scans matching all SOF0 components are supported"
                    .to_string(),
            });
        }

        request.scan_components.clear();

        for _ in 0..scan_component_count {
            // Csj
            let component_id = self.read_u8()?;
            // DC / AC entropy table destination id: Tdj / Taj (4 bits each)
            let table_selectors = self.read_u8()?;

            let dc_table_id = table_selectors >> 4;
            let ac_table_id = table_selectors & 0x0F;

            if dc_table_id >= 4 || ac_table_id >= 4 {
                return Err(PictorError::InvalidFormat {
                    msg: "Invalid JPEG Huffman table identifier in SOS".to_string(),
                });
            }

            // Make sure the table id actually exists and is not a duplicate
            if !request
                .components
                .iter()
                .any(|component| component.id == component_id)
            {
                return Err(PictorError::InvalidFormat {
                    msg: "SOS references an unknown JPEG component".to_string(),
                });
            }

            if request
                .scan_components
                .iter()
                .any(|component| component.component_id == component_id)
            {
                return Err(PictorError::InvalidFormat {
                    msg: "JPEG SOS contains a duplicate component".to_string(),
                });
            }

            if request.dc_huffman_tables[usize::from(dc_table_id)].is_none() {
                return Err(PictorError::InvalidFormat {
                    msg: "SOS references a missing DC Huffman table".to_string(),
                });
            }

            if request.ac_huffman_tables[usize::from(ac_table_id)].is_none() {
                return Err(PictorError::InvalidFormat {
                    msg: "SOS references a missing AC Huffman table".to_string(),
                });
            }

            request.scan_components.push(JpegScanComponent {
                component_id,
                dc_table_id,
                ac_table_id,
            });
        }

        // Spectral selection start: Ss
        let spectral_start = self.read_u8()?;
        // Spectral selection end: Se
        let spectral_end = self.read_u8()?;
        // Successive aprox bit pos high / low: Ah / Al (4 bits each)
        let successive = self.read_u8()?;

        if spectral_start != 0 || spectral_end != 63 || successive != 0 {
            return Err(PictorError::InvalidFormat {
                msg: "Only baseline sequential JPEG scans are supported".to_string(),
            });
        }

        Ok(())
    }

    fn validate(&self, request: &JpegDecodeRequest) -> PictorResult<()> {
        if request.components.is_empty() {
            return Err(PictorError::InvalidFormat {
                msg: "JPEG SOF0 contains no components".to_string(),
            });
        }

        for component in &request.components {
            let table_id = usize::from(component.quantization_table_id);

            let Some(table) = request.quantization_tables.get(table_id) else {
                return Err(PictorError::InvalidFormat {
                    msg: "JPEG component references an invalid quantization table".to_string(),
                });
            };

            if table.is_none() {
                return Err(PictorError::InvalidFormat {
                    msg: "JPEG component references a missing quantization table".to_string(),
                });
            }
        }

        Ok(())
    }

    fn skip_segment(&mut self) -> PictorResult<()> {
        let length = self.read_u16_be()?;

        if length < 2 {
            return Err(PictorError::InvalidFormat {
                msg: "Invalid JPEG segment length".to_string(),
            });
        }

        let mut remaining = usize::from(length - 2);
        let mut buffer = [0_u8; 256];

        while remaining > 0 {
            let chunk_size = remaining.min(buffer.len());
            self.reader.read_exact(&mut buffer[..chunk_size])?;
            remaining -= chunk_size;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Marker(u8);

impl Marker {
    pub(crate) const SOI: Self = Self(0xD8);
    // pub(crate) const EOI: Self = Self(0xD9);
    pub(crate) const SOF0: Self = Self(0xC0);
    pub(crate) const DHT: Self = Self(0xC4);
    pub(crate) const DQT: Self = Self(0xDB);
    pub(crate) const SOS: Self = Self(0xDA);
    pub(crate) const COM: Self = Self(0xFE);
}

impl Marker {
    pub(crate) fn from_byte(byte: u8) -> Self {
        Self(byte)
    }

    pub(crate) fn is_sof(self) -> bool {
        matches!(
            self.0,
            0xC0..=0xC3 |
            0xC5..=0xC7 |
            0xC9..=0xCB |
            0xCD..=0xCF
        )
    }

    pub(crate) fn is_app(self) -> bool {
        matches!(self.0, 0xE0..=0xEF)
    }

    pub(crate) fn is_skippable_segment(self) -> bool {
        self.is_app() || self == Marker::COM
    }
}
