//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(any(feature = "deflate", feature = "lzhuf", feature = "bzip2"))]

pub mod direction;
pub mod reader;
pub mod small_bit_vec;
pub mod writer;
