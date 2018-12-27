//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use action::Action;
use core::borrow::BorrowMut;
use core::hash::{BuildHasher, Hasher};
use core::marker::PhantomData;
use core::mem;
use crc32::{BuiltinDigest, IEEE_REVERSE};
use deflate::encoder::Inflater;
use error::CompressionError;
use traits::encoder::Encoder;

struct ScanIterator<I: Iterator, BI: BorrowMut<I>, F: FnMut(&I::Item) -> ()> {
    phantom: PhantomData<I>,
    inner: BI,
    closure: F,
}

impl<I: Iterator, BI: BorrowMut<I>, F: FnMut(&I::Item) -> ()> Iterator
    for ScanIterator<I, BI, F>
{
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        let ret = self.inner.borrow_mut().next();
        if let Some(ref s) = ret {
            (self.closure)(s);
        }
        ret
    }
}

impl<I: Iterator, BI: BorrowMut<I>, F: FnMut(&I::Item) -> ()>
    ScanIterator<I, BI, F>
{
    pub fn new(inner: BI, closure: F) -> Self {
        Self {
            inner,
            closure,
            phantom: PhantomData,
        }
    }
}

pub struct GZipEncoder {
    inflater: Inflater,
    crc32: Option<BuiltinDigest>,
    header_len: u8,
    header: [u8; 10],
    hash: Option<u32>,
    hashlen: u8,
    i_size_len: u8,
    i_size: u32,
}

impl Default for GZipEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl GZipEncoder {
    pub fn new() -> Self {
        Self {
            inflater: Inflater::new(),
            crc32: Some(IEEE_REVERSE.build_hasher()),
            header: [
                0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF
            ],
            header_len: 10,
            hash: None,
            hashlen: 3,
            i_size: 0,
            i_size_len: 4,
        }
    }
}

impl Encoder for GZipEncoder {
    type Error = CompressionError;
    fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: &Action,
    ) -> Option<Result<u8, CompressionError>> {
        let hlen = self.header_len;
        if hlen > 0 {
            let hlen_all = self.header.len();
            self.header_len = hlen - 1;
            Some(Ok(self.header[hlen_all - hlen as usize]))
        } else if let Some(hash) = self.hash {
            if self.hashlen == 0 {
                if self.i_size_len == 0 {
                    None
                } else {
                    self.i_size_len -= 1;
                    let ret = self.i_size as u8;
                    self.i_size >>= 8;
                    Some(Ok(ret))
                }
            } else {
                self.hashlen -= 1;
                self.hash = Some(hash >> 8);
                Some(Ok(hash as u8))
            }
        } else {
            let mut crc32 = mem::replace(&mut self.crc32, None);
            let mut i_size = self.i_size;
            let ret = self.inflater.next(
                &mut ScanIterator::<I, _, _>::new(iter, |x: &u8| {
                    crc32.as_mut().unwrap().write_u8(*x);
                    i_size += 1;
                }),
                action,
            );
            self.i_size = i_size;
            mem::replace(&mut self.crc32, crc32);
            if ret.is_none() {
                let hash = self.crc32.as_mut().unwrap().finish() as u32;
                let ret = hash as u8;
                self.hash = Some(hash >> 8);
                Some(Ok(ret))
            } else {
                ret
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;
    use traits::encoder::EncodeExt;

    #[test]
    fn test_unit() {
        let mut encoder = GZipEncoder::new();
        let ret = b"a".iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(
            ret,
            Ok(vec![
                0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF,
                0x4b, 0x04, 0x00, 0x43, 0xbe, 0xb7, 0xe8, 0x01, 0x00, 0x00,
                0x00,
            ])
        );
    }
}
