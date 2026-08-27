# pictor

pictor re-exports the `pictor-read` and `pictor-write` create, and offers an interface for format conversion.

For more imformation about the codes, see [pictor-read](../pictor-read/README.md) and [pictor-write](../pictor-write/README.md).

### Example for format conversion

The pictor crate is stil work in progress. Currely, format conversion is achieve using pictor-read directly

```rust
    let path = PathBuf::from("in.png");
    let dest = PathBuf::from("out.jpg");

    PngDecodeRequest::decode(path)?.convert().png().compression(CompressionLevel::Level7).encode(dest)?;

    Ok(())
```
