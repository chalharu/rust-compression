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
use bitio::reader::{BitRead, BitReader};
use core::hash::Hasher;
use deflate::decoder::DeflaterBase;
use error::CompressionError;
use traits::decoder::{BitDecodeService, BitDecoderImpl, Decoder};

#[derive(Default)]
pub struct ZlibDecoderBase {
    deflater: DeflaterBase,
    adler32: Adler32,
    dict_hash: Option<u32>,
    header: Vec<u8>,
    header_needlen: usize,
    header_checked: bool,
}

impl ZlibDecoderBase {
    fn with_dict(dict: &[u8]) -> Self {
        let mut dict_idc = Adler32::new();
        dict_idc.write(dict);
        Self {
            deflater: DeflaterBase::with_dict(dict),
            adler32: Adler32::new(),
            dict_hash: Some(dict_idc.finish() as u32),
            header: Vec::new(),
            header_needlen: 0,
            header_checked: false,
        }
    }
}

impl BitDecodeService for ZlibDecoderBase {
    type Direction = Right;
    type Error = CompressionError;
    type Output = u8;

    fn next<I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut BitReader<Self::Direction>,
        iter: &mut I,
    ) -> Result<Option<u8>, Self::Error> {
        loop {
            if !self.header_checked {
                let s = reader
                    .read_bits::<u8, _>(8, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data();
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
                        % 31)
                        != 0
                    {
                        return Err(CompressionError::DataError);
                    }
                    if let Some(dict_hash) = self.dict_hash {
                        let dictid = self
                            .header
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
                match self.deflater.next(reader, iter) {
                    Ok(Some(s)) => {
                        self.adler32.write_u8(s);
                        return Ok(Some(s));
                    }
                    Ok(None) => {
                        reader.skip_to_next_byte();
                        let c = (0..4)
                            .map(|_| reader.read_bits::<u32, _>(8, iter))
                            .fold(
                                Ok(0_u32),
                                |s: Result<_, CompressionError>, x| {
                                    Ok(x.map_err(|_| {
                                        CompressionError::UnexpectedEof
                                    })?
                                    .data()
                                        | (s.map_err(|_| {
                                            CompressionError::UnexpectedEof
                                        })? << 8))
                                },
                            )?;
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

pub struct ZlibDecoder {
    inner: BitDecoderImpl<ZlibDecoderBase>,
}

impl ZlibDecoder {
    pub fn new() -> Self {
        Self {
            inner: BitDecoderImpl::<ZlibDecoderBase>::new(),
        }
    }

    pub fn with_dict(dict: &[u8]) -> Self {
        Self {
            inner: BitDecoderImpl::<ZlibDecoderBase>::with_service(
                ZlibDecoderBase::with_dict(dict),
                BitReader::new(),
            ),
        }
    }
}

impl Default for ZlibDecoder {
    fn default() -> Self {
        Self {
            inner: BitDecoderImpl::<ZlibDecoderBase>::new(),
        }
    }
}

impl Decoder for ZlibDecoder {
    type Input = u8;
    type Output = u8;
    type Error = CompressionError;

    fn next<I: Iterator<Item = Self::Input>>(
        &mut self,
        iter: &mut I,
    ) -> Option<Result<Self::Output, Self::Error>> {
        self.inner.next(iter)
    }
}
