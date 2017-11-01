#![crate_type = "lib"]

//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

extern crate num_iter;
extern crate num_traits;

mod bit_vector;
mod bit_writer;
mod bit_reader;
mod huffman_encoder;
mod huffman_decoder;
mod internal;

pub use bit_reader::*;
pub use bit_vector::BitVector;
pub use bit_writer::*;
pub use huffman_encoder::*;
pub use huffman_decoder::*;

use internal::*;

