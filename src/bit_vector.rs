//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

#[derive(Copy, PartialEq, Eq, Clone, Debug, Hash)]
pub struct BitVector {
    data: u32,
    len: usize,
}

impl BitVector {
    pub fn new(data: u32, len: usize) -> Self {
        BitVector { data, len }
    }

    pub fn reverse(&self) -> Self {
        let mut x = self.data;
        x = (x & 0x55555555) << 1 | (x & 0xAAAAAAAA) >> 1;
        x = (x & 0x33333333) << 2 | (x & 0xCCCCCCCC) >> 2;
        x = (x & 0x0F0F0F0F) << 4 | (x & 0xF0F0F0F0) >> 4;
        x = x << 24 | (x & 0xFF00) << 8 | (x & 0xFF0000) >> 8 | x >> 24;
        x >>= 32 - self.len;
        Self::new(x, self.len)
    }

    #[inline(always)]
    pub fn data(&self) -> u32 {
        self.data
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitvector_reverse() {
        assert_eq!(
            BitVector::new(0xC71F, 17).reverse(),
            BitVector::new(0x1F1C6, 17)
        );
    }
}
