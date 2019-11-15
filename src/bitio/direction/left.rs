//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(any(feature = "lzhuf", feature = "bzip2", test))]

use crate::bitio::direction::Direction;
use crate::core::mem::size_of;
use crate::core::ops::{Shl, Shr};
use num_traits::Zero;

#[derive(Debug)]
pub(crate) struct Left;

impl Direction for Left {
    #[inline]
    fn forward<T>(value: T, count: usize) -> T
    where
        T: Shl<usize, Output = T> + Shr<usize, Output = T> + Zero,
    {
        if (size_of::<T>() << 3) <= count {
            T::zero()
        } else {
            value << count
        }
    }

    #[inline]
    fn backward<T>(value: T, count: usize) -> T
    where
        T: Shl<usize, Output = T> + Shr<usize, Output = T> + Zero,
    {
        if (size_of::<T>() << 3) <= count {
            T::zero()
        } else {
            value >> count
        }
    }

    #[inline]
    fn convert<T>(value: T, src_cap: usize, dst_cap: usize) -> T
    where
        T: Shl<usize, Output = T> + Shr<usize, Output = T> + Zero,
    {
        debug_assert!(src_cap <= (size_of::<T>() << 3));
        debug_assert!(dst_cap <= (size_of::<T>() << 3));
        if src_cap > dst_cap {
            debug_assert!((src_cap - dst_cap) != (size_of::<T>() << 3));
            value >> (src_cap - dst_cap)
        } else {
            debug_assert!((dst_cap - src_cap) != (size_of::<T>() << 3));
            value << (dst_cap - src_cap)
        }
    }

    #[inline]
    fn is_reverse() -> bool {
        false
    }
}
