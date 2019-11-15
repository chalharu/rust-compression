//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

pub(crate) mod left;
pub(crate) mod right;

use crate::core::ops::{Shl, Shr};
use num_traits::Zero;

pub(crate) trait Direction {
    fn forward<T>(value: T, count: usize) -> T
    where
        T: Shl<usize, Output = T> + Shr<usize, Output = T> + Zero;
    fn backward<T>(value: T, count: usize) -> T
    where
        T: Shl<usize, Output = T> + Shr<usize, Output = T> + Zero;
    fn convert<T>(value: T, src_cap: usize, dst_cap: usize) -> T
    where
        T: Shl<usize, Output = T> + Shr<usize, Output = T> + Zero;
    fn is_reverse() -> bool;
}
