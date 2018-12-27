//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(feature = "bzip2")]

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::fmt::{Debug, Formatter, Result};
use core::iter::{FromIterator, IntoIterator, Iterator};
use core::ops::Index;

pub struct BitArray {
    data: Box<[u64]>, // want to use RawVec but that is unstable
    len: usize,
}

impl Index<usize> for BitArray {
    type Output = bool;
    fn index(&self, index: usize) -> &bool {
        const BOOL_TAB: [&bool; 2] = [&false, &true];
        BOOL_TAB[(self.data[index >> 6] >> (index & 63)) as usize & 1]
    }
}

impl BitArray {
    pub fn new(len: usize) -> Self {
        Self {
            data: vec![0_u64; (len + 63) >> 6].into_boxed_slice(),
            len,
        }
    }

    pub fn get(&self, idx: usize) -> bool {
        (self.data[idx >> 6] & (1 << ((idx & 63) as u64))) != 0
    }

    pub fn set(&mut self, idx: usize, value: bool) {
        let v = 1 << ((idx & 63) as u8);
        if value {
            self.data[idx >> 6] |= v;
        } else {
            self.data[idx >> 6] &= !v;
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn set_all(&mut self, value: bool) {
        let writeval = if value { 0xFFFF_FFFF } else { 0 };
        for d in self.data.as_mut().iter_mut() {
            *d = writeval;
        }
    }

    pub fn iter(&self) -> BitArrayIter<&Self> {
        BitArrayIter {
            array: self,
            pos: 0,
            len: self.len(),
            data: 0,
        }
    }

    pub fn u16_iter(&self) -> BitArrayU16Iter {
        BitArrayU16Iter {
            array: self,
            pos: 0,
            len: (self.len() + 15) >> 4,
            data: 0,
        }
    }
}

impl FromIterator<bool> for BitArray {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = bool>,
    {
        let mut s = 0;
        let mut l: usize = 0;
        let it = iter.into_iter();
        let mut data = if let (_, Some(sh)) = it.size_hint() {
            Vec::with_capacity((sh + 63) >> 6)
        } else {
            Vec::new()
        };
        for v in it {
            if v {
                s |= 1 << (l & 63);
            }
            l += 1;
            if l.trailing_zeros() >= 6 {
                data.push(s);
                s = 0;
            }
        }
        if l.trailing_zeros() < 6 {
            data.push(s);
        }
        BitArray {
            data: data.into_boxed_slice(),
            len: l,
        }
    }
}

impl IntoIterator for BitArray {
    type Item = bool;
    type IntoIter = BitArrayIter<Self>;
    fn into_iter(self) -> BitArrayIter<Self> {
        BitArrayIter {
            len: self.len(),
            array: self,
            pos: 0,
            data: 0,
        }
    }
}

pub struct BitArrayIter<A: Borrow<BitArray>> {
    array: A,
    pos: usize,
    len: usize,
    data: u64,
}

impl<A: Borrow<BitArray>> Iterator for BitArrayIter<A> {
    type Item = bool;
    fn next(&mut self) -> Option<bool> {
        if self.pos < self.len {
            let d = 1 << ((self.pos & 63) as u64);
            if d == 1 {
                self.data = self.array.borrow().data[self.pos >> 6]
            }
            self.pos += 1;
            Some(self.data & d != 0)
        } else {
            None
        }
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let newpos = self.pos + n;
        if newpos < self.len {
            self.pos += n;
            let d = 1 << ((newpos & 63) as u64);
            if (self.pos ^ newpos) >> 6 != 0 {
                self.data = self.array.borrow().data[self.pos >> 6]
            }
            self.pos = newpos + 1;
            Some(self.data & d != 0)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl Debug for BitArray {
    fn fmt(&self, f: &mut Formatter) -> Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

pub struct BitArrayU16Iter<'a> {
    array: &'a BitArray,
    pos: usize,
    len: usize,
    data: u64,
}

impl<'a> Iterator for BitArrayU16Iter<'a> {
    type Item = u16;
    fn next(&mut self) -> Option<u16> {
        if self.pos < self.len {
            let d = self.pos & 3;
            if d == 0 {
                self.data = self.array.borrow().data[self.pos >> 2]
            }
            self.pos += 1;
            Some((self.data >> (d << 4)) as u16)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}
