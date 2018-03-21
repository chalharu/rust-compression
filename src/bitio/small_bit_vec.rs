//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use core::mem::size_of;

#[derive(Clone, Debug, Eq)]
pub struct SmallBitVec<T = u32> {
    data: T,
    len: usize,
}

impl<T, U> PartialEq<SmallBitVec<U>> for SmallBitVec<T>
where
    T: PartialEq<U>,
{
    fn eq(&self, other: &SmallBitVec<U>) -> bool {
        self.data == other.data && self.len == other.len
    }
}

impl<T> SmallBitVec<T> {
    pub fn new(data: T, len: usize) -> Self {
        debug_assert!(
            (size_of::<T>() * 8) >= len,
            "len is greater than bit capacity"
        );
        SmallBitVec { data, len }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn data_ref(&self) -> &T {
        &self.data
    }
}

impl<T: Copy> SmallBitVec<T> {
    #[inline]
    pub fn data(&self) -> T {
        self.data
    }
}

pub trait SmallBitVecReverse {
    fn reverse(&self) -> Self;
}

impl SmallBitVecReverse for SmallBitVec<u8> {
    fn reverse(&self) -> Self {
        let mut x = self.data;
        x = (x & 0x55) << 1 | (x & 0xAA) >> 1;
        x = (x & 0x33) << 2 | (x & 0xCC) >> 2;
        x = x << 4 | x >> 4;
        x >>= 8 - self.len;
        Self::new(x, self.len)
    }
}

impl SmallBitVecReverse for SmallBitVec<u16> {
    fn reverse(&self) -> Self {
        let mut x = self.data;
        x = (x & 0x5555) << 1 | (x & 0xAAAA) >> 1;
        x = (x & 0x3333) << 2 | (x & 0xCCCC) >> 2;
        x = (x & 0x0F0F) << 4 | (x & 0xF0F0) >> 4;
        x = x << 8 | x >> 8;
        x >>= 16 - self.len;
        Self::new(x, self.len)
    }
}

impl SmallBitVecReverse for SmallBitVec<u32> {
    fn reverse(&self) -> Self {
        let mut x = self.data;
        x = (x & 0x5555_5555) << 1 | (x & 0xAAAA_AAAA) >> 1;
        x = (x & 0x3333_3333) << 2 | (x & 0xCCCC_CCCC) >> 2;
        x = (x & 0x0F0F_0F0F) << 4 | (x & 0xF0F0_F0F0) >> 4;
        x = (x & 0x00FF_00FF) << 8 | (x & 0xFF00_FF00) >> 8;
        x = x << 16 | x >> 16;
        x >>= 32 - self.len;
        Self::new(x, self.len)
    }
}

impl SmallBitVecReverse for SmallBitVec<u64> {
    fn reverse(&self) -> Self {
        let mut x = self.data;
        x = (x & 0x5555_5555_5555_5555) << 1 | (x & 0xAAAA_AAAA_AAAA_AAAA) >> 1;
        x = (x & 0x3333_3333_3333_3333) << 2 | (x & 0xCCCC_CCCC_CCCC_CCCC) >> 2;
        x = (x & 0x0F0F_0F0F_0F0F_0F0F) << 4 | (x & 0xF0F0_F0F0_F0F0_F0F0) >> 4;
        x = (x & 0x00FF_00FF_00FF_00FF) << 8 | (x & 0xFF00_FF00_FF00_FF00) >> 8;
        x = (x & 0x0000_FFFF_0000_FFFF) << 16
            | (x & 0xFFFF_FFFF_0000_0000) >> 16;
        x = x << 32 | x >> 32;
        x >>= 64 - self.len;
        Self::new(x, self.len)
    }
}

impl<T: Default> Default for SmallBitVec<T> {
    fn default() -> Self {
        SmallBitVec::<T>::new(T::default(), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smallbitvec_u8_reverse() {
        assert_eq!(
            SmallBitVec::<u8>::new(0x1D, 7).reverse(),
            SmallBitVec::<u8>::new(0x5C, 7)
        );
    }

    #[test]
    fn smallbitvec_u16_reverse() {
        assert_eq!(
            SmallBitVec::<u16>::new(0x071F, 13).reverse(),
            SmallBitVec::<u16>::new(0x1F1C, 13)
        );
    }

    #[test]
    fn smallbitvec_u32_reverse() {
        assert_eq!(
            SmallBitVec::<u32>::new(0xC71F, 17).reverse(),
            SmallBitVec::<u32>::new(0x0001_F1C6, 17)
        );
    }

    #[test]
    fn smallbitvec_u64_reverse() {
        assert_eq!(
            SmallBitVec::<u64>::new(0xC71F, 17).reverse(),
            SmallBitVec::<u64>::new(0x0001_F1C6, 17)
        );
    }
}
