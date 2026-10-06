# pictor

pictor re-exports the `pictor-read` and `pictor-write` create, and offers an interface for format conversion.

For more imformation about the codes, see [pictor-read](../pictor-read/README.md) and [pictor-write](../pictor-write/README.md).

### Example for format conversion

```rust
    convert(path)?.png().encode(dest)?;
```

```rust
    convert(path)?.png().compression(CompressionLevel::Level6).encode(dest)?;
```
