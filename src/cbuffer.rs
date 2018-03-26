//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(any(feature = "deflate", feature = "lzhuf", feature = "bzip2"))]

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
use core::{ptr, usize};
use core::iter;
use core::ops::{Index, IndexMut};

#[derive(Clone, Hash, Debug)]
pub(crate) struct CircularBuffer<T> {
    data: Box<[T]>, // want to use RawVec but that is unstable
    pos: usize,
    is_first: bool,
}

impl<T: Default + Clone> CircularBuffer<T> {
    pub fn new(cap: usize) -> Self {
        Self {
            data: vec![T::default(); cap].into_boxed_slice(),
            pos: 0,
            is_first: true,
        }
    }
}

impl<T> CircularBuffer<T> {
    pub fn append(&mut self, data: &[T]) {
        let len = self.data.len() - self.pos;
        let count = data.len();

        unsafe {
            let daddr = data.get_unchecked(0);
            if len >= data.len() {
                ptr::copy_nonoverlapping(
                    daddr,
                    self.data.get_unchecked_mut(self.pos),
                    count,
                );
                self.pos = if len != data.len() {
                    self.pos + data.len()
                } else {
                    self.is_first = false;
                    0
                };
            } else {
                ptr::copy_nonoverlapping(
                    daddr,
                    self.data.get_unchecked_mut(self.pos),
                    len,
                );
                ptr::copy_nonoverlapping(
                    data.get_unchecked(len),
                    self.data.get_unchecked_mut(0),
                    count - len,
                );
                self.pos = data.len() - len;
                self.is_first = false;
            }
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        if self.is_first {
            self.pos
        } else {
            self.data.len()
        }
    }

    #[inline]
    pub fn cap(&self) -> usize {
        self.data.len()
    }

    #[cfg(any(feature = "lzhuf", feature = "deflate", test))]
    pub fn push(&mut self, data: T) {
        self.data[self.pos] = data;
        self.pos += 1;
        if self.pos >= self.data.len() {
            self.pos = 0;
            self.is_first = false;
        }
    }

    #[inline]
    pub fn get_raw_pos(&self) -> usize {
        self.pos
    }

    #[inline]
    pub fn get_raw_ref(&self) -> &[T] {
        self.data.as_ref()
    }

    #[inline]
    fn inner_idx(&self, idx: usize) -> usize {
        debug_assert!(idx < self.len());
        if self.pos < idx + 1 {
            self.pos + self.data.len() - idx - 1
        } else {
            self.pos - idx - 1
        }
    }
}

impl<T> Index<usize> for CircularBuffer<T> {
    type Output = T;

    #[inline]
    fn index(&self, idx: usize) -> &T {
        &self.data[self.inner_idx(idx)]
    }
}

impl<T> IndexMut<usize> for CircularBuffer<T> {
    #[inline]
    fn index_mut(&mut self, idx: usize) -> &mut T {
        let idx = self.inner_idx(idx);
        &mut self.data[idx]
    }
}

impl<T: Clone> IntoIterator for CircularBuffer<T> {
    type Item = T;
    type IntoIter = Iterator<T>;
    fn into_iter(self) -> Iterator<T> {
        Iterator::new(self)
    }
}

pub struct Iterator<T> {
    inner: CircularBuffer<T>,
    pos: usize,
    len: usize,
}

impl<T> Iterator<T> {
    fn new(inner: CircularBuffer<T>) -> Self {
        Self {
            pos: inner.get_raw_pos(),
            len: inner.len(),
            inner,
        }
    }
}
impl<T: Clone> iter::Iterator for Iterator<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.len > 0 {
            self.len -= 1;
            self.pos = if self.pos > 0 {
                self.pos - 1
            } else {
                self.inner.cap() - 1
            };
            Some(
                unsafe { self.inner.get_raw_ref().get_unchecked(self.pos) }
                    .clone(),
            )
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    #[test]
    fn add() {
        let mut buf = CircularBuffer::new(16);
        for d in 0..17 {
            buf.push(d);
        }

        for (d, i) in buf.clone().into_iter().enumerate().take(16) {
            assert_eq!(i, 16 - d);
        }

        for d in 17..21 {
            buf.push(d);
        }

        for (d, i) in buf.clone().into_iter().enumerate().take(16) {
            assert_eq!(i, 20 - d);
        }
    }

    #[test]
    fn append() {
        let mut buf = CircularBuffer::new(16);
        buf.append(&(1..17).collect::<Vec<_>>());

        for (d, i) in buf.clone().into_iter().enumerate().take(16) {
            assert_eq!(i, 16 - d);
        }

        buf.append(&(17..21).collect::<Vec<_>>());

        for (d, i) in buf.clone().into_iter().enumerate().take(16) {
            assert_eq!(i, 20 - d);
        }

        buf.append(&(1..17).collect::<Vec<_>>());

        for (d, i) in buf.clone().into_iter().enumerate().take(16) {
            assert_eq!(i, 16 - d);
        }
    }
}
