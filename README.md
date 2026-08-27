# pictor

### Basic Encoding / Decoding image library for jpg / png / qoi


## Jpg coodec

| JPG Features      | Encoder | Decoder |
| ----------------- | :-----: | :-----: |
| Baseline DCT JPEG |    ✓    |    ✓    |
| 8-bit samples     |    ✓    |    ✓    |
| Grayscale         |    ✓    |    ✓    |
| RGB               |    ✓    |    ✓    |
| 4:4:4 sampling    |    ✓    |    ✓    |
| 4:2:0 subsampling |    ✓    |    ✓    |
| Huffman coding    |    ✓    |    ✓    |
| Vertical flipping |    ✓    |    —    |

## QOI Codec

Encoder and decoder for the [Quite OK Image Format (QOI)].

This implementation is largely a direct port of the reference C implementation,
with some Rust-specific additions.

For the QOI format specification and reference implementation, see
[phoboslab/qoi](https://github.com/phoboslab/qoi/).