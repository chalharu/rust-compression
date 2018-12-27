//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use adler32::Adler32;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::direction::right::Right;
use bitio::reader::BitRead;
use core::hash::Hasher;
use deflate::decoder::Deflater;
use error::CompressionError;
use traits::decoder::Decoder;

#[derive(Default)]
pub struct ZlibDecoder {
    deflater: Deflater,
    adler32: Adler32,
    dict_hash: Option<u32>,
    header: Vec<u8>,
    header_needlen: usize,
    header_checked: bool,
}

impl ZlibDecoder {
    pub fn new() -> Self {
        Self {
            deflater: Deflater::new(),
            adler32: Adler32::new(),
            dict_hash: None,
            header: Vec::new(),
            header_needlen: 0,
            header_checked: false,
        }
    }

    pub fn with_dict(dict: &[u8]) -> Self {
        let mut dict_idc = Adler32::new();
        dict_idc.write(dict);
        Self {
            deflater: Deflater::with_dict(dict),
            adler32: Adler32::new(),
            dict_hash: Some(dict_idc.finish() as u32),
            header: Vec::new(),
            header_needlen: 0,
            header_checked: false,
        }
    }
}

impl<R> Decoder<R> for ZlibDecoder
where
    R: BitRead<Right>,
{
    type Error = CompressionError;
    type Output = u8;

    fn next(&mut self, iter: &mut R) -> Result<Option<u8>, Self::Error> {
        loop {
            if !self.header_checked {
                let s = try!(
                    iter.read_bits::<u8>(8)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data();
                if self.header.len() < 2 {
                    self.header.push(s);
                    if self.header.len() == 2 {
                        self.header_needlen =
                            if (self.header[1] & 0b10_0000) == 0b10_0000 {
                                6
                            } else if self.dict_hash.is_some() {
                                return Err(CompressionError::DataError);
                            } else {
                                2
                            };
                    }
                } else if self.header_needlen != self.header.len() {
                    self.header.push(s);
                }
                if self.header_needlen == self.header.len() {
                    if (self.header[0] & 0x0F) != 8 {
                        return Err(CompressionError::DataError);
                    }
                    if ((self.header[0] & 0xF0) >> 4) > 7 {
                        return Err(CompressionError::DataError);
                    }
                    if ((u16::from(self.header[0]) << 8
                        | u16::from(self.header[1]))
                        % 31) != 0
                    {
                        return Err(CompressionError::DataError);
                    }
                    if let Some(dict_hash) = self.dict_hash {
                        let dictid = self.header
                            .as_slice()
                            .iter()
                            .skip(2)
                            .fold(0_u32, |s, &x| u32::from(x) | (s << 8));
                        if dict_hash != dictid {
                            return Err(CompressionError::DataError);
                        }
                    }
                    self.header_checked = true;
                }
            } else {
                // body
                match self.deflater.next(iter) {
                    Ok(Some(s)) => {
                        self.adler32.write_u8(s);
                        return Ok(Some(s));
                    }
                    Ok(None) => {
                        iter.skip_to_next_byte();
                        let c = try!(
                            (0..4).map(|_| iter.read_bits::<u32>(8)).fold(
                                Ok(0_u32),
                                |s: Result<_, CompressionError>, x| {
                                    Ok(try!(x.map_err(|_| {
                                        CompressionError::UnexpectedEof
                                    })).data()
                                        | (try!(s.map_err(|_| {
                                            CompressionError::UnexpectedEof
                                        }))
                                            << 8))
                                },
                            )
                        );
                        if u64::from(c) != self.adler32.finish() {
                            return Err(CompressionError::DataError);
                        } else {
                            return Ok(None);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
}
