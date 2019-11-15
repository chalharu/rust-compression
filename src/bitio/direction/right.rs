//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(any(feature = "deflate", test))]

use crate::bitio::direction::Direction;
use crate::core::mem::size_of;
use crate::core::ops::{Shl, Shr};
use num_traits::Zero;

#[derive(Debug)]
pub(crate) struct Right;

impl Direction for Right {
    #[inline]
    fn forward<T>(value: T, count: usize) -> T
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
    fn backward<T>(value: T, count: usize) -> T
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
    fn convert<T>(value: T, src_cap: usize, dst_cap: usize) -> T
    where
        T: Shl<usize, Output = T> + Shr<usize, Output = T> + Zero,
    {
        debug_assert!(src_cap <= (size_of::<T>() << 3));
        debug_assert!(dst_cap <= (size_of::<T>() << 3));
        let s = (size_of::<T>() << 3) - dst_cap;
        debug_assert!(s != (size_of::<T>() << 3));
        (value << s) >> s
    }

    #[inline]
    fn is_reverse() -> bool {
        true
    }
}
