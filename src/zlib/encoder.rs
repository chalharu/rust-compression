//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use action::Action;
use adler32::Adler32;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::borrow::BorrowMut;
use core::hash::Hasher;
use core::marker::PhantomData;
use core::mem;
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

pub struct ZlibEncoder {
    inflater: Inflater,
    adler32: Option<Adler32>,
    header_len: u8,
    header: Vec<u8>,
    hash: Option<u32>,
    hashlen: u8,
}

impl Default for ZlibEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ZlibEncoder {
    pub fn new() -> Self {
        // CM - Compression method - 32K deflate = 8
        // CINFO - Window Size - 32K = 7
        // FDICT = 0
        // FLEVEL = 2
        // FCHECK = 1C
        Self {
            inflater: Inflater::new(),
            adler32: Some(Adler32::new()),
            header: vec![0x78, 0xDA],
            header_len: 2,
            hash: None,
            hashlen: 3,
        }
    }

    pub fn with_dict(dict: &[u8]) -> Self {
        // CM - Compression method - 32K deflate = 8
        // CINFO - Window Size - 32K = 7
        // FDICT = 1
        // FLEVEL = 2
        // FCHECK = 25
        let mut dict_idc = Adler32::new();
        dict_idc.write(dict);
        let dict_hash = dict_idc.finish() as u32;
        Self {
            inflater: Inflater::with_dict(dict),
            adler32: Some(Adler32::new()),
            header: vec![
                0x78,
                0xF9,
                (dict_hash >> 24) as u8,
                (dict_hash >> 16) as u8,
                (dict_hash >> 8) as u8,
                dict_hash as u8,
            ],
            header_len: 6,
            hash: None,
            hashlen: 3,
        }
    }
}

impl Encoder for ZlibEncoder {
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
                None
            } else {
                self.hashlen -= 1;
                Some(Ok((hash >> (self.hashlen << 3)) as u8))
            }
        } else {
            let mut adler32 = mem::replace(&mut self.adler32, None);
            let ret = self.inflater.next(
                &mut ScanIterator::<I, _, _>::new(iter, |x: &u8| {
                    adler32.as_mut().unwrap().write_u8(*x)
                }),
                action,
            );
            mem::replace(&mut self.adler32, adler32);
            if ret.is_none() {
                let hash = self.adler32.as_mut().unwrap().finish() as u32;
                let ret = (hash >> 24) as u8;
                self.hash = Some(hash);
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
    use traits::encoder::EncodeExt;

    #[test]
    fn test_unit() {
        let mut encoder = ZlibEncoder::new();
        let ret = b"a".iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(
            ret,
            Ok(vec![
                0x78, 0xDA, 0x4B, 0x04, 0x00, 0x00, 0x62, 0x00, 0x62
            ])
        );
    }

    #[test]
    fn test_unit_with_dict() {
        let mut encoder = ZlibEncoder::with_dict(b"a");
        let ret = b"a".iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(
            ret,
            Ok(vec![
                0x78, 0xF9, 0x00, 0x62, 0x00, 0x62, 0x4B, 0x04, 0x00, 0x00,
                0x62, 0x00, 0x62,
            ])
        );
    }
}
