pub mod tables;

pub struct JpegTables {
    /// Quantization table written to the DQT marker for the Y/luma component.
    pub luma_quant: [u8; 64],
    /// Quantization table written to the DQT marker for the Cb/Cr chroma components.
    pub chroma_quant: [u8; 64],
    /// Internal multiplier table used after DCT for the Y/luma component.
    pub luma_quant_factors: [f32; 64],
    /// Internal multiplier table used after DCT for the Cb/Cr chroma components.
    pub chroma_quant_factors: [f32; 64],
}
