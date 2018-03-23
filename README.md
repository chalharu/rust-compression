# compression

[![crates.io badge](https://img.shields.io/crates/v/compression.svg)](https://crates.io/crates/compression)
[![Build Status](https://travis-ci.org/chalharu/rust-compression.svg?branch=master)](https://travis-ci.org/chalharu/rust-compression)
[![docs.rs](https://docs.rs/compression/badge.svg)](https://docs.rs/compression)
[![Coverage Status](https://coveralls.io/repos/github/chalharu/rust-compression/badge.svg?branch=master)](https://coveralls.io/github/chalharu/rust-compression?branch=master)

Compression libraries implemented by pure Rust.

```toml
[dependencies]
compression = "0.1"
```

## Features

- **`deflate`** - Enabled by default.

- **`gzip`** - Enabled by default.

- **`zlib`** - Enabled by default.

- **`bzip2`** - Enabled by default.

- **`lzhuf`** - Disabled by default.

- **`std`** - By default, `compression` depends on libstd. However, it can be configured to use the unstable liballoc API instead, for use on platforms that have liballoc but not libstd. This configuration is currently unstable and is not guaranteed to work on all versions of Rust. To depend on `compression` without libstd, use default-features = false in the `compression` section of Cargo.toml to disable its "std" feature.

### Examples

```rust
extern crate compression;
use compression::prelude::*;

fn main() {
    let compressed = b"aabbaabbaabbaabb\n"
        .into_iter()
        .cloned()
        .encode(&mut BZip2Encoder::new(9), Action::Finish)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let decompressed = compressed
        .iter()
        .cloned()
        .decode(&mut BZip2Decoder::new())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
}
```
