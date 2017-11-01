
//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

mod bucket_sort;
pub use self::bucket_sort::*;

pub trait MinValue {
    fn min_value() -> Self;
}

pub trait MaxValue {
    fn max_value() -> Self;
}

impl MinValue for u8 {
    fn min_value() -> Self {
        u8::min_value()
    }
}

impl MaxValue for u8 {
    fn max_value() -> Self {
        u8::max_value()
    }
}

impl MinValue for u16 {
    fn min_value() -> Self {
        u16::min_value()
    }
}

impl MaxValue for u16 {
    fn max_value() -> Self {
        u16::max_value()
    }
}
