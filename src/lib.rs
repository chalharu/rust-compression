//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#![crate_type = "lib"]
#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(feature = "std"), feature(alloc))]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate num_traits;

#[cfg(feature = "std")]
extern crate std as core;

#[cfg(not(feature = "std"))]
#[macro_use(vec)]
extern crate alloc;

#[cfg(test)]
extern crate rand;
#[cfg(test)]
extern crate simple_logger;

mod cbuffer;
mod action;
mod error;
mod bucket_sort;
mod bitset;
mod crc32;
mod adler32;

mod bitio;
mod suffix_array;

mod traits;
mod huffman;
mod lzss;

mod bzip2;
mod deflate;
mod lzhuf;

mod zlib;
mod gzip;

pub mod prelude {
    pub use action::Action;
    pub use bzip2::decoder::BZip2Decoder;
    pub use bzip2::encoder::BZip2Encoder;
    pub use bzip2::error::BZip2Error;
    pub use deflate::decoder::Deflater;
    pub use deflate::encoder::Inflater;
    pub use error::CompressionError;
    pub use gzip::decoder::GZipDecoder;
    pub use gzip::encoder::GZipEncoder;
    pub use lzhuf::LzhufMethod;
    pub use lzhuf::decoder::LzhufDecoder;
    pub use lzhuf::encoder::LzhufEncoder;
    pub use traits::decoder::{DecodeExt, DecodeIterator, Decoder};
    pub use traits::encoder::{EncodeExt, EncodeIterator, Encoder};
    pub use zlib::decoder::ZlibDecoder;
    pub use zlib::encoder::ZlibEncoder;
}
