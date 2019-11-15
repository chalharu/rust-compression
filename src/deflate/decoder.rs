//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use crate::bitio::direction::right::Right;
use crate::bitio::reader::{BitRead, BitReader};
use crate::deflate::{
    fix_offset_table, fix_symbol_table, gen_len_tab, gen_off_tab, CodeTable,
};
use crate::error::CompressionError;
use crate::huffman::decoder::HuffmanDecoder;
use crate::lzss::decoder::LzssDecoder;
use crate::lzss::LzssCode;
use crate::traits::decoder::{
    BitDecodeService, BitDecoder, BitDecoderImpl, DecodeIterator, Decoder,
};
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[derive(Debug)]
enum DeflateHuffmanDecoder {
    HuffmanDecoder(HuffmanDecoder<Right>, bool),
    NoComp(u32),
}

impl DeflateHuffmanDecoder {
    pub(crate) fn dec<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<Option<u16>, CompressionError> {
        match *self {
            DeflateHuffmanDecoder::HuffmanDecoder(ref mut rhd, ref mut end) => {
                if *end {
                    Ok(None)
                } else {
                    rhd.dec(reader, iter)
                        .map_err(|_| CompressionError::DataError)
                        .and_then(|x| match x {
                            Some(256) => {
                                *end = true;
                                Ok(None)
                            }
                            None => Err(CompressionError::UnexpectedEof),
                            x => Ok(x),
                        })
                }
            }
            DeflateHuffmanDecoder::NoComp(ref mut block_size) => {
                if *block_size > 0 {
                    *block_size -= 1;
                    reader
                        .read_bits::<u16, _>(8, iter)
                        .map(|x| Some(x.data()))
                        .map_err(|_| CompressionError::UnexpectedEof)
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub(crate) fn end(&self) -> bool {
        match *self {
            DeflateHuffmanDecoder::HuffmanDecoder(_, end) => end,
            DeflateHuffmanDecoder::NoComp(block_size) => block_size == 0,
        }
    }
}

#[derive(Debug)]
struct DeflaterInner {
    symbol_decoder: Option<DeflateHuffmanDecoder>,
    offset_decoder: Option<DeflateHuffmanDecoder>,
    is_final: bool,
    len_tab: CodeTable,
    offset_tab: CodeTable,
}

impl DeflaterInner {
    const SEARCH_TAB_LEN: usize = 12;

    pub(crate) fn new() -> Self {
        Self {
            symbol_decoder: None,
            offset_decoder: None,
            is_final: false,
            len_tab: gen_len_tab(),
            offset_tab: gen_off_tab(),
        }
    }

    fn dec_len_tree<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        hclen: u32,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<DeflateHuffmanDecoder, CompressionError> {
        let len_index = [
            16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
        ];
        let mut len_list = vec![0; 19];
        for &i in len_index.iter().take(hclen as usize) {
            len_list[i] = *reader
                .read_bits(3, iter)
                .map_err(|_| CompressionError::UnexpectedEof)?
                .data_ref();
        }
        Ok(DeflateHuffmanDecoder::HuffmanDecoder(
            HuffmanDecoder::new(&len_list, Self::SEARCH_TAB_LEN)
                .map_err(|_| CompressionError::DataError)?,
            false,
        ))
    }

    fn dec_huff_tree<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        len_decoder: &mut DeflateHuffmanDecoder,
        len: usize,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<DeflateHuffmanDecoder, CompressionError> {
        let mut ll = Vec::new();
        while ll.len() < len {
            match len_decoder.dec(reader, iter)? {
                None => return Err(CompressionError::UnexpectedEof),
                Some(16) => {
                    let last = ll.iter().last().map_or_else(
                        || Err(CompressionError::UnexpectedEof),
                        |&l| Ok(l),
                    )?;
                    for _ in 0..(reader
                        .read_bits::<u8, _>(2, iter)
                        .map_err(|_| CompressionError::UnexpectedEof)?
                        .data()
                        + 3)
                    {
                        ll.push(last);
                    }
                }
                Some(17) => {
                    for _ in 0..(3 + reader
                        .read_bits::<u8, _>(3, iter)
                        .map_err(|_| CompressionError::UnexpectedEof)?
                        .data())
                    {
                        ll.push(0);
                    }
                }
                Some(18) => {
                    for _ in 0..(11
                        + reader
                            .read_bits::<u8, _>(7, iter)
                            .map_err(|_| CompressionError::UnexpectedEof)?
                            .data())
                    {
                        ll.push(0);
                    }
                }
                Some(n) => ll.push(n as u8),
            }
        }
        Ok(DeflateHuffmanDecoder::HuffmanDecoder(
            HuffmanDecoder::new(&ll, Self::SEARCH_TAB_LEN)
                .map_err(|_| CompressionError::DataError)?,
            false,
        ))
    }

    fn init_block<R: BitRead, I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut R,
        iter: &mut I,
    ) -> Result<(), CompressionError> {
        self.is_final = reader
            .read_bits::<u8, _>(1, iter)
            .map_err(|_| CompressionError::UnexpectedEof)?
            .data()
            == 1;
        match reader
            .read_bits::<u8, _>(2, iter)
            .map_err(|_| CompressionError::UnexpectedEof)?
            .data()
        {
            // 無圧縮
            0 => {
                let _ = reader.skip_to_next_byte();
                let block_len = reader
                    .read_bits(16, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data();
                let block_len_checksum = reader
                    .read_bits::<u32, _>(16, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data();
                if (block_len ^ block_len_checksum) != 0xFFFF {
                    return Err(CompressionError::DataError);
                }
                self.symbol_decoder =
                    Some(DeflateHuffmanDecoder::NoComp(block_len));
                self.offset_decoder = None;
            }
            // 固定ハフマン
            1 => {
                self.symbol_decoder =
                    Some(DeflateHuffmanDecoder::HuffmanDecoder(
                        HuffmanDecoder::new(
                            &fix_symbol_table(),
                            Self::SEARCH_TAB_LEN,
                        )
                        .map_err(|_| CompressionError::DataError)?,
                        false,
                    ));
                self.offset_decoder =
                    Some(DeflateHuffmanDecoder::HuffmanDecoder(
                        HuffmanDecoder::new(
                            fix_offset_table(),
                            Self::SEARCH_TAB_LEN,
                        )
                        .map_err(|_| CompressionError::DataError)?,
                        false,
                    ));
            }
            // カスタムハフマン
            2 => {
                // リテラル/長さ符号の個数
                let hlit = reader
                    .read_bits::<u16, _>(5, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data()
                    + 257;
                // 距離符号の個数
                let hdist = reader
                    .read_bits::<u16, _>(5, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data()
                    + 1;
                // 長さ符号の個数
                let hclen = reader
                    .read_bits::<u32, _>(4, iter)
                    .map_err(|_| CompressionError::UnexpectedEof)?
                    .data()
                    + 4;
                let mut lt = self.dec_len_tree(hclen, reader, iter)?;
                self.symbol_decoder = Some(self.dec_huff_tree(
                    &mut lt,
                    hlit as usize,
                    reader,
                    iter,
                )?);
                self.offset_decoder = Some(self.dec_huff_tree(
                    &mut lt,
                    hdist as usize,
                    reader,
                    iter,
                )?);
            }
            // ありえない
            _ => unreachable!(),
        }
        Ok(())
    }
}

impl BitDecodeService for DeflaterInner {
    type Direction = Right;
    type Error = CompressionError;
    type Output = LzssCode;

    fn next<I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut BitReader<Self::Direction>,
        iter: &mut I,
    ) -> Result<Option<LzssCode>, CompressionError> {
        loop {
            if self
                .symbol_decoder
                .as_ref()
                .map_or_else(|| true, DeflateHuffmanDecoder::end)
            {
                if self.is_final {
                    return Ok(None);
                }
                self.init_block(reader, iter)?;
            } else if let Some(sym) =
                self.symbol_decoder.as_mut().unwrap().dec(reader, iter)?
            {
                if sym <= 255 {
                    return Ok(Some(LzssCode::Symbol(sym as u8)));
                } else {
                    let len_index = (sym - 257) as usize;
                    let extbits = (&self.len_tab).ext_bits(len_index);
                    let len = (self.len_tab.convert_back(
                        len_index,
                        if extbits != 0 {
                            reader
                                .read_bits(extbits, iter)
                                .map_err(|_| CompressionError::UnexpectedEof)?
                                .data()
                        } else {
                            0
                        },
                    ) + 3) as usize;
                    let off_index = self
                        .offset_decoder
                        .as_mut()
                        .unwrap()
                        .dec(reader, iter)?
                        .ok_or_else(|| CompressionError::UnexpectedEof)?
                        as usize;
                    let off_extbits = (&self.offset_tab).ext_bits(off_index);
                    let pos = self.offset_tab.convert_back(
                        off_index,
                        if off_extbits != 0 {
                            reader
                                .read_bits(off_extbits, iter)
                                .map_err(|_| CompressionError::UnexpectedEof)?
                                .data()
                        } else {
                            0
                        },
                    ) as usize;
                    return Ok(Some(LzssCode::Reference { len, pos }));
                }
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct DeflaterBase {
    inner: DeflaterInner,
    lzss_decoder: LzssDecoder,
}

impl Default for DeflaterBase {
    fn default() -> Self {
        Self::new()
    }
}

impl DeflaterBase {
    const MAX_BLOCK_SIZE: usize = 0x1_0000;

    pub(crate) fn new() -> Self {
        Self {
            lzss_decoder: LzssDecoder::new(Self::MAX_BLOCK_SIZE),
            inner: DeflaterInner::new(),
        }
    }

    pub(crate) fn with_dict(dict: &[u8]) -> Self {
        Self {
            lzss_decoder: LzssDecoder::with_dict(Self::MAX_BLOCK_SIZE, dict),
            inner: DeflaterInner::new(),
        }
    }
}

impl BitDecodeService for DeflaterBase {
    type Direction = Right;
    type Error = CompressionError;
    type Output = u8;

    fn next<I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut BitReader<Self::Direction>,
        iter: &mut I,
    ) -> Result<Option<u8>, Self::Error> {
        let mut bd = BitDecoder::<DeflaterInner, _, _>::with_service(
            &mut self.inner,
            reader,
        );
        self.lzss_decoder
            .next(&mut DecodeIterator::<I, _, _>::new(iter, &mut bd).flatten())
            .transpose()
    }
}

#[derive(Debug)]
pub struct Deflater {
    inner: BitDecoderImpl<DeflaterBase>,
}

impl Deflater {
    pub fn new() -> Self {
        Self {
            inner: BitDecoderImpl::<DeflaterBase>::new(),
        }
    }

    pub fn with_dict(dict: &[u8]) -> Self {
        Self {
            inner: BitDecoderImpl::<DeflaterBase>::with_service(
                DeflaterBase::with_dict(dict),
                BitReader::new(),
            ),
        }
    }
}

impl Default for Deflater {
    fn default() -> Self {
        Self {
            inner: BitDecoderImpl::<DeflaterBase>::new(),
        }
    }
}

impl Decoder for Deflater {
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
