//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

pub mod reader;
pub mod writer;
pub mod small_bit_vec;

use core::mem::size_of;
use core::ops::{Shl, Shr};
use num_traits::Zero;

// pub(crate) use reader::{BitRead, BitReader};
// pub(crate) use small_bit_vec::{SmallBitVec, SmallBitVecReverse};
// pub(crate) use writer::{Action, BitIterator, BitWriteExt, BitWriter};

pub trait Direction {
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

pub struct Left;

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

pub struct Right;

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
