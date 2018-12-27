//! rust-compression
//!
//! # Overview
//! Compression libraries implemented by pure Rust.
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
//!
//! # Examples
//!
//! ```rust
//! extern crate compression;
//! use compression::prelude::*;
//!
//! fn main() {
//!     # #[cfg(feature = "bzip2")]
//!     let compressed = b"aabbaabbaabbaabb\n"
//!         .into_iter()
//!         .cloned()
//!         .encode(&mut BZip2Encoder::new(9), Action::Finish)
//!         .collect::<Result<Vec<_>, _>>()
//!         .unwrap();
//!
//!     # #[cfg(feature = "bzip2")]
//!     let decompressed = compressed
//!         .iter()
//!         .cloned()
//!         .decode(&mut BZip2Decoder::new())
//!         .collect::<Result<Vec<_>, _>>()
//!         .unwrap();
//! }
//! ```

#![crate_type = "lib"]
#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(feature = "std"), feature(alloc))]

#[macro_use]
extern crate cfg_if;
extern crate num_traits;

#[cfg(any(feature = "bzip2", feature = "gzip"))]
#[macro_use]
extern crate lazy_static;

#[cfg(feature = "bzip2")]
#[macro_use]
extern crate log;

#[cfg(feature = "std")]
extern crate std as core;

#[cfg(not(feature = "std"))]
#[macro_use(vec)]
extern crate alloc;

#[cfg(test)]
extern crate rand;
#[cfg(test)]
extern crate rand_xorshift;
#[cfg(test)]
extern crate simple_logger;

mod action;
mod adler32;
mod bitset;
mod bucket_sort;
mod cbuffer;
mod crc32;
mod error;

mod bitio;
mod suffix_array;

mod huffman;
mod lzss;
mod traits;

mod bzip2;
mod deflate;
mod lzhuf;

mod gzip;
mod zlib;

pub mod prelude {
    pub use action::Action;
    cfg_if! {
        if #[cfg(feature = "bzip2")] {
            pub use bzip2::decoder::BZip2Decoder;
            pub use bzip2::encoder::BZip2Encoder;
            pub use bzip2::error::BZip2Error;
        }
    }

    cfg_if! {
        if #[cfg(feature = "deflate")] {
            pub use deflate::decoder::Deflater;
            pub use deflate::encoder::Inflater;
        }
    }
    cfg_if! {
        if #[cfg(feature = "gzip")] {
            pub use gzip::decoder::GZipDecoder;
            pub use gzip::encoder::GZipEncoder;
        }
    }
    cfg_if! {
        if #[cfg(feature = "lzhuf")] {
            pub use lzhuf::LzhufMethod;
            pub use lzhuf::decoder::LzhufDecoder;
            pub use lzhuf::encoder::LzhufEncoder;
        }
    }
    cfg_if! {
        if #[cfg(feature = "zlib")] {
            pub use zlib::decoder::ZlibDecoder;
            pub use zlib::encoder::ZlibEncoder;
        }
    }
    pub use error::CompressionError;
    pub use traits::decoder::{DecodeExt, DecodeIterator, Decoder};
    pub use traits::encoder::{EncodeExt, EncodeIterator, Encoder};
}
