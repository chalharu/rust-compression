//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use BitVector;
use LzssCode;
use Write;
use cano_huff_table;
use huffman_encoder::{HuffmanEncoder, LeftHuffmanEncoder};
use std::cmp::max;
use std::io::Result as ioResult;
use write::MultiWriter;

const MIN_MATCH: u16 = 3;

#[derive(Debug)]
enum LzhufLzssCode {
    Symbol(u8),
    Reference {
        len: u16,
        pos_offset: u16,
        pos_sublen: u16,
    },
}

impl From<LzssCode> for LzhufLzssCode {
    fn from(data: LzssCode) -> Self {
        match data {
            LzssCode::Symbol(s) => LzhufLzssCode::Symbol(s),
            LzssCode::Reference { len, pos } => {
                let po = pos.next_power_of_two();
                LzhufLzssCode::Reference {
                    len: len as u16 + 256 - MIN_MATCH,
                    pos_offset: po.trailing_zeros() as u16,
                    pos_sublen: (pos - (po >> 1)) as u16,
                }
            }
        }
    }
}

pub struct LzhufEncoder<W: Write<BitVector>> {
    inner: MultiWriter<BitVector, W>,
    max_block_len: usize,
    offset_tab_len: usize,
    block_buf: Vec<LzhufLzssCode>,
    symbol_freq: Vec<usize>,
    offset_freq: Vec<usize>,
    size_of_symbol_freq_buf: usize,
    size_of_offset_freq_buf: usize,
}

impl<W: Write<BitVector> + Clone> LzhufEncoder<W> {
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
        inner: W,
        max_block_len: usize,
        offset_tab_len: usize,
        max_match: usize,
    ) -> Self {
        let mbl_npot = max_block_len.next_power_of_two() >> 1;
        let size_of_offset_freq_buf =
            max(max_block_len - mbl_npot, mbl_npot - 1);
        let size_of_symbol_freq_buf = max_match + 256 - MIN_MATCH as usize + 1;
        Self {
            inner: MultiWriter::new(inner),
            max_block_len,
            offset_tab_len,
            size_of_symbol_freq_buf,
            size_of_offset_freq_buf,
            block_buf: Vec::with_capacity(max_block_len),
            symbol_freq: vec![0; size_of_symbol_freq_buf],
            offset_freq: vec![0; size_of_offset_freq_buf],
        }
    }

    pub fn into_inner(self) -> W {
        self.inner.into_inner()
    }

    fn enc_len(&mut self, len: u32) {
        if len >= 7 {
            let _ = self.inner.write(&BitVector::new(7, 3));
            for _ in 7..len {
                let _ = self.inner.write(&BitVector::new(1, 1));
            }
            let _ = self.inner.write(&BitVector::new(0, 1));
        } else {
            let _ = self.inner.write(&BitVector::new(len, 3));
        }
    }

    fn write_symb_tab(&mut self, symb_enc_tab: &Vec<u8>) {
        // generate length encoder
        let mut sym_list = Vec::new();
        let mut freq = vec![0; 19];

        if !symb_enc_tab.is_empty() {
            let mut i = 0;
            for (sym_ind, &sym_len) in
                symb_enc_tab.iter().enumerate().filter(|&(_, &t)| t != 0)
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
        let mut len_enc;
        if sym_list.is_empty() {
            let _ = self.inner.write(&BitVector::new(0, Self::TBIT_SIZE));
            let _ = self.inner.write(&BitVector::new(0, Self::TBIT_SIZE));
            len_enc = LeftHuffmanEncoder::new(self.inner.clone(), &Vec::new());
        } else {
            let len_enc_tab = cano_huff_table::make_table(&freq, 16);
            len_enc = LeftHuffmanEncoder::new(self.inner.clone(), &len_enc_tab);
            let len_enc_tab_map = len_enc_tab
                .iter()
                .enumerate()
                .filter(|&(_, &t)| t != 0)
                .collect::<Vec<_>>();
            if len_enc_tab_map.len() == 1 && *len_enc_tab_map[0].1 == 1 {
                let _ = self.inner.write(&BitVector::new(0, Self::TBIT_SIZE));
                let _ = self.inner.write(&BitVector::new(
                    len_enc_tab_map[0].0 as u32,
                    Self::TBIT_SIZE,
                ));
            } else {
                let mut i = 0;
                let _ = self.inner.write(&BitVector::new(
                    len_enc_tab_map.last().unwrap().0 as u32 + 1,
                    Self::TBIT_SIZE,
                ));

                for (len_ind, &len_len) in len_enc_tab_map {
                    while len_ind >= i {
                        if i == 3 {
                            let skip =
                                if len_ind > 6 { 3 } else { len_ind - 3 };
                            let _ = self.inner.write(
                                &BitVector::new(skip as u32, 2),
                            );
                            i += skip;
                        }

                        if len_ind != i {
                            let _ = self.inner.write(&BitVector::new(0, 3));
                        } else {
                            self.enc_len(len_len as u32);
                        }
                        i += 1;
                    }
                }
            }
        }

        // write symbol table
        if symb_enc_tab.is_empty() {
            let _ = self.inner.write(&BitVector::new(0, Self::CBIT_SIZE));
            let _ = self.inner.write(&BitVector::new(0, Self::CBIT_SIZE));
        } else {
            let symb_enc_tab_map = symb_enc_tab
                .iter()
                .enumerate()
                .filter(|&(_, &t)| t != 0)
                .collect::<Vec<_>>();

            if symb_enc_tab_map.len() == 1 && *symb_enc_tab_map[0].1 == 1 {
                let _ = self.inner.write(&BitVector::new(0, Self::CBIT_SIZE));
                let _ = self.inner.write(&BitVector::new(
                    symb_enc_tab_map[0].0 as u32,
                    Self::CBIT_SIZE,
                ));
            } else {
                let _ = self.inner.write(&BitVector::new(
                    symb_enc_tab_map.last().unwrap().0 as u32 +
                        1,
                    Self::CBIT_SIZE,
                ));
                for (s, l) in sym_list.into_iter() {
                    match s {
                        0 => {
                            let _ = len_enc.enc(&0);
                        }
                        1 => {
                            let _ = len_enc.enc(&1);
                            let _ =
                                self.inner.write(&BitVector::new(l as u32, 4));
                        }
                        2 => {
                            let _ = len_enc.enc(&2);
                            let _ =
                                self.inner.write(&BitVector::new(l as u32, 9));
                        }
                        _ => {
                            let _ = len_enc.enc(&l);
                        }
                    }
                }
            }
        }
    }

    fn write_offset_tab(&mut self, off_enc_tab: &Vec<u8>, pbit_len: usize) {
        // write length and symbol table
        let off_enc_tab_map = off_enc_tab
            .iter()
            .enumerate()
            .filter(|&(_, &t)| t != 0)
            .collect::<Vec<_>>();

        if off_enc_tab_map.is_empty() {
            let _ = self.inner.write(&BitVector::new(0, pbit_len));
            let _ = self.inner.write(&BitVector::new(0, pbit_len));
        } else if off_enc_tab_map.len() == 1 && *off_enc_tab_map[0].1 == 1 &&
                   off_enc_tab_map[0].0 > 0
        {
            let _ = self.inner.write(&BitVector::new(0, pbit_len));
            let _ = self.inner.write(&BitVector::new(
                off_enc_tab_map[0].0 as u32,
                pbit_len,
            ));
        } else {
            let mut i = 0;
            let _ = self.inner.write(&BitVector::new(
                off_enc_tab_map.last().unwrap().0 as u32 + 1,
                pbit_len,
            ));
            for (symb, &len) in off_enc_tab_map.into_iter() {
                while symb >= i {
                    if symb != i {
                        let _ = self.inner.write(&BitVector::new(0, 3));
                    } else {
                        self.enc_len(len as u32);
                    }
                    i += 1;
                }
            }
        }

    }

    fn write_block(&mut self) {
        let sym_enc_tab = cano_huff_table::make_table(&self.symbol_freq, 16);
        let off_enc_tab = cano_huff_table::make_table(&self.offset_freq, 16);
        let mut sym_enc =
            LeftHuffmanEncoder::new(self.inner.clone(), &sym_enc_tab);
        let mut off_enc =
            LeftHuffmanEncoder::new(self.inner.clone(), &off_enc_tab);

        // write block length
        let _ = self.inner.write(
            &BitVector::new(self.block_buf.len() as u32, 16),
        );

        self.write_symb_tab(&sym_enc_tab);
        let l = self.offset_tab_len;
        self.write_offset_tab(&off_enc_tab, l);

        for d in self.block_buf.iter() {
            match d {
                &LzhufLzssCode::Symbol(ref s) => {
                    let _ = sym_enc.enc(s);
                }
                &LzhufLzssCode::Reference {
                    ref len,
                    ref pos_offset,
                    pos_sublen,
                } => {
                    let _ = sym_enc.enc(len);
                    let _ = off_enc.enc(pos_offset);
                    if pos_sublen > 0 {
                        let _ = self.inner.write(&BitVector::new(
                            pos_sublen as u32,
                            *pos_offset as usize - 1,
                        ));
                    }
                }
            }
        }

        self.init_block();
    }
}

impl<W: Write<BitVector> + Clone> Write<LzssCode> for LzhufEncoder<W> {
    fn write(&mut self, buf: &LzssCode) -> ioResult<usize> {
        let code = LzhufLzssCode::from(buf.clone());
        match code {
            LzhufLzssCode::Symbol(s) => {
                self.symbol_freq[s as usize] += 1;
            }
            LzhufLzssCode::Reference { len, pos_offset, .. } => {
                self.symbol_freq[len as usize] += 1;
                self.offset_freq[pos_offset as usize] += 1;
            }
        }
        self.block_buf.push(code);

        if self.block_buf.len() == self.max_block_len {
            self.write_block();
        }

        Ok(1)
    }

    fn flush(&mut self) -> ioResult<()> {
        if !self.block_buf.is_empty() {
            self.write_block();
        }
        self.inner.flush();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lzss_encoder::LzssEncoder;
    use std::cmp::Ordering;
    use std::io::Write;

    fn comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
        match (lhs, rhs) {
            (LzssCode::Reference {
                 len: llen,
                 pos: lpos,
             },
             LzssCode::Reference {
                 len: rlen,
                 pos: rpos,
             }) => ((llen << 3) - lpos).cmp(&((rlen << 3) - rpos)).reverse(),
            (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
            (_, LzssCode::Symbol(_)) => Ordering::Greater,
            (LzssCode::Symbol(_), _) => Ordering::Less,
        }
    }

    #[test]
    fn test_arr() {
        let mut encoder = LzssEncoder::new(
            LzhufEncoder::new(Vec::new(), 65_536, 5, 256),
            comparison,
            65_536,
            256,
            3,
            3,
        );
        let _ = encoder.write_all(b"aaaaaaaaaaa");
        let _ = encoder.flush();
        assert_eq!(
            encoder.into_inner().into_inner(),
            vec![
                BitVector::new(2, 16),
                BitVector::new(4, 5),
                BitVector::new(0, 3),
                BitVector::new(0, 3),
                BitVector::new(1, 3),
                BitVector::new(0, 2),
                BitVector::new(1, 3),
                BitVector::new(264, 9),
                BitVector::new(0, 1),
                BitVector::new(77, 9),
                BitVector::new(1, 1),
                BitVector::new(0, 1),
                BitVector::new(145, 9),
                BitVector::new(1, 1),
                BitVector::new(1, 5),
                BitVector::new(1, 3),
                BitVector::new(0, 1),
                BitVector::new(1, 1),
                BitVector::new(0, 1),
            ]
        );
    }

    #[test]
    fn test_empty() {
        let mut encoder = LzssEncoder::new(
            LzhufEncoder::new(Vec::new(), 65_536, 5, 256),
            comparison,
            65_536,
            256,
            3,
            3,
        );
        let _ = encoder.write_all(b"");
        let _ = encoder.flush();
        assert_eq!(encoder.into_inner().into_inner(), vec![]);
    }

    #[test]
    fn test_unit() {
        let mut encoder = LzssEncoder::new(
            LzhufEncoder::new(Vec::new(), 65_536, 5, 256),
            comparison,
            65_536,
            256,
            3,
            3,
        );
        let _ = encoder.write_all(b"a");
        let _ = encoder.flush();
        assert_eq!(
            encoder.into_inner().into_inner(),
            vec![
                BitVector::new(1, 16),
                BitVector::new(4, 5),
                BitVector::new(0, 3),
                BitVector::new(0, 3),
                BitVector::new(1, 3),
                BitVector::new(0, 2),
                BitVector::new(1, 3),
                BitVector::new(0, 9),
                BitVector::new(97, 9),
                BitVector::new(0, 5),
                BitVector::new(0, 5),
                BitVector::new(0, 1),
            ]
        );
    }
}
