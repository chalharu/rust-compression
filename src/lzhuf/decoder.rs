//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::direction::left::Left;
use bitio::reader::{BitRead, BitReader};
use error::CompressionError;
use huffman::decoder::HuffmanDecoder;
use lzhuf::{LzhufMethod, LZSS_MIN_MATCH};
use lzss::decoder::LzssDecoder;
use lzss::LzssCode;
use traits::decoder::{
    BitDecodeService, BitDecoder, BitDecoderImpl, DecodeIterator, Decoder,
};

enum LzhufHuffmanDecoder {
    HuffmanDecoder(HuffmanDecoder<Left>),
    Default(u16),
}

impl LzhufHuffmanDecoder {
    pub fn dec<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<Option<u16>, CompressionError> {
        match *self {
            LzhufHuffmanDecoder::HuffmanDecoder(ref mut hd) => hd
                .dec(reader, iter)
                .map_err(|_| CompressionError::DataError),
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

    fn dec_len<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<u8, CompressionError> {
        let mut c = reader
            .read_bits::<u8, _>(3, iter)
            .map_err(|_| CompressionError::UnexpectedEof)?
            .data();
        if c == 7 {
            while reader
                .read_bits::<u8, _>(1, iter)
                .map_err(|_| CompressionError::UnexpectedEof)?
                .data()
                == 1
            {
                c += 1;
            }
        }
        Ok(c)
    }

    fn dec_len_tree<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        tbit_len: usize,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<LzhufHuffmanDecoder, CompressionError> {
        let len = reader
            .read_bits::<u16, _>(tbit_len, iter)
            .map_err(|_| CompressionError::UnexpectedEof)?
            .data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                reader
                    .read_bits::<u16, _>(tbit_len, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data(),
            ))
        } else {
            let mut ll = Vec::new();
            while ll.len() < len {
                if ll.len() == 3 {
                    for _ in 0..reader
                        .read_bits::<u8, _>(2, iter)
                        .map_err(|_| CompressionError::UnexpectedEof)?
                        .data()
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
                ll.push(self.dec_len(reader, iter)?);
            }
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(
                HuffmanDecoder::new(&ll, 5)
                    .map_err(|_| CompressionError::DataError)?,
            ))
        }
    }

    fn dec_symb_tree<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        len_decoder: &mut LzhufHuffmanDecoder,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<LzhufHuffmanDecoder, CompressionError> {
        let len = reader
            .read_bits::<u16, _>(9, iter)
            .map_err(|_| CompressionError::UnexpectedEof)?
            .data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                reader
                    .read_bits::<u16, _>(9, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data(),
            ))
        } else {
            let mut ll = Vec::new();
            while ll.len() < len {
                match len_decoder.dec(reader, iter)? {
                    None => return Err(CompressionError::UnexpectedEof),
                    Some(0) => ll.push(0),
                    Some(1) => {
                        for _ in 0..(3 + reader
                            .read_bits::<u8, _>(4, iter)
                            .map_err(|_| CompressionError::UnexpectedEof)?
                            .data())
                        {
                            ll.push(0);
                        }
                    }
                    Some(2) => {
                        for _ in 0..(20
                            + reader
                                .read_bits::<u16, _>(9, iter)
                                .map_err(|_| CompressionError::UnexpectedEof)?
                                .data())
                        {
                            ll.push(0);
                        }
                    }
                    Some(n) => ll.push((n - 2) as u8),
                }
            }
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(
                HuffmanDecoder::new(&ll, Self::SEARCH_TAB_LEN)
                    .map_err(|_| CompressionError::DataError)?,
            ))
        }
    }

    fn dec_offs_tree<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        pbit_len: usize,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<LzhufHuffmanDecoder, CompressionError> {
        let len = reader
            .read_bits::<u16, _>(pbit_len, iter)
            .map_err(|_| CompressionError::UnexpectedEof)?
            .data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                reader
                    .read_bits::<u16, _>(pbit_len, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data(),
            ))
        } else {
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(
                HuffmanDecoder::new(
                    &(0..len)
                        .map(|_| self.dec_len(reader, iter))
                        .collect::<Result<Vec<u8>, CompressionError>>()?,
                    Self::SEARCH_TAB_LEN,
                )
                .map_err(|_| CompressionError::DataError)?,
            ))
        }
    }

    fn init_block<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<bool, CompressionError> {
        match reader
            .read_bits::<u16, _>(16, iter)
            .map(|x| (x.data(), x.len()))
            .map_err(|_| CompressionError::UnexpectedEof)?
        {
            (s, 16) if s != 0 => {
                self.block_len = s as usize;
                let mut lt = self.dec_len_tree(5, reader, iter)?;
                self.symbol_decoder =
                    Some(self.dec_symb_tree(&mut lt, reader, iter)?);
                let offlen = self.offset_len;
                self.offset_decoder =
                    Some(self.dec_offs_tree(offlen, reader, iter)?);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl BitDecodeService for LzhufDecoderInner {
    type Direction = Left;
    type Error = CompressionError;
    type Output = LzssCode;

    fn next<I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut BitReader<Self::Direction>,
        iter: &mut I,
    ) -> Result<Option<LzssCode>, CompressionError> {
        if self.block_len == 0 && !self.init_block(reader, iter)? {
            return Ok(None);
        }
        self.block_len -= 1;
        let sym = self
            .symbol_decoder
            .as_mut()
            .unwrap()
            .dec(reader, iter)?
            .ok_or_else(|| CompressionError::UnexpectedEof)?
            as usize;
        if sym <= 255 {
            Ok(Some(LzssCode::Symbol(sym as u8)))
        } else {
            let len = sym - 256 + self.min_match;
            let mut pos = self
                .offset_decoder
                .as_mut()
                .unwrap()
                .dec(reader, iter)?
                .ok_or_else(|| CompressionError::UnexpectedEof)?
                as usize;
            if pos > 1 {
                pos = (1 << (pos - 1))
                    | reader
                        .read_bits::<u16, _>(pos - 1, iter)
                        .map_err(|_| CompressionError::UnexpectedEof)?
                        .data() as usize;
            }
            Ok(Some(LzssCode::Reference { len, pos }))
        }
    }
}

pub struct LzhufDecoderBase {
    lzss_decoder: LzssDecoder,
    inner: LzhufDecoderInner,
}

impl LzhufDecoderBase {
    const MAX_BLOCK_SIZE: usize = 0x1_0000;

    pub fn new(method: &LzhufMethod) -> Self {
        Self {
            lzss_decoder: LzssDecoder::new(Self::MAX_BLOCK_SIZE),
            inner: LzhufDecoderInner::new(method),
        }
    }
}

impl BitDecodeService for LzhufDecoderBase {
    type Direction = Left;
    type Error = CompressionError;
    type Output = u8;

    fn next<I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut BitReader<Self::Direction>,
        iter: &mut I,
    ) -> Result<Option<u8>, Self::Error> {
        let mut bd = BitDecoder::<LzhufDecoderInner, _, _>::with_service(
            &mut self.inner,
            reader,
        );
        self.lzss_decoder
            .next(&mut DecodeIterator::<I, _, _>::new(iter, &mut bd).flatten())
            .transpose()
    }
}

pub struct LzhufDecoder {
    inner: BitDecoderImpl<LzhufDecoderBase>,
}

impl LzhufDecoder {
    pub fn new(method: &LzhufMethod) -> Self {
        Self {
            inner: BitDecoderImpl::<LzhufDecoderBase>::with_service(
                LzhufDecoderBase::new(method),
                BitReader::new(),
            ),
        }
    }
}

impl Decoder for LzhufDecoder {
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
