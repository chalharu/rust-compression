//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(any(feature = "bzip2", feature = "gzip"))]

use crate::core::borrow::Borrow;
use crate::core::fmt;
use crate::core::hash::{BuildHasher, Hasher};
use lazy_static::lazy_static;

#[cfg(any(feature = "gzip", test))]
lazy_static! {
    pub(crate) static ref IEEE_REVERSE_TABLE: [u32; 256] =
        { make_table_reverse(0xEDB8_8320) };
    pub(crate) static ref IEEE_REVERSE: DigestBuilder<&'static [u32; 256]> = {
        DigestBuilder {
            table: &*IEEE_REVERSE_TABLE,
            initial: 0xFFFF_FFFF,
            poly_repr: PolynomialRepresentation::Reverse,
        }
    };
}

#[cfg(any(feature = "bzip2"))]
lazy_static! {
    pub(crate) static ref IEEE_NORMAL_TABLE: [u32; 256] =
        { make_table_normal(0x04C1_1DB7) };
    pub(crate) static ref IEEE_NORMAL: DigestBuilder<&'static [u32; 256]> = {
        DigestBuilder {
            table: &*IEEE_NORMAL_TABLE,
            initial: 0xFFFF_FFFF,
            poly_repr: PolynomialRepresentation::Normal,
        }
    };
}

#[cfg(any(feature = "gzip", test))]
fn make_table_reverse(poly: u32) -> [u32; 256] {
    let mut table = [0u32; 256];
    for (i, item) in table.iter_mut().enumerate() {
        let mut value = i as u32;
        for _ in 0..8 {
            value = if (value & 1) == 1 {
                (value >> 1) ^ poly
            } else {
                value >> 1
            }
        }
        *item = value;
    }
    table
}

#[cfg(any(feature = "bzip2"))]
fn make_table_normal(poly: u32) -> [u32; 256] {
    let mut table = [0u32; 256];
    for (i, item) in table.iter_mut().enumerate() {
        let mut value = (i << 24) as u32;
        for _ in 0..8 {
            value = if value & (1 << 31) == (1 << 31) {
                (value << 1) ^ poly
            } else {
                value << 1
            }
        }
        *item = value;
    }
    table
}

#[cfg(any(feature = "gzip", test))]
#[inline]
fn update_reverse(value: u32, table: &[u32; 256], byte: u8) -> u32 {
    table[((value as u8) ^ byte) as usize] ^ (value >> 8)
}

#[cfg(any(feature = "bzip2"))]
#[inline]
fn update_normal(value: u32, table: &[u32; 256], byte: u8) -> u32 {
    table[(((value >> 24) as u8) ^ byte) as usize] ^ (value << 8)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PolynomialRepresentation {
    #[cfg(any(feature = "bzip2"))]
    Normal,
    #[cfg(any(feature = "gzip", test))]
    Reverse,
}

pub(crate) struct DigestBuilder<T: Borrow<[u32; 256]> + Clone> {
    table: T,
    initial: u32,
    poly_repr: PolynomialRepresentation,
}

impl<T: Borrow<[u32; 256]> + Clone> BuildHasher for DigestBuilder<T> {
    type Hasher = Digest<T>;
    fn build_hasher(&self) -> Self::Hasher {
        Digest {
            table: self.table.clone(),
            value: self.initial,
            poly_repr: self.poly_repr,
        }
    }
}

pub(crate) struct Digest<T: Borrow<[u32; 256]>> {
    table: T,
    value: u32,
    poly_repr: PolynomialRepresentation,
}

impl<T: Borrow<[u32; 256]>> fmt::Debug for Digest<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Digest")
            // TODO:
            .field("table", &self.table.borrow().iter())
            .field("value", &self.value)
            .field("poly_repr", &self.poly_repr)
            .finish()
    }
}

impl<T: Borrow<[u32; 256]>> Hasher for Digest<T> {
    fn finish(&self) -> u64 {
        u64::from(!self.value)
    }

    fn write(&mut self, bytes: &[u8]) {
        bytes.iter().for_each(|&i| self.write_u8(i))
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.value = match self.poly_repr {
            #[cfg(any(feature = "bzip2"))]
            PolynomialRepresentation::Normal => {
                update_normal(self.value, self.table.borrow(), i)
            }
            #[cfg(any(feature = "gzip", test))]
            PolynomialRepresentation::Reverse => {
                update_reverse(self.value, self.table.borrow(), i)
            }
        };
    }
}

pub(crate) type BuiltinDigest = Digest<&'static [u32; 256]>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ieee_reverse() {
        let mut hasher = IEEE_REVERSE.build_hasher();
        hasher.write(b"123456789");
        assert_eq!(hasher.finish(), 0xcbf4_3926);
    }
}
