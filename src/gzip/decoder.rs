//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::direction::right::Right;
use bitio::reader::{BitRead, BitReader};
use core::hash::{BuildHasher, Hasher};
use crc32::{BuiltinDigest, IEEE_REVERSE};
use deflate::decoder::DeflaterBase;
use error::CompressionError;
use traits::decoder::{BitDecodeService, BitDecoderImpl, Decoder};

pub struct GZipDecoderBase {
    deflater: DeflaterBase,
    crc32: BuiltinDigest,
    header: Vec<u8>,
    header_needlen: usize,
    header_checked: bool,
    i_size: u32,
}

impl Default for GZipDecoderBase {
    fn default() -> Self {
        Self::new()
    }
}

impl GZipDecoderBase {
    pub fn new() -> Self {
        Self {
            deflater: DeflaterBase::new(),
            crc32: IEEE_REVERSE.build_hasher(),
            header: Vec::new(),
            header_needlen: 10,
            header_checked: false,
            i_size: 0,
        }
    }

    fn read_u32<R: BitRead, I: Iterator<Item = u8>>(
        reader: &mut R,
        iter: &mut I,
    ) -> Result<u32, CompressionError> {
        (0..4)
            .map(|i| {
                Ok(reader
                    .read_bits::<u32, _>(8, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data()
                    << (i << 3))
            })
            .fold(
                Ok(0_u32),
                |s: Result<_, CompressionError>,
                 x: Result<_, CompressionError>| {
                    Ok(x? | s?)
                },
            )
    }
}

impl BitDecodeService for GZipDecoderBase {
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
                if self.header.len() < self.header_needlen {
                    self.header.push(
                        reader
                            .read_bits::<u8, _>(8, iter)
                            .map_err(|_| CompressionError::UnexpectedEof)?
                            .data(),
                    );
                } else {
                    // ID1 1byte
                    if self.header[0] != 0x1f {
                        return Err(CompressionError::DataError);
                    }
                    // ID2 1byte
                    if self.header[1] != 0x8b {
                        return Err(CompressionError::DataError);
                    }
                    // CM 1byte
                    if self.header[2] != 0x08 {
                        return Err(CompressionError::DataError);
                    }

                    // FLG 1byte
                    let flg = self.header[3];
                    if (flg & 0b1110_0000) != 0 {
                        return Err(CompressionError::DataError);
                    }

                    // MTIME 4byte
                    // XFL, OS 2byte

                    // FEXTRA
                    let xlen = if (flg & 0b100) != 0 {
                        // XLEN 2byte
                        if self.header.len() < 12 {
                            if self.header_needlen < 12 {
                                self.header_needlen = 12;
                            }
                            continue;
                        }
                        (usize::from(self.header[11]) << 8)
                            + usize::from(self.header[10])
                            + 2
                    } else {
                        0
                    };
                    let fextra_last = 10 + xlen;
                    if self.header.len() < fextra_last {
                        if self.header_needlen < fextra_last {
                            self.header_needlen = fextra_last;
                        }
                        continue;
                    }

                    // FNAME
                    let fname_len = if (flg & 0b1000) != 0 {
                        // NAME
                        if let Some(l) = (&self.header)
                            .iter()
                            .enumerate()
                            .skip(10 + xlen)
                            .skip_while(|x| *x.1 != 0)
                            .next()
                        {
                            l.0 - 10 - xlen
                        } else {
                            self.header_needlen += 1;
                            continue;
                        }
                    } else {
                        0
                    };

                    // FCOMMENT
                    let fcomment_len = if (flg & 0b1_0000) != 0 {
                        // COMMENT
                        if let Some(l) = (&self.header)
                            .iter()
                            .enumerate()
                            .skip(10 + xlen + fname_len)
                            .skip_while(|x| *x.1 != 0)
                            .next()
                        {
                            l.0 - 10 - xlen - fname_len
                        } else {
                            self.header_needlen += 1;
                            continue;
                        }
                    } else {
                        0
                    };

                    // FHCRC
                    if (flg & 0b10) != 0 {
                        let comment_last = 10 + xlen + fname_len + fcomment_len;
                        if self.header.len() < comment_last + 2 {
                            if self.header_needlen < comment_last + 2 {
                                self.header_needlen = comment_last + 2;
                            }
                            continue;
                        }
                        // HCRC
                        let hcrc = (u16::from(self.header[1 + comment_last])
                            << 8)
                            | u16::from(self.header[comment_last]);
                        let mut digest4header = IEEE_REVERSE.build_hasher();
                        digest4header.write(&self.header[0..(comment_last)]);
                        if hcrc != digest4header.finish() as u16 {
                            return Err(CompressionError::DataError);
                        }
                    }

                    self.header_checked = true;
                }
            } else {
                // body
                match self.deflater.next(reader, iter) {
                    Ok(Some(s)) => {
                        self.crc32.write_u8(s);
                        self.i_size += 1;
                        return Ok(Some(s));
                    }
                    Ok(None) => {
                        reader.skip_to_next_byte();

                        let c = Self::read_u32(reader, iter)?;
                        if u64::from(c) != self.crc32.finish() {
                            return Err(CompressionError::DataError);
                        }
                        let i_size = Self::read_u32(reader, iter)?;
                        if i_size != self.i_size {
                            return Err(CompressionError::DataError);
                        }
                        return Ok(None);
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
}

pub struct GZipDecoder {
    inner: BitDecoderImpl<GZipDecoderBase>,
}

impl GZipDecoder {
    pub fn new() -> Self {
        Self {
            inner: BitDecoderImpl::<GZipDecoderBase>::new(),
        }
    }
}

impl Default for GZipDecoder {
    fn default() -> Self {
        Self {
            inner: BitDecoderImpl::<GZipDecoderBase>::new(),
        }
    }
}

impl Decoder for GZipDecoder {
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
