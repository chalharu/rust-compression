//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use std::ops::{Index, IndexMut};
use std::ptr;
use std::usize;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct CircularBuffer<T> {
    data: Vec<T>, // want to use RawVec but that is unstable
    pos: usize,
}

impl<T: Default + Clone> CircularBuffer<T> {
    pub fn new(cap: usize) -> Self {
        Self {
            data: vec![T::default(); cap],
            pos: 0,
        }
    }

    pub fn push(&mut self, data: T) {
        self.data[self.pos] = data;
        self.pos += 1;
        if self.pos >= self.data.len() {
            self.pos = 0;
        }
    }

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
            }
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
}

impl<T> CircularBuffer<T> {
    #[inline]
    fn inner_idx(&self, idx: usize) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add() {
        let mut buf = CircularBuffer::new(16);
        for d in (0..17).into_iter() {
            buf.push(d);
        }

        for d in (0..16).into_iter() {
            assert_eq!(buf[d], 16 - d);
        }

        for d in (17..21).into_iter() {
            buf.push(d);
        }

        for d in (0..16).into_iter() {
            assert_eq!(buf[d], 20 - d);
        }
    }

    #[test]
    fn append() {
        let mut buf = CircularBuffer::new(16);
        buf.append(&(1..17).collect::<Vec<_>>());

        for d in (0..16).into_iter() {
            assert_eq!(buf[d], 16 - d);
        }

        buf.append(&(17..21).collect::<Vec<_>>());

        for d in (0..16).into_iter() {
            assert_eq!(buf[d], 20 - d);
        }

        buf.append(&(1..17).collect::<Vec<_>>());

        for d in (0..16).into_iter() {
            assert_eq!(buf[d], 16 - d);
        }
    }
}
