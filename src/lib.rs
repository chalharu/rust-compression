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
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub(crate) use std as core;

#[cfg(not(feature = "std"))]
pub(crate) use core;

#[cfg(not(feature = "std"))]
// #[macro_use(vec)]
extern crate alloc;

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
    pub use crate::action::Action;
    use cfg_if::cfg_if;

    cfg_if! {
        if #[cfg(feature = "bzip2")] {
            pub use crate::bzip2::decoder::BZip2Decoder;
            pub use crate::bzip2::encoder::BZip2Encoder;
            pub use crate::bzip2::error::BZip2Error;
        }
    }

    cfg_if! {
        if #[cfg(feature = "deflate")] {
            pub use crate::deflate::decoder::Deflater;
            pub use crate::deflate::encoder::Inflater;
        }
    }
    cfg_if! {
        if #[cfg(feature = "gzip")] {
            pub use crate::gzip::decoder::GZipDecoder;
            pub use crate::gzip::encoder::GZipEncoder;
        }
    }
    cfg_if! {
        if #[cfg(feature = "lzhuf")] {
            pub use crate::lzhuf::LzhufMethod;
            pub use crate::lzhuf::decoder::LzhufDecoder;
            pub use crate::lzhuf::encoder::LzhufEncoder;
        }
    }
    cfg_if! {
        if #[cfg(feature = "zlib")] {
            pub use crate::zlib::decoder::ZlibDecoder;
            pub use crate::zlib::encoder::ZlibEncoder;
        }
    }
    cfg_if! {
        if #[cfg(feature = "lzss")] {
            pub use crate::lzss::decoder::LzssDecoder;
            pub use crate::lzss::encoder::LzssEncoder;
            pub use crate::lzss::LzssCode;
        }
    }
    pub use crate::error::CompressionError;
    pub use crate::traits::decoder::{DecodeExt, DecodeIterator, Decoder};
    pub use crate::traits::encoder::{EncodeExt, EncodeIterator, Encoder};
}
