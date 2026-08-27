# pictor-write

### Encoding library for jpg / png / qoi

## JPG codec

```text
Grayscale, RGB, and RGBA-style input
Configurable quality from 1 to 100
4:4:4 and 4:2:0 chroma sampling
Images with dimensions that are not multiples of JPEG block sizes
Optional vertical flipping
Output to any Write implementation or directly to a file
```

| Feature           | Encoder |
| ----------------- | :-----: |
| Baseline DCT JPEG |    ✓    |
| 8-bit samples     |    ✓    |
| Grayscale         |    ✓    |
| RGB               |    ✓    |
| 4:4:4 sampling    |    ✓    |
| 4:2:0 subsampling |    ✓    |
| Huffman coding    |    ✓    |
| Vertical flipping |    ✓    |

#### Not Currently Supported

- Progressive, lossless, or arithmetic-coded JPEG
- Restart markers and multiple scans
- 12-bit or higher precision images
- CMYK, YCCK, and four-component images
- Alpha-channel preservation
- EXIF orientation and general metadata handling
- ICC profiles

## PNG

#### Supported

- Generic sample types implementing Sample
- Configurable dimensions, stride, compression level, color type, and filtering
- Automatic or forced PNG filter selection
- Optional vertical flipping
- Optional parallel scanline filtering with the rayon feature
- Output to memory, any Write implementation, or a file

| Feature                  | Encoder |
| ------------------------ | :-----: |
| PNG                      |    ✓    |
| Generic sample types     |    ✓    |
| Scanline filtering       |    ✓    |
| Automatic filter selection |  ✓    |
| Forced filter selection  |    ✓    |
| Deflate compression      |    ✓    |
| Configurable compression |    ✓    |
| Vertical flipping        |    ✓    |
| Parallel filtering       | Optional |
| Adam7 interlacing        |    —    |
| Palette (`PLTE`)         |    —    |
| Transparency (`tRNS`)    |    —    |
| Metadata                 |    —    |

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