//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::mem;

pub(crate) struct MtfPosition {
    data: Vec<usize>,
}

impl MtfPosition {
    pub fn new(count: usize) -> Self {
        Self {
            data: (0..count).collect::<Vec<_>>(),
        }
    }
    pub fn pop(&mut self, value: usize) -> usize {
        if value == self.data[0] {
            0
        } else {
            let mut t = self.data[0];
            self.data[0] = value;

            for (i, d) in self.data.iter_mut().enumerate().skip(1) {
                mem::swap(d, &mut t);

                if t == value {
                    return i;
                }
            }
            unreachable!();
        }
    }
}

pub(crate) struct MtfPositionDecoder {
    data: Vec<usize>,
}

impl MtfPositionDecoder {
    pub fn new(count: usize) -> Self {
        Self {
            data: (0..count).collect::<Vec<_>>(),
        }
    }
    pub fn pop(&mut self, value: usize) -> usize {
        if value == 0 {
            self.data[0]
        } else {
            let t = self.data[value];

            for i in (0..value).rev() {
                self.data[i + 1] = self.data[i];
            }

            self.data[0] = t;
            t
        }
    }
}
