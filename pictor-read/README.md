# pictor-read

### Decoding library for jpg / png / qoi

## JPG Codec

### JPEG Codec

#### Supported

- Baseline sequential JPEG
- 8-bit grayscale and three-component color images
- `4:4:4` and `4:2:0` component sampling
- Input from any `Read` implementation or directly from a file
- Malformed and unsupported input is reported through `PictorError`

| Feature           | Decoder |
| ----------------- | :-----: |
| Baseline DCT JPEG |    ✓    |
| 8-bit samples     |    ✓    |
| Grayscale         |    ✓    |
| RGB               |    ✓    |
| 4:4:4 sampling    |    ✓    |
| 4:2:0 subsampling |    ✓    |
| Huffman coding    |    ✓    |
| Vertical flipping |    —    |

#### Not Currently Supported

- Progressive, lossless, or arithmetic-coded JPEG
- Restart markers and multiple scans
- 12-bit or higher precision images
- CMYK, YCCK, and four-component images
- Alpha-channel preservation
- EXIF orientation and general metadata handling
- ICC profiles

## PNG Codec

#### Supported

- Non-interlaced PNG images
- Multiple consecutive IDAT chunks
- Palette data (PLTE)
- PNG signature, chunk, and CRC validation
- Input from any Read implementation or a file
- Malformed input is reported through PictorError

| Feature                  | Decoder |
| ------------------------ | :-----: |
| PNG                      |    ✓    |
| Scanline filtering       |    ✓    |
| Deflate decompression    |    ✓    |
| Multiple IDAT chunks     |    ✓    |
| Palette data (`PLTE`)    |    ✓    |
| CRC validation           |    ✓    |
| Non-interlaced images    |    ✓    |
| Adam7 interlacing        |    —    |
| Vertical flipping        |    —    |
| Metadata preservation    |    —    |

#### Not Currently Supported
- Adam7 interlacing is not supported
- tRNS transparency is recognized but not expanded into the decoded output
- Palette and transparency chunks cannot currently be encoded
- PNG metadata is not preserved
- Decoded output remains raw sample data; color type and bit-depth conversion are not currently supported

## QOI Codec

Encoder and decoder for the [Quite OK Image Format (QOI)].

This implementation is largely a direct port of the reference C implementation,
with some Rust-specific additions.

For the QOI format specification and reference implementation, see
[phoboslab/qoi](https://github.com/phoboslab/qoi/).