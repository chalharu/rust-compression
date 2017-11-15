//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#![crate_type = "lib"]
#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]

extern crate num_iter;
extern crate num_traits;

#[cfg(test)]
extern crate rand;

mod bit_vector;
mod bit_writer;
mod bit_reader;
mod huffman_encoder;
mod huffman_decoder;
mod internal;
mod write;
mod read;
mod compress;
mod decompress;
mod lzhuf_compress;
mod lzhuf_compression;
mod io_queue;
mod lzhuf_decompress;


pub use bit_reader::*;
pub use bit_vector::BitVector;
pub use bit_writer::*;
pub use compress::Action;

pub use compress::Compress;
pub use decompress::Decompress;
pub use huffman_decoder::*;
pub use huffman_encoder::*;
use internal::*;
use io_queue::*;
pub use lzhuf_compression::LzhufCompression;
pub use read::Read;
pub use write::Write;
