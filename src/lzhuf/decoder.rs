//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::Left;
use bitio::reader::BitRead;
use error::CompressionError;
use huffman::decoder::HuffmanDecoder;
use lzhuf::{LzhufMethod, LZSS_MIN_MATCH};
use lzss::LzssCode;
use lzss::decoder::LzssDecoder;
use traits::decoder::Decoder;

enum LzhufHuffmanDecoder {
    HuffmanDecoder(HuffmanDecoder<Left>),
    Default(u16),
}

impl LzhufHuffmanDecoder {
    pub fn dec<R: BitRead<Left>>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<u16>, CompressionError> {
        match *self {
            LzhufHuffmanDecoder::HuffmanDecoder(ref mut hd) => {
                hd.dec(reader).map_err(|_| CompressionError::DataError)
            }
            LzhufHuffmanDecoder::Default(s) => Ok(Some(s)),
        }
    }
}

pub struct LzhufDecoderInner {
    offset_len: usize,
    min_match: usize,
    block_len: usize,
    symbol_decoder: Option<LzhufHuffmanDecoder>,
    offset_decoder: Option<LzhufHuffmanDecoder>,
}

impl LzhufDecoderInner {
    const SEARCH_TAB_LEN: usize = 12;

    pub fn new(method: &LzhufMethod) -> Self {
        Self {
            offset_len: method.offset_bits(),
            min_match: LZSS_MIN_MATCH,
            block_len: 0,

            symbol_decoder: None,
            offset_decoder: None,
        }
    }

    fn dec_len<R: BitRead<Left>>(
        &mut self,
        reader: &mut R,
    ) -> Result<u8, CompressionError> {
        let mut c = try!(
            reader
                .read_bits::<u8>(3)
                .map_err(|_| CompressionError::UnexpectedEof)
        ).data();
        if c == 7 {
            while try!(
                reader
                    .read_bits::<u8>(1)
                    .map_err(|_| CompressionError::UnexpectedEof)
            ).data() == 1
            {
                c += 1;
            }
        }
        Ok(c)
    }

    fn dec_len_tree<R: BitRead<Left>>(
        &mut self,
        tbit_len: usize,
        reader: &mut R,
    ) -> Result<LzhufHuffmanDecoder, CompressionError> {
        let len = try!(
            reader
                .read_bits::<u16>(tbit_len)
                .map_err(|_| CompressionError::UnexpectedEof)
        ).data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                try!(
                    reader
                        .read_bits::<u16>(tbit_len)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data(),
            ))
        } else {
            let mut ll = Vec::new();
            while ll.len() < len {
                if ll.len() == 3 {
                    for _ in 0
                        ..try!(
                            reader
                                .read_bits::<u8>(2)
                                .map_err(|_| CompressionError::UnexpectedEof)
                        ).data()
                    {
                        ll.push(0);
                    }
                    if ll.len() > len {
                        return Err(CompressionError::DataError);
                    }
                    if ll.len() == len {
                        break;
                    }
                }
                ll.push(try!(self.dec_len(reader)));
            }
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(try!(
                HuffmanDecoder::new(&ll, 5)
                    .map_err(|_| CompressionError::DataError)
            )))
        }
    }

    fn dec_symb_tree<R: BitRead<Left>>(
        &mut self,
        len_decoder: &mut LzhufHuffmanDecoder,
        reader: &mut R,
    ) -> Result<LzhufHuffmanDecoder, CompressionError> {
        let len = try!(
            reader
                .read_bits::<u16>(9)
                .map_err(|_| CompressionError::UnexpectedEof)
        ).data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                try!(
                    reader
                        .read_bits::<u16>(9)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data(),
            ))
        } else {
            let mut ll = Vec::new();
            while ll.len() < len {
                match try!(len_decoder.dec(reader)) {
                    None => return Err(CompressionError::UnexpectedEof),
                    Some(0) => ll.push(0),
                    Some(1) => {
                        for _ in 0
                            ..(3
                                + try!(reader.read_bits::<u8>(4).map_err(
                                    |_| CompressionError::UnexpectedEof
                                )).data())
                        {
                            ll.push(0);
                        }
                    }
                    Some(2) => {
                        for _ in 0
                            ..(20
                                + try!(reader.read_bits::<u16>(9).map_err(
                                    |_| CompressionError::UnexpectedEof
                                )).data())
                        {
                            ll.push(0);
                        }
                    }
                    Some(n) => ll.push((n - 2) as u8),
                }
            }
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(try!(
                HuffmanDecoder::new(&ll, Self::SEARCH_TAB_LEN)
                    .map_err(|_| CompressionError::DataError)
            )))
        }
    }

    fn dec_offs_tree<R: BitRead<Left>>(
        &mut self,
        pbit_len: usize,
        reader: &mut R,
    ) -> Result<LzhufHuffmanDecoder, CompressionError> {
        let len = try!(
            reader
                .read_bits::<u16>(pbit_len)
                .map_err(|_| CompressionError::UnexpectedEof)
        ).data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                try!(
                    reader
                        .read_bits::<u16>(pbit_len)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data(),
            ))
        } else {
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(try!(
                HuffmanDecoder::new(
                    &try!(
                        (0..len)
                            .map(|_| self.dec_len(reader))
                            .collect::<Result<Vec<u8>, CompressionError>>()
                    ),
                    Self::SEARCH_TAB_LEN,
                ).map_err(|_| CompressionError::DataError)
            )))
        }
    }

    fn init_block<R: BitRead<Left>>(
        &mut self,
        reader: &mut R,
    ) -> Result<bool, CompressionError> {
        match try!(
            reader
                .read_bits::<u16>(16)
                .map(|x| (x.data(), x.len()))
                .map_err(|_| CompressionError::UnexpectedEof)
        ) {
            (s, 16) if s != 0 => {
                self.block_len = s as usize;
                let mut lt = try!(self.dec_len_tree(5, reader));
                self.symbol_decoder =
                    Some(try!(self.dec_symb_tree(&mut lt, reader)));
                let offlen = self.offset_len;
                self.offset_decoder =
                    Some(try!(self.dec_offs_tree(offlen, reader)));
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl Decoder for LzhufDecoderInner {
    type Item = LzssCode;
    type Error = CompressionError;
    type Direction = Left;
    fn next<R: BitRead<Left>>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<LzssCode>, CompressionError> {
        if self.block_len == 0 && !try!(self.init_block(reader)) {
            return Ok(None);
        }
        self.block_len -= 1;
        let sym = try!(
            try!(self.symbol_decoder.as_mut().unwrap().dec(reader))
                .ok_or_else(|| CompressionError::UnexpectedEof)
        ) as usize;
        if sym <= 255 {
            Ok(Some(LzssCode::Symbol(sym as u8)))
        } else {
            let len = sym - 256 + self.min_match;
            let mut pos = try!(
                try!(self.offset_decoder.as_mut().unwrap().dec(reader))
                    .ok_or_else(|| CompressionError::UnexpectedEof)
            ) as usize;
            if pos > 1 {
                pos = (1 << (pos - 1))
                    | try!(
                        reader
                            .read_bits::<u16>(pos - 1)
                            .map_err(|_| CompressionError::UnexpectedEof)
                    ).data() as usize;
            }
            Ok(Some(LzssCode::Reference { len, pos }))
        }
    }
}

pub struct LzhufDecoder {
    lzss_decoder: LzssDecoder,
    inner: LzhufDecoderInner,
}

impl LzhufDecoder {
    const MAX_BLOCK_SIZE: usize = 0x1_0000;

    pub fn new(method: &LzhufMethod) -> Self {
        Self {
            lzss_decoder: LzssDecoder::new(Self::MAX_BLOCK_SIZE),
            inner: LzhufDecoderInner::new(method),
        }
    }
}

impl Decoder for LzhufDecoder {
    type Error = CompressionError;
    type Direction = Left;
    type Item = u8;

    fn next<R: BitRead<Self::Direction>>(
        &mut self,
        iter: &mut R,
    ) -> Result<Option<u8>, Self::Error> {
        self.lzss_decoder.next(&mut self.inner.iter(iter))
    }
}
