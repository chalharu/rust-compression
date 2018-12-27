//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::direction::right::Right;
use bitio::reader::BitRead;
use deflate::{fix_offset_table, fix_symbol_table, gen_len_tab, gen_off_tab,
              CodeTable};
use error::CompressionError;
use huffman::decoder::HuffmanDecoder;
use lzss::LzssCode;
use lzss::decoder::LzssDecoder;
use traits::decoder::Decoder;

enum DeflateHuffmanDecoder {
    HuffmanDecoder(HuffmanDecoder<Right>, bool),
    NoComp(u32),
}

impl DeflateHuffmanDecoder {
    pub fn dec<R: BitRead<Right>>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<u16>, CompressionError> {
        match *self {
            DeflateHuffmanDecoder::HuffmanDecoder(ref mut rhd, ref mut end) => {
                if *end {
                    Ok(None)
                } else {
                    rhd.dec(reader)
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
                        .read_bits::<u16>(8)
                        .map(|x| Some(x.data()))
                        .map_err(|_| CompressionError::UnexpectedEof)
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub fn end(&self) -> bool {
        match *self {
            DeflateHuffmanDecoder::HuffmanDecoder(_, end) => end,
            DeflateHuffmanDecoder::NoComp(block_size) => block_size == 0,
        }
    }
}

struct DeflaterInner {
    symbol_decoder: Option<DeflateHuffmanDecoder>,
    offset_decoder: Option<DeflateHuffmanDecoder>,
    is_final: bool,
    len_tab: CodeTable,
    offset_tab: CodeTable,
}

impl DeflaterInner {
    const SEARCH_TAB_LEN: usize = 12;

    pub fn new() -> Self {
        Self {
            symbol_decoder: None,
            offset_decoder: None,
            is_final: false,
            len_tab: gen_len_tab(),
            offset_tab: gen_off_tab(),
        }
    }

    fn dec_len_tree<R: BitRead<Right>>(
        &mut self,
        hclen: u32,
        reader: &mut R,
    ) -> Result<DeflateHuffmanDecoder, CompressionError> {
        let len_index = [
            16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
        ];
        let mut len_list = vec![0; 19];
        for &i in len_index.iter().take(hclen as usize) {
            len_list[i] = *try!(
                reader
                    .read_bits(3)
                    .map_err(|_| CompressionError::UnexpectedEof)
            ).data_ref();
        }
        Ok(DeflateHuffmanDecoder::HuffmanDecoder(
            try!(
                HuffmanDecoder::new(&len_list, Self::SEARCH_TAB_LEN,)
                    .map_err(|_| CompressionError::DataError)
            ),
            false,
        ))
    }

    fn dec_huff_tree<R: BitRead<Right>>(
        &mut self,
        len_decoder: &mut DeflateHuffmanDecoder,
        len: usize,
        reader: &mut R,
    ) -> Result<DeflateHuffmanDecoder, CompressionError> {
        let mut ll = Vec::new();
        while ll.len() < len {
            match try!(len_decoder.dec(reader)) {
                None => return Err(CompressionError::UnexpectedEof),
                Some(16) => {
                    let last = try!(ll.iter().last().map_or_else(
                        || Err(CompressionError::UnexpectedEof),
                        |&l| Ok(l),
                    ));
                    for _ in 0..(try!(
                        reader
                            .read_bits::<u8>(2)
                            .map_err(|_| CompressionError::UnexpectedEof)
                    ).data() + 3)
                    {
                        ll.push(last);
                    }
                }
                Some(17) => for _ in 0..(3 + try!(
                    reader
                        .read_bits::<u8>(3)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data())
                {
                    ll.push(0);
                },
                Some(18) => for _ in 0..(11 + try!(
                    reader
                        .read_bits::<u8>(7)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data())
                {
                    ll.push(0);
                },
                Some(n) => ll.push(n as u8),
            }
        }
        Ok(DeflateHuffmanDecoder::HuffmanDecoder(
            try!(
                HuffmanDecoder::new(&ll, Self::SEARCH_TAB_LEN,)
                    .map_err(|_| CompressionError::DataError)
            ),
            false,
        ))
    }

    fn init_block<R: BitRead<Right>>(
        &mut self,
        reader: &mut R,
    ) -> Result<(), CompressionError> {
        self.is_final = try!(
            reader
                .read_bits::<u8>(1)
                .map_err(|_| CompressionError::UnexpectedEof)
        ).data() == 1;
        match try!(
            reader
                .read_bits::<u8>(2)
                .map_err(|_| CompressionError::UnexpectedEof)
        ).data()
        {
            // 無圧縮
            0 => {
                reader.skip_to_next_byte();
                let block_len = try!(
                    reader
                        .read_bits(16)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data();
                let block_len_checksum = try!(
                    reader
                        .read_bits::<u32>(16)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data();
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
                        try!(
                            HuffmanDecoder::new(
                                &fix_symbol_table(),
                                Self::SEARCH_TAB_LEN,
                            ).map_err(|_| CompressionError::DataError)
                        ),
                        false,
                    ));
                self.offset_decoder =
                    Some(DeflateHuffmanDecoder::HuffmanDecoder(
                        try!(
                            HuffmanDecoder::new(
                                fix_offset_table(),
                                Self::SEARCH_TAB_LEN,
                            ).map_err(|_| CompressionError::DataError)
                        ),
                        false,
                    ));
            }
            // カスタムハフマン
            2 => {
                // リテラル/長さ符号の個数
                let hlit = try!(
                    reader
                        .read_bits::<u16>(5)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data() + 257;
                // 距離符号の個数
                let hdist = try!(
                    reader
                        .read_bits::<u16>(5)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data() + 1;
                // 長さ符号の個数
                let hclen = try!(
                    reader
                        .read_bits::<u32>(4)
                        .map_err(|_| CompressionError::UnexpectedEof)
                ).data() + 4;
                let mut lt = try!(self.dec_len_tree(hclen, reader));
                self.symbol_decoder = Some(try!(self.dec_huff_tree(
                    &mut lt,
                    hlit as usize,
                    reader
                )));
                self.offset_decoder = Some(try!(self.dec_huff_tree(
                    &mut lt,
                    hdist as usize,
                    reader
                )));
            }
            // ありえない
            _ => unreachable!(),
        }
        Ok(())
    }
}

impl<R> Decoder<R> for DeflaterInner
where
    R: BitRead<Right>,
{
    type Error = CompressionError;
    type Output = LzssCode;

    fn next(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<LzssCode>, CompressionError> {
        loop {
            if self.symbol_decoder
                .as_ref()
                .map_or_else(|| true, |x| x.end())
            {
                if self.is_final {
                    return Ok(None);
                }
                try!(self.init_block(reader));
            } else if let Some(sym) = try!(
                self.symbol_decoder
                    .as_mut()
                    .unwrap()
                    .dec(reader)
            ) {
                if sym <= 255 {
                    return Ok(Some(LzssCode::Symbol(sym as u8)));
                } else {
                    let len_index = (sym - 257) as usize;
                    let extbits = (&self.len_tab).ext_bits(len_index);
                    let len =
                        (self.len_tab.convert_back(
                            len_index,
                            if extbits != 0 {
                                try!(reader.read_bits(extbits).map_err(|_| {
                                    CompressionError::UnexpectedEof
                                })).data()
                            } else {
                                0
                            },
                        ) + 3) as usize;
                    let off_index = try!(
                        try!(
                            self.offset_decoder
                                .as_mut()
                                .unwrap()
                                .dec(reader)
                        ).ok_or_else(|| CompressionError::UnexpectedEof)
                    ) as usize;
                    let off_extbits = (&self.offset_tab).ext_bits(off_index);
                    let pos =
                        self.offset_tab.convert_back(
                            off_index,
                            if off_extbits != 0 {
                                try!(reader.read_bits(off_extbits).map_err(
                                    |_| CompressionError::UnexpectedEof
                                )).data()
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

pub struct Deflater {
    inner: DeflaterInner,
    lzss_decoder: LzssDecoder,
}

impl Default for Deflater {
    fn default() -> Self {
        Self::new()
    }
}

impl Deflater {
    const MAX_BLOCK_SIZE: usize = 0x1_0000;

    pub fn new() -> Self {
        Self {
            lzss_decoder: LzssDecoder::new(Self::MAX_BLOCK_SIZE),
            inner: DeflaterInner::new(),
        }
    }

    pub fn with_dict(dict: &[u8]) -> Self {
        Self {
            lzss_decoder: LzssDecoder::with_dict(Self::MAX_BLOCK_SIZE, dict),
            inner: DeflaterInner::new(),
        }
    }
}

impl<R> Decoder<R> for Deflater
where
    R: BitRead<Right>,
{
    type Error = CompressionError;
    type Output = u8;

    fn next(&mut self, iter: &mut R) -> Result<Option<u8>, Self::Error> {
        self.lzss_decoder
            .next(&mut self.inner.iter(iter))
    }
}
