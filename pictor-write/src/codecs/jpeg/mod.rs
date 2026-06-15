use std::{
    f32::consts,
    io::{BufWriter, Write},
    path::Path,
};

use pictor_core::{codecs::color_type::ColorType, PictorResult};
use tables::*;

mod tables;

/// Maps a natural 8x8 block index to JPEG zigzag order.
///
/// JPEG splits an image to 8x8 blocks and applies
/// Discrete Cosine Transform to each block.
///
/// Coefficients near the top left corner are low-freq image information
/// Coefficients neat the botton right corners are high frequesncy information
/// High frequency information prioritizes low frequency information and
/// stores high frequency information with less accuracy.
///
/// This table makes it easier to walk the image prioritizing the low frequency
/// information, leaving as many zeroes as possible grouped together (the high freq info).
const JPEG_ZIGZAG: [usize; 64] = [
    0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9, 11,
    18, 24, 31, 40, 44, 53, 10, 19, 23, 32, 39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60, 21, 34,
    37, 47, 50, 56, 59, 61, 35, 36, 48, 49, 57, 58, 62, 63,
];

#[allow(dead_code)]
pub struct EncodingRequest<'a> {
    width: u16,
    height: u16,
    color_type: ColorType,
    subsample: bool,
    quality_factor: u32,
    vertical_flip: bool,
    data: &'a [u8],
}

struct JpegTables {
    /// Quantization table written to the DQT marker for the Y/luma component.
    luma_quant: [u8; 64],
    /// Quantization table written to the DQT marker for the Cb/Cr chroma components.
    chroma_quant: [u8; 64],
    /// Internal multiplier table used after DCT for the Y/luma component.
    luma_quant_factors: [f32; 64],
    /// Internal multiplier table used after DCT for the Cb/Cr chroma components.
    chroma_quant_factors: [f32; 64],
}

impl<'a> EncodingRequest<'a> {
    pub fn encode<W: Write>(&self, dest: &mut W) -> PictorResult<()> {
        let quant_tables = self.build_quant_tables();

        let mut writer = BufWriter::new(dest);
        // Write the header
        self.init_soi_and_tables(&mut writer, &quant_tables)?;
        self.write_dct_huffman_tables(&mut writer)?;
        self.finish_header(&mut writer)?;

        let mut writer = JpegBitWriter::new(&mut writer);

        // Comment
        let mut dc_y = 0_i32;
        let mut dc_u = 0_i32;
        let mut dc_v = 0_i32;

        let comp = self.color_type.pixel_size() as usize;
        let mut y: usize = 0; // height
        let chunk_size: usize = if self.subsample { 16 } else { 8 };

        while y < self.height as usize {
            let mut x: usize = 0; // Width. Needs to be reset for each row
            while x < self.width as usize {
                let table_size = chunk_size * chunk_size;
                let mut y_table = Vec::new();
                let mut u_table = Vec::new();
                let mut v_table = Vec::new();
                y_table.resize_with(table_size, || 0.0);
                u_table.resize_with(table_size, || 0.0);
                v_table.resize_with(table_size, || 0.0);

                let mut row = y; // row in a JPEG block (16x16 or 8x8)
                let mut pos: usize = 0;
                while row < y + chunk_size {
                    let mut col = x; /* col in a JPEG block (16x16 or 8x8) */
                    // Handle partial blocks
                    let clamped_row = if row < self.height as usize {
                        row
                    } else {
                        // We are out of bounds
                        self.height as usize - 1
                    };
                    let base_p = if self.vertical_flip {
                        self.height as usize - 1 - clamped_row
                    } else {
                        clamped_row
                    };
                    let base_p = base_p * self.width as usize * comp;

                    while col < x + chunk_size {
                        let clamped_col = if col < self.width as usize {
                            col
                        } else {
                            self.width as usize - 1
                        };
                        let idx = base_p + clamped_col * comp;
                        let (r, g, b) = self.index_rbd_data(idx);
                        y_table[pos] = 0.29900 * r + 0.58700 * g + 0.11400 * b - 128.0;
                        u_table[pos] = -0.16874 * r - 0.33126 * g + 0.50000 * b;
                        v_table[pos] = 0.50000 * r - 0.41869 * g - 0.08131 * b;

                        pos += 1;
                        col += 1;
                    }
                    row += 1;
                }
                x += chunk_size;

                let mut sub_u = [0.0; 64];
                let mut sub_v = [0.0; 64];

                if self.subsample {
                    self.process_data_unit(
                        &mut writer,
                        &mut y_table, // top-left
                        16,
                        &quant_tables.luma_quant_factors,
                        &mut dc_y,
                        &YDC_HT,
                        &YAC_HT,
                    )?;
                    self.process_data_unit(
                        &mut writer,
                        &mut y_table[8..], // top-right
                        16,
                        &quant_tables.luma_quant_factors,
                        &mut dc_y,
                        &YDC_HT,
                        &YAC_HT,
                    )?;
                    self.process_data_unit(
                        &mut writer,
                        &mut y_table[128..], // bottom-left
                        16,
                        &quant_tables.luma_quant_factors,
                        &mut dc_y,
                        &YDC_HT,
                        &YAC_HT,
                    )?;
                    self.process_data_unit(
                        &mut writer,
                        &mut y_table[136..], // bottom-right
                        16,
                        &quant_tables.luma_quant_factors,
                        &mut dc_y,
                        &YDC_HT,
                        &YAC_HT,
                    )?;

                    for yy in 0..8 {
                        for xx in 0..8 {
                            let dst = yy * 8 + xx;
                            let src = yy * 32 + xx * 2;

                            sub_u[dst] = (u_table[src]
                                + u_table[src + 1]
                                + u_table[src + 16]
                                + u_table[src + 17])
                                * 0.25;

                            sub_v[dst] = (v_table[src]
                                + v_table[src + 1]
                                + v_table[src + 16]
                                + v_table[src + 17])
                                * 0.25;
                        }
                    }

                    self.process_data_unit(
                        &mut writer,
                        &mut sub_u,
                        8,
                        &quant_tables.chroma_quant_factors,
                        &mut dc_u,
                        &UVDC_HT,
                        &UVAC_HT,
                    )?;

                    self.process_data_unit(
                        &mut writer,
                        &mut sub_v,
                        8,
                        &quant_tables.chroma_quant_factors,
                        &mut dc_v,
                        &UVDC_HT,
                        &UVAC_HT,
                    )?;
                } else {
                    self.process_data_unit(
                        &mut writer,
                        &mut y_table,
                        8,
                        &quant_tables.luma_quant_factors,
                        &mut dc_y,
                        &YDC_HT,
                        &YAC_HT,
                    )?;
                    self.process_data_unit(
                        &mut writer,
                        &mut u_table,
                        8,
                        &quant_tables.chroma_quant_factors,
                        &mut dc_u,
                        &UVDC_HT,
                        &UVAC_HT,
                    )?;
                    self.process_data_unit(
                        &mut writer,
                        &mut v_table,
                        8,
                        &quant_tables.chroma_quant_factors,
                        &mut dc_v,
                        &UVDC_HT,
                        &UVAC_HT,
                    )?;
                }
            }
            y += chunk_size;
        }

        writer.finish_entropy_bits()?;

        let writer = writer.into_inner();

        // End of Image
        // EOI -> X'FFD9 (ITU-T T.81)
        writer.write_all(&[0xFF, 0xD9])?;
        writer.flush()?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn process_data_unit<W: Write>(
        &self,
        writer: &mut JpegBitWriter<'a, W>,
        block: &mut [f32],
        block_stride: usize,
        quant_scale: &[f32; 64],
        dc_coef: &mut i32,
        dc_huff: &[[u16; 2]; 12],
        ac_huff: &[[u16; 2]; 256],
    ) -> PictorResult<()> {
        // Run DCT
        for row in 0..8 {
            let offset = row * block_stride;
            let mut row_values = [
                block[offset],
                block[offset + 1],
                block[offset + 2],
                block[offset + 3],
                block[offset + 4],
                block[offset + 5],
                block[offset + 6],
                block[offset + 7],
            ];

            Self::dct_1d(&mut row_values);

            block[offset..offset + 8].copy_from_slice(&row_values);
        }

        for col in 0..8 {
            let mut column = [
                block[col],
                block[col + block_stride],
                block[col + 2 * block_stride],
                block[col + 3 * block_stride],
                block[col + 4 * block_stride],
                block[col + 5 * block_stride],
                block[col + 6 * block_stride],
                block[col + 7 * block_stride],
            ];

            Self::dct_1d(&mut column);

            for row in 0..8 {
                block[row * block_stride + col] = column[row];
            }
        }

        let mut du = [0_i32; 64];

        // Quantize + zig zag
        for row in 0..8 {
            for col in 0..8 {
                let j = row * 8 + col;
                let src = row * block_stride + col;

                let v = block[src] * quant_scale[j];

                du[JPEG_ZIGZAG[j]] = if v < 0.0 {
                    (v - 0.5) as i32
                } else {
                    (v + 0.5) as i32
                }
            }
        }

        // Encode DC
        let diff = du[0] - *dc_coef;

        if diff == 0 {
            writer.write_bits(dc_huff[0])?;
        } else {
            let bits = Self::calc_bits(diff);
            let category = bits[1] as usize;

            writer.write_bits(dc_huff[category])?;
            writer.write_bits(bits)?;
        }

        *dc_coef = du[0];

        // Encode AC
        let mut end0pos = 63;
        let eob = ac_huff[0x00];
        let zrl = ac_huff[0xF0];

        while end0pos > 0 && du[end0pos] == 0 {
            end0pos -= 1;
        }

        if end0pos == 0 {
            writer.write_bits(eob)?;
            return Ok(());
        }

        let mut i = 1;

        while i <= end0pos {
            let start = i;

            while i <= end0pos && du[i] == 0 {
                i += 1;
            }

            let mut zero_run = i - start;

            while zero_run >= 16 {
                writer.write_bits(zrl)?;
                zero_run -= 16;
            }

            let bits = Self::calc_bits(du[i]);
            let category = bits[1] as usize;
            let symbol = (zero_run << 4) + category;

            writer.write_bits(ac_huff[symbol])?;
            writer.write_bits(bits)?;

            i += 1;
        }

        if end0pos != 63 {
            writer.write_bits(eob)?;
        }

        Ok(())
    }

    fn calc_bits(value: i32) -> [u16; 2] {
        let mut absolute_value = if value < 0 { -value } else { value };
        let encoded_value = if value < 0 { value - 1 } else { value };

        let mut bitcount = 1_u16;

        while {
            absolute_value >>= 1;
            absolute_value != 0
        } {
            bitcount += 1;
        }

        let mask = (1_i32 << bitcount) - 1;
        let bits = encoded_value & mask;

        [bits as u16, bitcount]
    }

    // stbiw__jpg_DCT
    fn dct_1d(values: &mut [f32; 8]) {
        let mut d0 = values[0];
        let d1 = values[1];
        let mut d2 = values[2];
        let d3 = values[3];
        let mut d4 = values[4];
        let d5 = values[5];
        let mut d6 = values[6];
        let d7 = values[7];

        let tmp0 = d0 + d7;
        let tmp7 = d0 - d7;
        let tmp1 = d1 + d6;
        let tmp6 = d1 - d6;
        let tmp2 = d2 + d5;
        let tmp5 = d2 - d5;
        let tmp3 = d3 + d4;
        let tmp4 = d3 - d4;

        // Even part
        let mut tmp10 = tmp0 + tmp3; // phase 2
        let tmp13 = tmp0 - tmp3;
        let mut tmp11 = tmp1 + tmp2;
        let mut tmp12 = tmp1 - tmp2;

        d0 = tmp10 + tmp11; // phase 3
        d4 = tmp10 - tmp11;

        let z1 = (tmp12 + tmp13) * consts::FRAC_1_SQRT_2; // c4
        d2 = tmp13 + z1; // phase 5
        d6 = tmp13 - z1;

        // Odd part
        tmp10 = tmp4 + tmp5; // phase 2
        tmp11 = tmp5 + tmp6;
        tmp12 = tmp6 + tmp7;

        // The rotator is modified from fig 4-8 to avoid extra negations.
        let z5 = (tmp10 - tmp12) * 0.382_683_43; // c6
        let z2 = tmp10 * 0.541_196_1 + z5; // c2-c6
        let z4 = tmp12 * 1.306_563 + z5; // c2+c6
        let z3 = tmp11 * consts::FRAC_1_SQRT_2; // c4

        let z11 = tmp7 + z3; // phase 5
        let z13 = tmp7 - z3;

        values[5] = z13 + z2; // phase 6
        values[3] = z13 - z2;
        values[1] = z11 + z4;
        values[7] = z11 - z4;

        values[0] = d0;
        values[2] = d2;
        values[4] = d4;
        values[6] = d6;
    }

    fn index_rbd_data(&self, idx: usize) -> (f32, f32, f32) {
        match self.color_type.pixel_size() {
            1 | 2 => {
                let y = self.data[idx] as f32;
                (y, y, y)
            }
            3 | 4 => (
                self.data[idx] as f32,
                self.data[idx + 1] as f32,
                self.data[idx + 2] as f32,
            ),
            _ => unreachable!(),
        }
    }

    fn build_quant_tables(&self) -> JpegTables {
        let mut luma_quant_table = [0_u8; 64];
        let mut chroma_quant_table = [0_u8; 64];

        for coeff_idx in 0..64 {
            let luma_quant = (YQT[coeff_idx] * self.quality_factor + 50) / 100;
            luma_quant_table[JPEG_ZIGZAG[coeff_idx]] = if luma_quant < 1 {
                1
            } else if luma_quant > 255 {
                255
            } else {
                luma_quant as u8
            };

            let chroma_quant = (UVQT[coeff_idx] * self.quality_factor + 50) / 100;
            chroma_quant_table[JPEG_ZIGZAG[coeff_idx]] = if chroma_quant < 1 {
                1
            } else if chroma_quant > 255 {
                255
            } else {
                chroma_quant as u8
            };
        }

        // luma_forward_dct_quant_table
        let mut fdtbl_y = [0.0; 64];
        // chroma_forward_dct_quant_table
        let mut fdtbl_uv = [0.0; 64];

        let mut coeff_idx = 0;
        #[allow(clippy::needless_range_loop)]
        for dict_row in 0..8 {
            for dict_col in 0..8 {
                let zigzag_idx = JPEG_ZIGZAG[coeff_idx];

                fdtbl_y[coeff_idx] =
                    1.0 / (luma_quant_table[zigzag_idx] as f32 * AASF[dict_row] * AASF[dict_col]);
                fdtbl_uv[coeff_idx] =
                    1.0 / (chroma_quant_table[zigzag_idx] as f32 * AASF[dict_row] * AASF[dict_col]);
                coeff_idx += 1;
            }
        }

        JpegTables {
            luma_quant: luma_quant_table,
            chroma_quant: chroma_quant_table,
            luma_quant_factors: fdtbl_y,
            chroma_quant_factors: fdtbl_uv,
        }
    }

    // jpeg standard suck camel balls
    fn init_soi_and_tables<W: Write>(
        &self,
        writer: &mut BufWriter<W>,
        tables: &JpegTables,
    ) -> PictorResult<()> {
        // Start of Image. SOI -> X’FFD8’ (ITU-T T.81)
        writer.write_all(&[0xFF, 0xD8])?;

        // Application marker segment
        // APP0 -> X'FFE0 (ITU-T T.871 - APPn)
        writer.write_all(&[0xFF, 0xE0])?;
        // Length: Lp -> 0x0 0x10 (16 decimal, 4 bytes x 4 char) (ITU-T T.871)
        writer.write_all(&16_u16.to_be_bytes())?;
        // Identifier (payload). Null terminated JFIF string.
        writer.write_all(&[b'J', b'F', b'I', b'F', 0])?;
        // Version. 11 -> 1.01, 12 -> 1.02
        writer.write_all(&[1, 1])?;
        // Units.
        // 0 for unspecified
        // 1 for pixels per inch.
        // 2 for pixels per centimeter.
        writer.write_all(&0_u8.to_be_bytes())?;
        // Horizontal and vertical pixel density, big-endian u16 values.
        // With units = 0, this is a pixel aspect ratio. 1:1 means square pixels.
        writer.write_all(&1_u16.to_be_bytes())?; // Xdensity: 1
        writer.write_all(&1_u16.to_be_bytes())?; // Ydensity: 1
                                                 // Hoxizontal and vertical thumbnail pixel count. Can be zero
        writer.write_all(&0_u8.to_be_bytes())?; // X
        writer.write_all(&0_u8.to_be_bytes())?; // Y

        // Write the quantization tables. DQT -> X’FFDB’ (ITU-T T.81)
        writer.write_all(&[0xFF, 0xDB])?;
        // Length: Lq -> 0x0 0x84 (132 bytes, 64 * 2 tables + length fielt itself)
        // As big endian u16
        writer.write_all(&132_u16.to_be_bytes())?;
        // Quantization table element precision. Pq  - 4 bits (ITU-T T.81)
        // 0 for 8-bit values
        // 1 for 16-bit values
        // Quantization table destination identifier. Tq - 4 bits (ITU-T T.81)
        // Table id - 0
        writer.write_all(&0_u8.to_be_bytes())?;
        // Write the luma quant table
        writer.write_all(&tables.luma_quant)?;
        // Pq + Tq for the second table.
        // 0 for 8-bit precision + table id 1
        writer.write_all(&1_u8.to_be_bytes())?;
        // Write the chroma quant table
        writer.write_all(&tables.chroma_quant)?;
        Ok(())
    }

    fn write_dct_huffman_tables<W: Write>(&self, writer: &mut BufWriter<W>) -> PictorResult<()> {
        // Baseline DCT
        // SOF0 ->X’FFC0’ (ITU-T T.81)
        writer.write_all(&[0xFF, 0xC0])?;
        // Frame header length: Lf
        writer.write_all(&17_u16.to_be_bytes())?;
        // Sample precision: P
        // Number of bits for the samples in the pixels
        writer.write_all(&8_u8.to_be_bytes())?;
        // Height: Y
        writer.write_all(&self.height.to_be_bytes())?;
        // Width: X
        writer.write_all(&self.width.to_be_bytes())?;
        // Image components in frame: Nf (Different than the input ColorType)
        writer.write_all(&3_u8.to_be_bytes())?;
        // Component 1
        // Component identifier (id): Ci
        writer.write_all(&1_u8.to_be_bytes())?;
        // Horizontal and vertical sampling factors: Hi / Vi, 4 bits each.
        // With chroma subsampling: 0x22
        // Without chroma subsampling: 0x11
        let xy: u8 = if self.subsample { 0x22 } else { 0x11 };
        writer.write_all(&xy.to_be_bytes())?;
        // Quantization table selector. Tqi (Tq from DQT)
        // First is id 0 - luma quantization table
        // Component Y/luma uses quantization table id 0
        writer.write_all(&[0])?;
        // Component 2
        writer.write_all(&[0x02, 0x11, 0x01])?; // Ci, Hi/Vi, Tqi
                                                // Component 3
        writer.write_all(&[0x03, 0x11, 0x01])?; // Ci, Hi/Vi, Tqi

        // Define Huffman table
        // DHT -> X'FFC4' (ITU-T T.81)
        writer.write_all(&[0xFF, 0xC4])?;
        // Table length: Lh (0x01, 0xA2 -> 418 decimal)
        writer.write_all(&418_u16.to_be_bytes())?;

        // First destination (table)
        // Table class: Tc / Destination identifier: Th. 4 bits each.
        // 0 = DC / 1 = AC
        // DC + table id 0
        writer.write_all(&[0])?;
        // Huffman codes: Li (multiple codes, in an array)
        writer.write_all(&DC_LUMIN_NRCODES[1..])?;
        // Huffman values: Vi,j (multiple values, in an array)
        writer.write_all(&DC_LUMIN_VALUES)?;

        // Second destination (table)
        writer.write_all(&[0x10])?; // AC + table 0
        writer.write_all(&AC_LUMIN_NRCODES[1..])?;
        writer.write_all(&AC_LUMIN_VALUES)?;

        // Third destination (table)
        writer.write_all(&[0x01])?; // DC + table 1
        writer.write_all(&DC_CHROMIN_NRCODES[1..])?;
        writer.write_all(&DC_CHROMIN_VALUES)?;

        // Fourth destination (table)
        writer.write_all(&[0x11])?; // AC + table 1
        writer.write_all(&AC_CHROMIN_NRCODES[1..])?;
        writer.write_all(&AC_CHROMIN_VALUES)?;

        Ok(())
    }

    fn finish_header<W: Write>(&self, writer: &mut BufWriter<W>) -> PictorResult<()> {
        // Start of Scan: SOS -> X’FFDA’ (ITU-T T.81)
        writer.write_all(&[0xFF, 0xDA])?;
        // Scan header length: Ls. 0x0 0xC (12 decimal)
        writer.write_all(&12_u16.to_be_bytes())?;
        // Image components in frame: Ns (Different than the input ColorType)
        writer.write_all(&3_u8.to_be_bytes())?;
        // Component 1 - Y (luma / luminance-like brightness component)
        // Scan component selector (component id): Csj
        writer.write_all(&1_u8.to_be_bytes())?;
        // DC / AC entropy table destination id: Tdj / Taj (4 bits each)
        writer.write_all(&[0])?; // DC table 0, AC table 0
                                 // Component 2 - Cb (blue-difference chroma component)
        writer.write_all(&2_u8.to_be_bytes())?;
        writer.write_all(&[0x11])?; // DC table 1, AC table 1
                                    // Component 3 - Cr (red-difference chroma component)
        writer.write_all(&3_u8.to_be_bytes())?;
        writer.write_all(&[0x11])?; // DC table 1, AC table 1
                                    // Spectral selection start: Ss
        writer.write_all(&[0])?;
        // Spectral selection end: Se
        writer.write_all(&[0x3F])?;
        // Successive aprox bit pos high / low: Ah / Al (4 bits each)
        writer.write_all(&[0])?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct HuffCode {
    bits: u16,
    len: u8,
}

struct JpegBitWriter<'a, W: Write> {
    writer: &'a mut W,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a, W: Write> JpegBitWriter<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    fn write_bits(&mut self, bits: [u16; 2]) -> PictorResult<()> {
        let code = bits[0] as u32;
        let size = bits[1] as u8;

        if size == 0 {
            return Ok(());
        }

        self.bit_count += size;

        // Jpeg write bits MSB-first.
        self.bit_buf |= code << (24 - self.bit_count);

        while self.bit_count >= 8 {
            let byte = ((self.bit_buf >> 16) & 0xFF) as u8;

            self.writer.write_all(&[byte])?;

            // Jpeg byte stuffing
            if byte == 0xFF {
                self.writer.write_all(&[0x00])?;
            }

            self.bit_buf <<= 8;
            self.bit_count -= 8;
        }
        Ok(())
    }

    fn finish_entropy_bits(&mut self) -> PictorResult<()> {
        // stb writes{ 0x7F, 7 } before EOI.
        // This pads the remaining partial byte with 1 bit.
        self.write_bits([0x7F, 7])
    }

    fn into_inner(self) -> &'a mut W {
        self.writer
    }
}

#[allow(dead_code)]
pub struct JpegBuilder {
    width: u16,
    height: u16,
    color_type: ColorType,
    quality: u32,
    vertical_flip: bool,
}

impl JpegBuilder {
    pub fn new(width: u16, height: u16, color_type: ColorType) -> Self {
        Self {
            width,
            height,
            color_type,
            quality: 90,
            vertical_flip: false,
        }
    }

    pub fn quality(&mut self, quality: u32) -> &mut Self {
        self.quality = quality.clamp(1, 100);
        self
    }

    pub fn vertical_flip(&mut self, yes: bool) -> &mut Self {
        self.vertical_flip = yes;
        self
    }

    pub fn create_request<'a>(&self, data: &'a [u8]) -> EncodingRequest<'a> {
        let quality_factor = if self.quality < 50 {
            5000 / self.quality
        } else {
            200 - self.quality * 2
        };
        let subsample = self.quality <= 90;
        EncodingRequest {
            width: self.width,
            height: self.height,
            color_type: self.color_type,
            subsample,
            quality_factor,
            vertical_flip: self.vertical_flip,
            data,
        }
    }

    pub fn encode_with<W: Write>(&self, data: &[u8], writer: &mut W) -> PictorResult<()> {
        let request = self.create_request(data);
        request.encode(writer)
    }

    pub fn encode(&self, data: &[u8], path: &Path) -> PictorResult<()> {
        let request = self.create_request(data);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        request.encode(&mut file)
    }
}
