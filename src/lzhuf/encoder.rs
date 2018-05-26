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
use alloc::vec_deque::VecDeque;
use bitio::direction::Direction;
use bitio::direction::left::Left;
use bitio::small_bit_vec::SmallBitVec;
use bitio::writer::BitWriter;
use core::cmp::{self, Ordering};
use error::CompressionError;
use huffman::cano_huff_table::make_table;
use huffman::encoder::HuffmanEncoder;
use lzhuf::{LzhufMethod, LZSS_MIN_MATCH};
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

enum LzhufHuffmanEncoder {
    HuffmanEncoder(HuffmanEncoder<Left, u16>),
    Default,
}

impl LzhufHuffmanEncoder {
    pub fn new(symb_len: &[u8]) -> Self {
        let symbs = symb_len
            .iter()
            .enumerate()
            .filter(|&(_, &t)| t != 0)
            .collect::<Vec<_>>();
        if symbs.len() <= 1 {
            LzhufHuffmanEncoder::Default
        } else {
            LzhufHuffmanEncoder::HuffmanEncoder(HuffmanEncoder::new(symb_len))
        }
    }

    pub fn enc(
        &mut self,
        data: &u16,
    ) -> Result<Option<SmallBitVec<u16>>, CompressionError> {
        match *self {
            LzhufHuffmanEncoder::HuffmanEncoder(ref mut lhe) => lhe.enc(data)
                .map(Some)
                .map_err(|_| CompressionError::DataError),
            LzhufHuffmanEncoder::Default => Ok(None),
        }
    }
}

const MIN_MATCH: u16 = 3;

#[derive(Debug, Eq, PartialEq)]
enum LzhufLzssCode {
    Symbol(u8),
    Reference {
        len: u16,
        pos_offset: u16,
        pos_sublen: u16,
    },
}

impl<'a> From<&'a LzssCode> for LzhufLzssCode {
    fn from(data: &LzssCode) -> Self {
        match *data {
            LzssCode::Symbol(s) => LzhufLzssCode::Symbol(s),
            LzssCode::Reference { len, pos } => {
                let po = (pos + 1).next_power_of_two();
                LzhufLzssCode::Reference {
                    len: len as u16 + 256 - MIN_MATCH,
                    pos_offset: po.trailing_zeros() as u16,
                    pos_sublen: (pos - (po >> 1)) as u16,
                }
            }
        }
    }
}

pub struct LzhufEncoder {
    inner: LzhufEncoderInner,
    lzss: LzssEncoder<fn(LzssCode, LzssCode) -> Ordering>,
    writer: BitWriter<Left>,
    queue: VecDeque<SmallBitVec<u16>>,
    finished: bool,
    bitbuf: u16,
    bitbuflen: usize,
    bit_finished: bool,
}

impl LzhufEncoder {
    const LZSS_MAX_MATCH: usize = 256;
    const LZSS_LAZY_LEVEL: usize = 3;
    const LZHUF_MAX_BLOCK_LENGTH: usize = 0xFFFF;

    pub fn new(method: &LzhufMethod) -> Self {
        let dic_len = 1 << method.dictionary_bits();
        Self {
            inner: LzhufEncoderInner::new(
                Self::LZHUF_MAX_BLOCK_LENGTH,
                method.offset_bits(),
                Self::LZSS_MAX_MATCH,
            ),
            lzss: LzssEncoder::new(
                lzss_comparison,
                dic_len,
                Self::LZSS_MAX_MATCH,
                LZSS_MIN_MATCH,
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

    fn next_bits<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: &Action,
    ) -> Option<Result<SmallBitVec<u16>, CompressionError>> {
        while self.queue.is_empty() {
            match self.lzss.next(iter, action) {
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
                        match *action {
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

impl Encoder for LzhufEncoder {
    type Error = CompressionError;
    fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: &Action,
    ) -> Option<Result<u8, CompressionError>> {
        while self.bitbuflen == 0 {
            let s = match self.next_bits(iter, action) {
                Some(Err(e)) => return Some(Err(e)),
                Some(Ok(ref s)) => self.writer.write_bits(s),
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

        let ret = Left::convert(self.bitbuf, 16, 8) as u8;

        self.bitbuf = Left::forward(self.bitbuf, 8);
        self.bitbuflen -= 1;
        Some(Ok(ret))
    }
}

struct LzhufEncoderInner {
    max_block_len: usize,
    offset_tab_len: usize,
    block_buf: Vec<LzhufLzssCode>,
    symbol_freq: Vec<usize>,
    offset_freq: Vec<usize>,
    size_of_symbol_freq_buf: usize,
    size_of_offset_freq_buf: usize,
}

impl LzhufEncoderInner {
    // Length Table
    const TBIT_SIZE: usize = 5;
    // Symbol Table
    const CBIT_SIZE: usize = 9;

    fn init_block(&mut self) {
        self.block_buf = Vec::with_capacity(self.max_block_len);
        self.symbol_freq = vec![0; self.size_of_symbol_freq_buf];
        self.offset_freq = vec![0; self.size_of_offset_freq_buf];
    }

    pub fn new(
        max_block_len: usize,
        offset_tab_len: usize,
        max_match: usize,
    ) -> Self {
        let mbl_npot = max_block_len.next_power_of_two() >> 1;
        let size_of_offset_freq_buf =
            cmp::max(max_block_len - mbl_npot, mbl_npot - 1);
        let size_of_symbol_freq_buf = max_match + 256 - MIN_MATCH as usize + 1;
        Self {
            max_block_len,
            offset_tab_len,
            size_of_symbol_freq_buf,
            size_of_offset_freq_buf,
            block_buf: Vec::with_capacity(max_block_len),
            symbol_freq: vec![0; size_of_symbol_freq_buf],
            offset_freq: vec![0; size_of_offset_freq_buf],
        }
    }

    fn enc_len(&mut self, len: u16) -> Vec<SmallBitVec<u16>> {
        if len >= 7 {
            let mut ret = vec![SmallBitVec::new(7, 3)];
            for _ in 7..len {
                ret.push(SmallBitVec::new(1, 1));
            }
            ret.push(SmallBitVec::new(0, 1));
            ret
        } else {
            vec![SmallBitVec::new(len, 3)]
        }
    }

    fn write_symb_tab(
        &mut self,
        symb_enc_tab: &[u8],
    ) -> Result<Vec<SmallBitVec<u16>>, CompressionError> {
        // write symbol table
        let symb_enc_tab_map = symb_enc_tab
            .iter()
            .enumerate()
            .filter(|&(_, &t)| t != 0)
            .collect::<Vec<_>>();
        let mut ret = Vec::new();
        if symb_enc_tab.is_empty() {
            ret.push(SmallBitVec::new(0, Self::TBIT_SIZE));
            ret.push(SmallBitVec::new(0, Self::TBIT_SIZE));
            ret.push(SmallBitVec::new(0, Self::CBIT_SIZE));
            ret.push(SmallBitVec::new(0, Self::CBIT_SIZE));
        } else if symb_enc_tab_map.len() == 1 {
            ret.push(SmallBitVec::new(0, Self::TBIT_SIZE));
            ret.push(SmallBitVec::new(0, Self::TBIT_SIZE));
            ret.push(SmallBitVec::new(0, Self::CBIT_SIZE));
            ret.push(SmallBitVec::new(
                symb_enc_tab_map[0].0 as u16,
                Self::CBIT_SIZE,
            ));
        } else {
            // generate length encoder
            let mut sym_list = Vec::new();
            let mut freq = vec![0; 19];

            if !symb_enc_tab.is_empty() {
                let mut i = 0;
                for (sym_ind, &sym_len) in symb_enc_tab
                    .iter()
                    .enumerate()
                    .filter(|&(_, &t)| t != 0)
                {
                    let gap = sym_ind - i;
                    i = sym_ind + 1;
                    if gap > 19 {
                        sym_list.push((2, gap - 20));
                        freq[2] += 1;
                    } else if gap == 19 {
                        sym_list.push((1, 15));
                        sym_list.push((0, 0));
                        freq[1] += 1;
                        freq[0] += 1;
                    } else if gap > 2 {
                        sym_list.push((1, gap - 3));
                        freq[1] += 1;
                    } else if gap > 0 {
                        sym_list.push((0, 0));
                        if gap == 2 {
                            sym_list.push((0, 0));
                        }
                        freq[0] += gap;
                    }
                    sym_list.push((3, sym_len as usize + 2));
                    freq[sym_len as usize + 2] += 1;
                }
            }

            // write length and symbol table
            let len_enc_tab = make_table(&freq, 16);
            let len_enc_tab_map = len_enc_tab
                .iter()
                .enumerate()
                .filter(|&(_, &t)| t != 0)
                .collect::<Vec<_>>();

            let mut len_enc = if len_enc_tab_map.is_empty() {
                unreachable!();
            } else if len_enc_tab_map.len() == 1 {
                ret.push(SmallBitVec::new(0, Self::TBIT_SIZE));
                ret.push(SmallBitVec::new(
                    len_enc_tab_map[0].0 as u16,
                    Self::TBIT_SIZE,
                ));
                LzhufHuffmanEncoder::Default
            } else {
                let mut i = 0;
                ret.push(SmallBitVec::new(
                    len_enc_tab_map.last().unwrap().0 as u16 + 1,
                    Self::TBIT_SIZE,
                ));

                for (len_ind, &len_len) in len_enc_tab_map {
                    while len_ind >= i {
                        if i == 3 {
                            let skip = if len_ind > 6 {
                                3
                            } else {
                                len_ind - 3
                            };
                            ret.push(SmallBitVec::new(skip as u16, 2));
                            i += skip;
                        }

                        if len_ind != i {
                            ret.push(SmallBitVec::new(0, 3));
                        } else {
                            ret.append(&mut self.enc_len(u16::from(len_len)));
                        }
                        i += 1;
                    }
                }
                LzhufHuffmanEncoder::new(&len_enc_tab)
            };

            ret.push(SmallBitVec::new(
                symb_enc_tab_map.last().unwrap().0 as u16 + 1,
                Self::CBIT_SIZE,
            ));
            for (s, l) in sym_list {
                match s {
                    0 => {
                        if let Some(e) = len_enc.enc(&0)? {
                            ret.push(e)
                        }
                    }
                    1 => {
                        if let Some(e) = len_enc.enc(&1)? {
                            ret.push(e)
                        };
                        ret.push(SmallBitVec::new(l as u16, 4));
                    }
                    2 => {
                        if let Some(e) = len_enc.enc(&2)? {
                            ret.push(e)
                        };
                        ret.push(SmallBitVec::new(l as u16, 9));
                    }
                    _ => {
                        if let Some(e) = len_enc.enc(&(l as u16))? {
                            ret.push(e)
                        }
                    }
                }
            }
        }
        Ok(ret)
    }

    fn write_offset_tab(
        &mut self,
        off_enc_tab: &[u8],
        pbit_len: usize,
    ) -> Vec<SmallBitVec<u16>> {
        // write length and symbol table
        let off_enc_tab_map = off_enc_tab
            .iter()
            .enumerate()
            .filter(|&(_, &t)| t != 0)
            .collect::<Vec<_>>();

        let mut ret = Vec::new();
        if off_enc_tab_map.is_empty() {
            ret.push(SmallBitVec::new(0, pbit_len));
            ret.push(SmallBitVec::new(0, pbit_len));
        } else if off_enc_tab_map.len() == 1 {
            ret.push(SmallBitVec::new(0, pbit_len));
            ret.push(SmallBitVec::new(
                off_enc_tab_map[0].0 as u16,
                pbit_len,
            ));
        } else {
            let mut i = 0;
            ret.push(SmallBitVec::new(
                off_enc_tab_map.last().unwrap().0 as u16 + 1,
                pbit_len,
            ));
            for (symb, &len) in off_enc_tab_map {
                while symb >= i {
                    if symb != i {
                        ret.push(SmallBitVec::new(0, 3));
                    } else {
                        ret.append(&mut self.enc_len(u16::from(len)));
                    }
                    i += 1;
                }
            }
        }
        ret
    }

    fn write_block(
        &mut self,
        queue: &mut VecDeque<SmallBitVec<u16>>,
    ) -> Result<(), CompressionError> {
        let sym_enc_tab = make_table(&self.symbol_freq, 16);
        let off_enc_tab = make_table(&self.offset_freq, 16);
        let mut sym_enc = LzhufHuffmanEncoder::new(&sym_enc_tab);
        let mut off_enc = LzhufHuffmanEncoder::new(&off_enc_tab);

        // write block length
        queue.push_back(SmallBitVec::new(
            self.block_buf.len() as u16,
            16,
        ));

        queue.extend(self.write_symb_tab(&sym_enc_tab)?);
        let l = self.offset_tab_len;
        queue.extend(self.write_offset_tab(&off_enc_tab, l));

        for d in &self.block_buf {
            match *d {
                LzhufLzssCode::Symbol(s) => {
                    if let Some(e) = sym_enc.enc(&u16::from(s))? {
                        queue.push_back(e)
                    }
                }
                LzhufLzssCode::Reference {
                    ref len,
                    ref pos_offset,
                    pos_sublen,
                } => {
                    if let Some(e) = sym_enc.enc(len)? {
                        queue.push_back(e)
                    }
                    if let Some(e) = off_enc.enc(pos_offset)? {
                        queue.push_back(e)
                    }
                    if *pos_offset > 1 {
                        queue.push_back(SmallBitVec::new(
                            pos_sublen,
                            *pos_offset as usize - 1,
                        ));
                    }
                }
            }
        }

        self.init_block();
        Ok(())
    }

    fn next(
        &mut self,
        buf: &LzssCode,
        queue: &mut VecDeque<SmallBitVec<u16>>,
    ) -> Result<(), CompressionError> {
        let code = LzhufLzssCode::from(buf);
        match code {
            LzhufLzssCode::Symbol(s) => {
                self.symbol_freq[s as usize] += 1;
            }
            LzhufLzssCode::Reference {
                len,
                pos_offset,
                ..
            } => {
                self.symbol_freq[len as usize] += 1;
                self.offset_freq[pos_offset as usize] += 1;
            }
        }

        self.block_buf.push(code);

        if self.block_buf.len() == self.max_block_len {
            self.write_block(queue)?;
        }

        Ok(())
    }

    fn flush(
        &mut self,
        queue: &mut VecDeque<SmallBitVec<u16>>,
    ) -> Result<(), CompressionError> {
        if !self.block_buf.is_empty() {
            self.write_block(queue)
        } else {
            Ok(())
        }
    }

    fn finish(
        &mut self,
        queue: &mut VecDeque<SmallBitVec<u16>>,
    ) -> Result<(), CompressionError> {
        if !self.block_buf.is_empty() {
            self.write_block(queue)
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
    fn test_arr() {
        let mut encoder = LzhufEncoder::new(&LzhufMethod::Lh7);
        let a = b"aaaaaaaaaaa"
            .into_iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        let r = vec![
            // Block Size
            SmallBitVec::new(2_u16, 16),
            // len
            SmallBitVec::new(4, 5),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(1, 3), // 2 -> 0
            SmallBitVec::new(0, 2),
            SmallBitVec::new(1, 3), // 3 -> 1
            // sym
            SmallBitVec::new(264, 9),
            SmallBitVec::new(0, 1),
            SmallBitVec::new(77, 9),
            SmallBitVec::new(1, 1), // 97: 3 -> 0
            SmallBitVec::new(0, 1),
            SmallBitVec::new(145, 9),
            SmallBitVec::new(1, 1), // 263: 3 -> 1
            // off
            SmallBitVec::new(0, 5),
            SmallBitVec::new(0, 5), // offset = 0
            // data
            SmallBitVec::new(0, 1), // symbol = 97
            SmallBitVec::new(1, 1), // len = 263 - 256 + 3 = 10
        ];

        let b = r.to_bytes(BitWriter::<Left>::new(), Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(a, Ok(b));
    }

    #[test]
    fn test_empty() {
        let mut encoder = LzhufEncoder::new(&LzhufMethod::Lh7);
        let a = b"".into_iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(a, Ok(vec![]));
    }

    #[test]
    fn test_unit() {
        let mut encoder = LzhufEncoder::new(&LzhufMethod::Lh7);
        let a = b"a".into_iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        let r = vec![
            // Block Size
            SmallBitVec::new(1_u16, 16),
            // len
            SmallBitVec::new(0, 5),
            SmallBitVec::new(0, 5),
            // sym
            SmallBitVec::new(0, 9),
            SmallBitVec::new(97, 9),
            // off
            SmallBitVec::new(0, 5),
            SmallBitVec::new(0, 5),
        ];

        let b = r.to_bytes(BitWriter::<Left>::new(), Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(a, Ok(b));
    }

    #[test]
    fn test_midarr() {
        let mut encoder = LzhufEncoder::new(&LzhufMethod::Lh7);
        let a = b"a".into_iter()
            .cycle()
            .take(260)
            .map(|&x| x as u8)
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        let r = vec![
            // block size
            SmallBitVec::new(3_u16, 16),
            // len
            SmallBitVec::new(5, 5),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(1, 3), // 2 -> 0
            SmallBitVec::new(0, 2),
            SmallBitVec::new(2, 3), // 3 -> 2
            SmallBitVec::new(2, 3), // 4 -> 3
            // sym
            SmallBitVec::new(510, 9),
            SmallBitVec::new(0, 1),
            SmallBitVec::new(77, 9),
            SmallBitVec::new(3, 2), // 97: 4 -> 2 -> 2
            SmallBitVec::new(0, 1),
            SmallBitVec::new(138, 9),
            SmallBitVec::new(3, 2), // 256: 4 -> 2 -> 3
            SmallBitVec::new(0, 1),
            SmallBitVec::new(232, 9),
            SmallBitVec::new(2, 2), // 509: 3 -> 1 -> 0
            // off
            SmallBitVec::new(0, 5),
            SmallBitVec::new(0, 5), // offset = 0
            // data
            SmallBitVec::new(2, 2), // symbol = 97
            SmallBitVec::new(0, 1), // len = 509 - 256 + 3 = 256
            SmallBitVec::new(3, 2), // len = 256 - 256 + 3 = 3
        ];

        let b = r.to_bytes(BitWriter::<Left>::new(), Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(a, Ok(b));
    }

    #[test]
    fn test_lzhuflzsscode_offset() {
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 0 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 0,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 1 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 1,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 2 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 2,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 3 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 2,
                pos_sublen: 1,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 4 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 3,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 5 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 3,
                pos_sublen: 1,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 6 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 3,
                pos_sublen: 2,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 7 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 3,
                pos_sublen: 3,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference { len: 3, pos: 8 }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 4,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 15,
=======
                pos: 15
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 4,
                pos_sublen: 7,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 16,
=======
                pos: 16
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 5,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 31,
=======
                pos: 31
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 5,
                pos_sublen: 15,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 32,
=======
                pos: 32
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 6,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 64,
=======
                pos: 64
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 7,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 128,
=======
                pos: 128
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 8,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 256,
=======
                pos: 256
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 9,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 512,
=======
                pos: 512
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 10,
                pos_sublen: 0,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 1023,
=======
                pos: 1023
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 10,
                pos_sublen: 511,
            }
        );
        assert_eq!(
            LzhufLzssCode::from(&LzssCode::Reference {
                len: 3,
<<<<<<< HEAD
                pos: 1024,
=======
                pos: 1024
>>>>>>> master
            }),
            LzhufLzssCode::Reference {
                len: 256,
                pos_offset: 11,
                pos_sublen: 0,
            }
        );
    }
}
