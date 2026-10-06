# pictor

pictor re-exports the `pictor-read` and `pictor-write` create, and offers an interface for format conversion.

For more imformation about the codecs, see [pictor-read](../pictor-read/README.md) and [pictor-write](../pictor-write/README.md).

### Example for format conversion

`pictor` offers a more convenient method of converting between formats.

```rust
    convert(path)?.png().encode(dest)?;
```

After selecting the target format, we can still use any available builder methods
```rust
    convert(path)?.png().compression(CompressionLevel::Level6).encode(dest)?;
```
