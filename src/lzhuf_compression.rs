//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

pub enum LzhufCompression {
    Lh4,
    Lh5,
    Lh6,
    Lh7,
}

impl LzhufCompression {
    pub fn dictionary_bits(&self) -> usize {
        match self {
            &LzhufCompression::Lh4 => 12,
            &LzhufCompression::Lh5 => 13,
            &LzhufCompression::Lh6 => 15,
            &LzhufCompression::Lh7 => 16,
        }
    }

    pub fn offset_bits(&self) -> usize {
        match self {
            &LzhufCompression::Lh4 => 4,
            &LzhufCompression::Lh5 => 4,
            &LzhufCompression::Lh6 => 5,
            &LzhufCompression::Lh7 => 5,
        }
    }
}
