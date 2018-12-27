//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use action::Action;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::collections::vec_deque::VecDeque;
use bitio::direction::Direction;
use bitio::direction::right::Right;
use bitio::small_bit_vec::SmallBitVec;
use bitio::writer::BitWriter;
use cbuffer::CircularBuffer;
use core::cmp::{self, Ordering};
use deflate::{fix_offset_table, fix_symbol_table, gen_len_tab, gen_off_tab,
              CodeTable};
use error::CompressionError;
use huffman::cano_huff_table::make_table;
use huffman::encoder::HuffmanEncoder;
use lzss::LzssCode;
use lzss::encoder::LzssEncoder;
#[cfg(feature = "std")]
use std::collections::vec_deque::VecDeque;
use traits::encoder::Encoder;

fn lzss_comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
    match (lhs, rhs) {
        (
            LzssCode::Reference {
                len: llen,
                pos: lpos,
            },
            LzssCode::Reference {
                len: rlen,
                pos: rpos,
            },
        ) => ((llen << 3) + lpos)
            .cmp(&((rlen << 3) + rpos))
            .reverse(),
        (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
        (_, LzssCode::Symbol(_)) => Ordering::Greater,
        (LzssCode::Symbol(_), _) => Ordering::Less,
    }
}

#[derive(Debug, Eq, PartialEq)]
enum DeflateLzssCode {
    Symbol(u8),
    Reference {
        len: u16,
        len_sub: SmallBitVec<u16>,
        pos: u8,
        pos_sub: SmallBitVec<u16>,
    },
}

impl DeflateLzssCode {
    pub fn from_with_codetab(
        source: &LzssCode,
        len_tab: &CodeTable,
        offset_tab: &CodeTable,
    ) -> Self {
        match *source {
            LzssCode::Symbol(s) => DeflateLzssCode::Symbol(s),
            LzssCode::Reference { len, pos } => {
                let l = len_tab.convert(len as u16 - 3);
                let o = offset_tab.convert(pos as u16);
                DeflateLzssCode::Reference {
                    len: u16::from(l.0) + 257,
                    len_sub: l.1,
                    pos: o.0,
                    pos_sub: o.1,
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum InflateBitVec {
    BitVec(SmallBitVec<u16>),
    Byte(u8),
    Flush,
}

pub struct Inflater {
    inner: InflaterInner,
    lzss: LzssEncoder<fn(LzssCode, LzssCode) -> Ordering>,
    writer: BitWriter<Right>,

    queue: VecDeque<InflateBitVec>,
    finished: bool,

    bitbuf: u16,
    bitbuflen: usize,
    bit_finished: bool,
}

impl Default for Inflater {
    fn default() -> Self {
        Self::new()
    }
}

impl Inflater {
    const LZSS_MIN_MATCH: usize = 3;
    const LZSS_MAX_MATCH: usize = 258;
    const LZSS_LAZY_LEVEL: usize = 3;

    pub fn new() -> Self {
        Self {
            inner: InflaterInner::new(),
            lzss: LzssEncoder::new(
                lzss_comparison,
                0x8000,
                Self::LZSS_MAX_MATCH,
                Self::LZSS_MIN_MATCH,
                Self::LZSS_LAZY_LEVEL,
            ),
            writer: BitWriter::new(),
            queue: VecDeque::new(),
            finished: false,
            bitbuf: 0,
            bitbuflen: 0,
            bit_finished: false,
        }
    }

    pub fn with_dict(dict: &[u8]) -> Self {
        Self {
            inner: InflaterInner::with_dict(dict),
            lzss: LzssEncoder::with_dict(
                lzss_comparison,
                0x8000,
                Self::LZSS_MAX_MATCH,
                Self::LZSS_MIN_MATCH,
                Self::LZSS_LAZY_LEVEL,
                dict,
            ),
            writer: BitWriter::new(),
            queue: VecDeque::new(),
            finished: false,
            bitbuf: 0,
            bitbuflen: 0,
            bit_finished: false,
        }
    }

    fn next_bits<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: Action,
    ) -> Option<Result<InflateBitVec, CompressionError>> {
        while self.queue.is_empty() {
            match self.lzss.next(iter, &action) {
                Some(ref s) => {
                    if let Err(e) = self.inner.next(s, &mut self.queue) {
                        return Some(Err(e));
                    }
                }
                None => {
                    if self.finished {
                        self.finished = false;
                        return None;
                    } else {
                        match action {
                            Action::Flush => {
                                if let Err(e) =
                                    self.inner.flush(&mut self.queue)
                                {
                                    return Some(Err(e));
                                }
                            }
                            Action::Finish => {
                                if let Err(e) =
                                    self.inner.finish(&mut self.queue)
                                {
                                    return Some(Err(e));
                                }
                            }
                            _ => {}
                        }
                        self.finished = true;
                    }
                }
            }
        }
        self.queue.pop_front().map(Ok)
    }
}

impl Encoder for Inflater {
    type Error = CompressionError;
    fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: &Action,
    ) -> Option<Result<u8, CompressionError>> {
        while self.bitbuflen == 0 {
            let s = match self.next_bits(iter, *action) {
                Some(Err(e)) => return Some(Err(e)),
                Some(Ok(InflateBitVec::BitVec(ref s))) => {
                    self.writer.write_bits(s)
                }
                Some(Ok(InflateBitVec::Byte(s))) => return Some(Ok(s)),
                Some(Ok(InflateBitVec::Flush)) => self.writer
                    .flush::<u16>()
                    .unwrap_or_else(|| (0, 0)),
                None => {
                    if self.bit_finished {
                        self.bit_finished = false;
                        return None;
                    } else {
                        match *action {
                            Action::Finish | Action::Flush => {
                                self.bit_finished = true;
                                match self.writer.flush::<u16>() {
                                    Some((x, y)) if y != 0 => (x, y),
                                    _ => return None,
                                }
                            }
                            _ => {
                                return None;
                            }
                        }
                    }
                }
            };
            self.bitbuf = s.0;
            self.bitbuflen = s.1;
        }

        let ret = Right::convert(self.bitbuf, 16, 8) as u8;

        self.bitbuf = Right::forward(self.bitbuf, 8);
        self.bitbuflen -= 1;
        Some(Ok(ret))
    }
}

struct InflaterInner {
    len_tab: CodeTable,
    offset_tab: CodeTable,

    block_buf: Vec<DeflateLzssCode>,
    decompress_len: usize,
    symbol_freq: Vec<usize>,
    offset_freq: Vec<usize>,
    nocomp_buf: CircularBuffer<u8>,
    finished: bool,
}

impl InflaterInner {
    const MAX_BLOCK_SIZE: usize = 0xFFFF;
    const SIZE_OF_SYMBOL_FREQ_BUF: usize = 257 + 29;
    const SIZE_OF_OFFSET_FREQ_BUF: usize = 30;

    fn init_block(&mut self) {
        self.block_buf = Vec::with_capacity(Self::MAX_BLOCK_SIZE);
        self.symbol_freq = vec![0; Self::SIZE_OF_SYMBOL_FREQ_BUF];
        self.offset_freq = vec![0; Self::SIZE_OF_OFFSET_FREQ_BUF];
        self.symbol_freq[256] = 1;
    }

    pub fn new() -> Self {
        let mut symbol_freq = vec![0; Self::SIZE_OF_SYMBOL_FREQ_BUF];
        symbol_freq[256] = 1;
        Self {
            len_tab: gen_len_tab(),
            offset_tab: gen_off_tab(),
            symbol_freq,
            block_buf: Vec::with_capacity(Self::MAX_BLOCK_SIZE),
            offset_freq: vec![0; Self::SIZE_OF_OFFSET_FREQ_BUF],
            decompress_len: 0,
            nocomp_buf: CircularBuffer::new(Self::MAX_BLOCK_SIZE),
            finished: false,
        }
    }

    pub fn with_dict(dict: &[u8]) -> Self {
        let mut symbol_freq = vec![0; Self::SIZE_OF_SYMBOL_FREQ_BUF];
        symbol_freq[256] = 1;
        let mut nocomp_buf = CircularBuffer::new(Self::MAX_BLOCK_SIZE);
        nocomp_buf.append(dict);
        Self {
            len_tab: gen_len_tab(),
            offset_tab: gen_off_tab(),
            symbol_freq,
            block_buf: Vec::with_capacity(Self::MAX_BLOCK_SIZE),
            offset_freq: vec![0; Self::SIZE_OF_OFFSET_FREQ_BUF],
            decompress_len: 0,
            nocomp_buf,
            finished: false,
        }
    }

    fn enc_tab_to_freq(enc_tab: &[u8]) -> (Vec<(u8, u16)>, Vec<u16>) {
        let mut freq = vec![0; 19];
        let mut list = Vec::new();
        let mut old = 255;
        let mut len = 0;
        for &d in enc_tab.iter().chain(vec![255_u8].iter()) {
            if old != d {
                if old == 0 {
                    if len >= 11 {
                        freq[18] += 1;
                        list.push((18, len - 11));
                    } else if len >= 3 {
                        freq[17] += 1;
                        list.push((17, len - 3));
                    } else {
                        for _ in 0..len {
                            list.push((0, 0));
                        }
                        freq[0] += len;
                    }
                } else if len >= 3 {
                    freq[16] += 1;
                    list.push((16, len - 3));
                } else if len > 0 {
                    for _ in 0..len {
                        list.push((old, 0));
                    }
                    freq[old as usize] += len;
                }

                if d != 0 && d != 255 {
                    list.push((d, 0));
                    freq[d as usize] += 1;
                    len = 0;
                } else {
                    len = 1;
                }
                old = d;
            } else {
                len += 1;
                if old == 0 && len == 138 {
                    freq[18] += 1;
                    list.push((18, 127));
                    len = 0;
                } else if old != 0 && len == 6 {
                    freq[16] += 1;
                    list.push((16, 3));
                    len = 0;
                }
            }
        }
        (list, freq)
    }

    fn conv_tab(list: &[(u8, u16)], enc_tab: &[u8]) -> Vec<SmallBitVec<u16>> {
        let mut ret = Vec::new();
        let enc = HuffmanEncoder::<Right, _>::new(enc_tab);
        for &(ref s, e) in list.iter() {
            ret.push(enc.enc(s).unwrap());
            match *s {
                16 => ret.push(SmallBitVec::new(e, 2)),
                17 => ret.push(SmallBitVec::new(e, 3)),
                18 => ret.push(SmallBitVec::new(e, 7)),
                _ => {}
            }
        }
        ret
    }

    fn create_custom_huffman_table(
        sym_enc_tab: &[u8],
        off_enc_tab: &[u8],
    ) -> Vec<SmallBitVec<u16>> {
        let (symlist, symfreq) = Self::enc_tab_to_freq(sym_enc_tab);
        let (offlist, offfreq) = Self::enc_tab_to_freq(off_enc_tab);
        let lenfreq = symfreq
            .iter()
            .zip(offfreq.iter())
            .map(|(&x, &y)| (x + y) as usize)
            .collect::<Vec<_>>();
        let len_enc_tab = make_table(&lenfreq, 7);

        let len_map = [
            3, 17, 15, 13, 11, 9, 7, 5, 4, 6, 8, 10, 12, 14, 16, 18, 0, 1, 2
        ];
        // let len_map = [16, 17, 18, 0, 8, 7, 9, 6, 10,
        //                5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
        let mut len_tab = vec![0; 19];
        let mut len_count = 3;

        for (&d, &i) in len_enc_tab.iter().zip(len_map.iter()) {
            if d != 0 {
                len_tab[i] = d;
                len_count = cmp::max(len_count, i);
            }
        }
        let hlit = sym_enc_tab
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s != 0)
            .last()
            .unwrap()
            .0 - 256;
        let hdist = off_enc_tab
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s != 0)
            .last()
            .unwrap_or_else(|| (0, &0))
            .0;
        let hclen = len_count - 3;

        let mut ret = Vec::new();
        ret.push(SmallBitVec::new(2, 2)); // custom huffman signature
        ret.push(SmallBitVec::new(hlit as u16, 5));
        ret.push(SmallBitVec::new(hdist as u16, 5));
        ret.push(SmallBitVec::new(hclen as u16, 4));
        for &d in len_tab.iter().take(len_count + 1) {
            ret.push(SmallBitVec::new(u16::from(d), 3));
        }

        ret.append(&mut Self::conv_tab(&symlist, &len_enc_tab));
        ret.append(&mut Self::conv_tab(&offlist, &len_enc_tab));
        ret
    }

    fn write_block(
        &mut self,
        is_final: bool,
        queue: &mut VecDeque<InflateBitVec>,
    ) -> Result<(), CompressionError> {
        queue.push_back(InflateBitVec::BitVec(SmallBitVec::new(
            if is_final {
                self.finished = true;
                1
            } else {
                0
            },
            1,
        )));

        let sym_enc_tab = make_table(&self.symbol_freq, 15);
        let off_enc_tab = make_table(&self.offset_freq, 15);

        // カスタムハフマンを使用した時のビット数を計算
        let custom_huffman_header =
            Self::create_custom_huffman_table(&sym_enc_tab, &off_enc_tab);

        let custom_haffman_size = self.cals_comp_len(&sym_enc_tab, &off_enc_tab)
            + custom_huffman_header
                .iter()
                .fold(0, |s, v| v.len() as u64 + s);

        let fix_sym_enc_tab = fix_symbol_table();
        let fix_off_enc_tab = fix_offset_table();

        // 固定ハフマンを使用した時のビット数を計算
        let fixed_haffman_size =
            self.cals_comp_len(&fix_sym_enc_tab, fix_off_enc_tab) + 2;

        // 無圧縮時のビット数
        let original_size = ((self.decompress_len as u64) << 3) + 2 + 16 + 16;

        if original_size <= custom_haffman_size
            && original_size <= fixed_haffman_size
        {
            // 無圧縮時
            queue.push_back(InflateBitVec::BitVec(SmallBitVec::new(0, 2)));
            queue.push_back(InflateBitVec::Flush);
            queue.push_back(InflateBitVec::BitVec(SmallBitVec::new(
                self.decompress_len as u16,
                16,
            )));
            queue.push_back(InflateBitVec::BitVec(SmallBitVec::new(
                self.decompress_len as u16 ^ 0xFFFF,
                16,
            )));
            for i in 1..=self.decompress_len {
                let d = self.nocomp_buf[self.decompress_len - i];
                queue.push_back(InflateBitVec::Byte(d));
            }
        } else {
            let (sym_enc, off_enc) = if fixed_haffman_size
                <= custom_haffman_size
            {
                // 固定ハフマン使用
                queue.push_back(InflateBitVec::BitVec(SmallBitVec::new(1, 2)));
                (
                    HuffmanEncoder::<Right, u16>::new(&fix_sym_enc_tab),
                    HuffmanEncoder::<Right, u16>::new(fix_off_enc_tab),
                )
            } else {
                // カスタムハフマン
                for d in custom_huffman_header {
                    queue.push_back(InflateBitVec::BitVec(d));
                }
                (
                    HuffmanEncoder::<Right, _>::new(&sym_enc_tab),
                    HuffmanEncoder::<Right, _>::new(&off_enc_tab),
                )
            };
            for b in &self.block_buf {
                match *b {
                    DeflateLzssCode::Symbol(ref s) => {
                        queue.push_back(InflateBitVec::BitVec(try!(
                            sym_enc
                                .enc(s)
                                .map_err(|_| CompressionError::Unexpected)
                        )));
                    }
                    DeflateLzssCode::Reference {
                        ref len,
                        ref len_sub,
                        ref pos,
                        ref pos_sub,
                    } => {
                        queue.push_back(InflateBitVec::BitVec(try!(
                            sym_enc
                                .enc(len)
                                .map_err(|_| CompressionError::Unexpected)
                        )));
                        queue.push_back(InflateBitVec::BitVec(len_sub.clone()));
                        queue.push_back(InflateBitVec::BitVec(try!(
                            off_enc
                                .enc(pos)
                                .map_err(|_| CompressionError::Unexpected)
                        )));
                        queue.push_back(InflateBitVec::BitVec(pos_sub.clone()));
                    }
                }
            }
            queue.push_back(InflateBitVec::BitVec(try!(
                sym_enc
                    .enc(&256)
                    .map_err(|_| CompressionError::Unexpected)
            )));
        }
        self.init_block();
        Ok(())
    }

    fn cals_comp_len(&self, sym_enc_tab: &[u8], off_enc_tab: &[u8]) -> u64 {
        sym_enc_tab
            .iter()
            .enumerate()
            .zip(self.symbol_freq.iter())
            .map(|((i, &l), &f)| {
                f as u64 * (u64::from(l) + if i >= 257 {
                    self.len_tab.ext_bits(i - 257) as u64
                } else {
                    0
                })
            })
            .sum::<u64>()
            + off_enc_tab
                .iter()
                .enumerate()
                .zip(self.offset_freq.iter())
                .map(|((i, &l), &f)| {
                    f as u64
                        * (u64::from(l) + self.offset_tab.ext_bits(i) as u64)
                })
                .sum::<u64>()
    }

    fn next(
        &mut self,
        buf: &LzssCode,
        queue: &mut VecDeque<InflateBitVec>,
    ) -> Result<(), CompressionError> {
        let next_len = if let LzssCode::Reference { len, .. } = *buf {
            len as usize
        } else {
            1
        };
        let new_len = self.decompress_len + next_len;
        if (new_len > Self::MAX_BLOCK_SIZE
            && self.decompress_len <= Self::MAX_BLOCK_SIZE
            && self.decompress_len != 0)
            || (self.block_buf.len() == Self::MAX_BLOCK_SIZE)
        {
            try!(self.write_block(false, queue));
            self.decompress_len = next_len;
        } else {
            self.decompress_len = new_len;
        }

        // lzss decode
        // 元のデータを使わずにlzssのデコードを行なっているので、
        // メモリ、計算資源ともに無駄となっている
        match *buf {
            LzssCode::Symbol(s) => {
                self.nocomp_buf.push(s);
            }
            LzssCode::Reference { len, pos } => {
                for _ in 0..len {
                    let d = self.nocomp_buf[pos];
                    self.nocomp_buf.push(d);
                }
            }
        }

        let code = DeflateLzssCode::from_with_codetab(
            buf,
            &self.len_tab,
            &self.offset_tab,
        );
        match code {
            DeflateLzssCode::Symbol(s) => {
                self.symbol_freq[s as usize] += 1;
            }
            DeflateLzssCode::Reference { len, pos, .. } => {
                self.symbol_freq[len as usize] += 1;
                self.offset_freq[pos as usize] += 1;
            }
        }

        self.block_buf.push(code);
        Ok(())
    }

    fn flush(
        &mut self,
        queue: &mut VecDeque<InflateBitVec>,
    ) -> Result<(), CompressionError> {
        if !self.finished {
            self.write_block(false, queue)
        } else {
            Ok(())
        }
    }

    fn finish(
        &mut self,
        queue: &mut VecDeque<InflateBitVec>,
    ) -> Result<(), CompressionError> {
        if !self.finished {
            self.write_block(true, queue)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use action::Action;
    use bitio::writer::BitWriteExt;
    use traits::encoder::EncodeExt;

    #[test]
    fn test_empty() {
        let mut encoder = Inflater::new();
        let ret = [].iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(ret, Ok(vec![3, 0]));
        // // BFINAL  1: final
        // SmallBitVec::new(1, 1),
        // // BTYPE   1: FixHuffman
        // SmallBitVec::new(1, 2),
        // // DATA
        // SmallBitVec::new(0, 7), // 256
    }

    #[test]
    fn test_unit() {
        let mut encoder = Inflater::new();
        let ret = b"a".iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(ret, Ok(vec![0x4B, 0x04, 0]));
        // // BFINAL  1: final
        // SmallBitVec::new(1, 1),
        // // BTYPE   1: FixHuffman
        // SmallBitVec::new(1, 2),
        // // DATA
        // SmallBitVec::new(137, 8), // 97
        // SmallBitVec::new(0, 7),   // 256
    }

    #[test]
    fn test_arr() {
        let mut encoder = Inflater::new();
        let ret = b"aaaaaaaaaaa"
            .iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(ret, Ok(vec![0x4B, 0x44, 0, 0]));

        // // BFINAL  1: final
        // SmallBitVec::new(1, 1),
        // // BTYPE   1:Fix Huffman
        // SmallBitVec::new(1, 2),
        // // data
        // SmallBitVec::new(137, 8), // 97
        // SmallBitVec::new(8, 7),   // 264
        // SmallBitVec::new(0, 0),   //
        // SmallBitVec::new(0, 5),   // 0
        // SmallBitVec::new(0, 0),   //
        // SmallBitVec::new(0, 7),   // 256
    }

    #[test]
    fn test_arr2() {
        let mut encoder = Inflater::new();
        let a = b"aabbaabbaaabbbaaabbbaabbaabb"
            .iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        let b = vec![
            // BFINAL  1: final
            SmallBitVec::new(1_u32, 1),
            // BTYPE   1:Fix Huffman
            SmallBitVec::new(1, 2),
            // data
            SmallBitVec::new(137, 8), // 97
            SmallBitVec::new(137, 8), // 97
            SmallBitVec::new(73, 8),  // 98
            SmallBitVec::new(73, 8),  // 98
            SmallBitVec::new(16, 7),  // 260
            SmallBitVec::new(0, 0),   //
            SmallBitVec::new(24, 5),  // 3
            SmallBitVec::new(0, 0),   //
            SmallBitVec::new(137, 8), // 97
            SmallBitVec::new(73, 8),  // 98
            SmallBitVec::new(8, 7),   // 264
            SmallBitVec::new(0, 0),   //
            SmallBitVec::new(4, 5),   // 4
            SmallBitVec::new(1, 1),   //
            SmallBitVec::new(16, 7),  // 260
            SmallBitVec::new(0, 0),   //
            SmallBitVec::new(24, 5),  // 3
            SmallBitVec::new(0, 0),   //
            SmallBitVec::new(0, 7),   // 256
        ].to_bytes(BitWriter::<Right>::new(), Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(a, Ok(b));
    }

    #[test]
    fn test_arr3() {
        let mut encoder = Inflater::new();
        let a = (144..256)
            .map(|x| x as u8)
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        let mut r = vec![
            // BFINAL  1: final
            SmallBitVec::new(1_u32, 1),
            // BTYPE   0:No compress
            SmallBitVec::new(0, 2),
            // FILLER
            SmallBitVec::new(0, 5),
            // LEN
            SmallBitVec::new(112, 16),
            // NLEN
            SmallBitVec::new(65_423, 16),
        ];

        // DATA
        r.append(
            &mut ((144..256)
                .map(|x| SmallBitVec::new(x, 8))
                .collect::<Vec<_>>()),
        );

        let b = r.to_bytes(BitWriter::<Right>::new(), Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(a, Ok(b));
    }

    #[test]
    fn test_arr4() {
        let mut encoder = Inflater::new();
        let a = (144..256)
            .cycle()
            .take(224)
            .map(|x| x as u8)
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        let r = vec![
            SmallBitVec::new(1_u32, 1),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(23, 5),
            SmallBitVec::new(13, 5),
            SmallBitVec::new(14, 4),
            SmallBitVec::new(2, 3),
            SmallBitVec::new(4, 3),
            SmallBitVec::new(3, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(2, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(2, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(4, 3),
            SmallBitVec::new(3, 3),
            SmallBitVec::new(127, 7),
            SmallBitVec::new(15, 4),
            SmallBitVec::new(3, 3),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(1, 2),
            SmallBitVec::new(3, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(3, 3),
            SmallBitVec::new(11, 7),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(3, 3),
            SmallBitVec::new(2, 7),
            SmallBitVec::new(7, 4),
            SmallBitVec::new(28, 7),
            SmallBitVec::new(92, 7),
            SmallBitVec::new(60, 7),
            SmallBitVec::new(124, 7),
            SmallBitVec::new(2, 7),
            SmallBitVec::new(66, 7),
            SmallBitVec::new(34, 7),
            SmallBitVec::new(98, 7),
            SmallBitVec::new(18, 7),
            SmallBitVec::new(82, 7),
            SmallBitVec::new(50, 7),
            SmallBitVec::new(114, 7),
            SmallBitVec::new(10, 7),
            SmallBitVec::new(74, 7),
            SmallBitVec::new(42, 7),
            SmallBitVec::new(106, 7),
            SmallBitVec::new(0, 6),
            SmallBitVec::new(26, 7),
            SmallBitVec::new(90, 7),
            SmallBitVec::new(58, 7),
            SmallBitVec::new(122, 7),
            SmallBitVec::new(6, 7),
            SmallBitVec::new(70, 7),
            SmallBitVec::new(38, 7),
            SmallBitVec::new(102, 7),
            SmallBitVec::new(22, 7),
            SmallBitVec::new(86, 7),
            SmallBitVec::new(54, 7),
            SmallBitVec::new(118, 7),
            SmallBitVec::new(14, 7),
            SmallBitVec::new(32, 6),
            SmallBitVec::new(78, 7),
            SmallBitVec::new(46, 7),
            SmallBitVec::new(110, 7),
            SmallBitVec::new(30, 7),
            SmallBitVec::new(94, 7),
            SmallBitVec::new(62, 7),
            SmallBitVec::new(126, 7),
            SmallBitVec::new(1, 7),
            SmallBitVec::new(65, 7),
            SmallBitVec::new(33, 7),
            SmallBitVec::new(97, 7),
            SmallBitVec::new(16, 6),
            SmallBitVec::new(17, 7),
            SmallBitVec::new(81, 7),
            SmallBitVec::new(49, 7),
            SmallBitVec::new(113, 7),
            SmallBitVec::new(9, 7),
            SmallBitVec::new(73, 7),
            SmallBitVec::new(41, 7),
            SmallBitVec::new(105, 7),
            SmallBitVec::new(25, 7),
            SmallBitVec::new(89, 7),
            SmallBitVec::new(48, 6),
            SmallBitVec::new(57, 7),
            SmallBitVec::new(121, 7),
            SmallBitVec::new(5, 7),
            SmallBitVec::new(69, 7),
            SmallBitVec::new(37, 7),
            SmallBitVec::new(8, 6),
            SmallBitVec::new(40, 6),
            SmallBitVec::new(24, 6),
            SmallBitVec::new(101, 7),
            SmallBitVec::new(21, 7),
            SmallBitVec::new(56, 6),
            SmallBitVec::new(85, 7),
            SmallBitVec::new(53, 7),
            SmallBitVec::new(117, 7),
            SmallBitVec::new(13, 7),
            SmallBitVec::new(77, 7),
            SmallBitVec::new(45, 7),
            SmallBitVec::new(109, 7),
            SmallBitVec::new(29, 7),
            SmallBitVec::new(93, 7),
            SmallBitVec::new(61, 7),
            SmallBitVec::new(125, 7),
            SmallBitVec::new(3, 7),
            SmallBitVec::new(67, 7),
            SmallBitVec::new(35, 7),
            SmallBitVec::new(99, 7),
            SmallBitVec::new(19, 7),
            SmallBitVec::new(83, 7),
            SmallBitVec::new(51, 7),
            SmallBitVec::new(115, 7),
            SmallBitVec::new(11, 7),
            SmallBitVec::new(75, 7),
            SmallBitVec::new(4, 6),
            SmallBitVec::new(43, 7),
            SmallBitVec::new(107, 7),
            SmallBitVec::new(27, 7),
            SmallBitVec::new(91, 7),
            SmallBitVec::new(59, 7),
            SmallBitVec::new(123, 7),
            SmallBitVec::new(7, 7),
            SmallBitVec::new(71, 7),
            SmallBitVec::new(39, 7),
            SmallBitVec::new(103, 7),
            SmallBitVec::new(23, 7),
            SmallBitVec::new(87, 7),
            SmallBitVec::new(55, 7),
            SmallBitVec::new(119, 7),
            SmallBitVec::new(15, 7),
            SmallBitVec::new(79, 7),
            SmallBitVec::new(47, 7),
            SmallBitVec::new(111, 7),
            SmallBitVec::new(31, 7),
            SmallBitVec::new(36, 6),
            SmallBitVec::new(20, 6),
            SmallBitVec::new(95, 7),
            SmallBitVec::new(52, 6),
            SmallBitVec::new(63, 7),
            SmallBitVec::new(12, 6),
            SmallBitVec::new(127, 7),
            SmallBitVec::new(13, 4),
            SmallBitVec::new(0, 1),
            SmallBitVec::new(15, 5),
            SmallBitVec::new(44, 6),
        ];

        let b = r.to_bytes(BitWriter::<Right>::new(), Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(a, Ok(b));
    }

    #[test]
    fn test_defaltelzsscode() {
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference { len: 3, pos: 0 },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 257,
                len_sub: SmallBitVec::new(0, 0),
                pos: 0,
                pos_sub: SmallBitVec::new(0, 0),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference { len: 4, pos: 1 },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 258,
                len_sub: SmallBitVec::new(0, 0),
                pos: 1,
                pos_sub: SmallBitVec::new(0, 0),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference { len: 5, pos: 2 },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 259,
                len_sub: SmallBitVec::new(0, 0),
                pos: 2,
                pos_sub: SmallBitVec::new(0, 0),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference { len: 6, pos: 3 },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 260,
                len_sub: SmallBitVec::new(0, 0),
                pos: 3,
                pos_sub: SmallBitVec::new(0, 0),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference { len: 7, pos: 4 },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 261,
                len_sub: SmallBitVec::new(0, 0),
                pos: 4,
                pos_sub: SmallBitVec::new(0, 1),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference { len: 8, pos: 5 },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 262,
                len_sub: SmallBitVec::new(0, 0),
                pos: 4,
                pos_sub: SmallBitVec::new(1, 1),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference { len: 9, pos: 6 },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 263,
                len_sub: SmallBitVec::new(0, 0),
                pos: 5,
                pos_sub: SmallBitVec::new(0, 1),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference {
                    len: 10,
                    pos: 7
                },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 264,
                len_sub: SmallBitVec::new(0, 0),
                pos: 5,
                pos_sub: SmallBitVec::new(1, 1),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference {
                    len: 11,
                    pos: 8
                },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 265,
                len_sub: SmallBitVec::new(0, 1),
                pos: 6,
                pos_sub: SmallBitVec::new(0, 2),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference {
                    len: 12,
                    pos: 9
                },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 265,
                len_sub: SmallBitVec::new(1, 1),
                pos: 6,
                pos_sub: SmallBitVec::new(1, 2),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference {
                    len: 13,
                    pos: 10
                },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 266,
                len_sub: SmallBitVec::new(0, 1),
                pos: 6,
                pos_sub: SmallBitVec::new(2, 2),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference {
                    len: 257,
                    pos: 24_576,
                },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 284,
                len_sub: SmallBitVec::new(30, 5),
                pos: 29,
                pos_sub: SmallBitVec::new(0, 13),
            }
        );
        assert_eq!(
            DeflateLzssCode::from_with_codetab(
                &LzssCode::Reference {
                    len: 258,
                    pos: 0x7FFF,
                },
                &gen_len_tab(),
                &gen_off_tab(),
            ),
            DeflateLzssCode::Reference {
                len: 285,
                len_sub: SmallBitVec::new(0, 0),
                pos: 29,
                pos_sub: SmallBitVec::new(0x1FFF, 13),
            }
        );
    }
}
