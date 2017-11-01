#![crate_type = "lib"]

//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

extern crate num_iter;
extern crate num_traits;

use num_traits::{cast, NumCast};
use std::collections::HashMap;

mod bit_vector;
mod bit_writer;
mod bit_reader;
mod huffman_encoder;
mod internal;

pub use bit_reader::*;
pub use bit_vector::BitVector;
pub use bit_writer::*;
pub use huffman_encoder::*;

use internal::*;

pub trait HuffmanDecoder {
    type BR: BitReader;
    type Item: Clone + NumCast;

    fn dec(&mut self) -> std::io::Result<Self::Item>;
    fn get_ref(&self) -> &Self::BR;
    fn get_mut(&mut self) -> &mut Self::BR;
    fn into_inner(&mut self) -> Self::BR;
}

macro_rules! huffman_decoder_impl {
    ($name:ident, $is_rev:expr) => {
        pub struct $name<BR: BitReader, T: NumCast + Clone> {
            inner: Option<BR>,
            stab_bits: usize,
            stab: Vec<Option<(T, u8)>>,
            long_map: HashMap<BitVector, T>,
        }

        impl<BR: BitReader, T: NumCast + Clone + std::fmt::Debug> $name<BR, T> {
            pub fn new(inner: BR, symb_len: &[u8], stab_bits: usize) -> Self {
                const IS_REV: bool = $is_rev;
                let huff_tab = internal::creat_huffman_table(symb_len, IS_REV);
                let mut stab = vec![None; 1 << stab_bits];
                let mut long_map = HashMap::new();
                for (i, h) in huff_tab.into_iter().enumerate() {
                    if let Some(b) = h {
                        let val = cast::<_, T>(i).unwrap();
                        if stab_bits >= b.len() {
                            let ld = stab_bits - b.len();
                            let head =
                                if !IS_REV { b.data() << ld } else { b.data() };
                            for j in 0..(1 << ld) {
                                if !IS_REV {
                                    stab[(head | j) as usize] =
                                        Some((val.clone(), b.len() as u8));
                                } else {
                                    stab[(head | (j << b.len())) as usize] =
                                        Some((val.clone(), b.len() as u8));
                                }
                            }
                        } else {
                            long_map.insert(b, val);
                        }
                    }
                }
                Self {
                    inner: Some(inner),
                    stab_bits,
                    stab,
                    long_map,
                }
            }
        }

        impl<BR: BitReader, T: NumCast + Clone> HuffmanDecoder
            for $name<BR, T> {
            type BR = BR;
            type Item = T;

            fn dec(&mut self) -> std::io::Result<Self::Item> {
                match self.inner.as_mut().unwrap().peek(self.stab_bits) {
                    Ok(c) => {
                        let c = if !$is_rev {
                            (c.data() << (self.stab_bits - c.len()))
                        } else {
                            c.data()
                        } as usize;
                        if let &Some(ref v) = &self.stab[c] {
                            let _ =
                                self.inner.as_mut().unwrap().skip(v.1 as usize);
                            Ok(v.0.clone())
                        } else {
                            let mut l = self.stab_bits;
                            while l < 32 {
                                l += 1;
                                if let Ok(mut b) = self.inner
                                    .as_mut()
                                    .unwrap()
                                    .peek(l)
                                {
                                    if b.len() == l {
                                        if let Some(v) = self.long_map.get(&b) {
                                            let _ = self.inner
                                                .as_mut()
                                                .unwrap()
                                                .skip(b.len());
                                            return Ok(v.clone());
                                        }
                                    } else {
                                        while b.len() < 32 {
                                            l += 1;
                                            b = BitVector::new(
                                                if !$is_rev {
                                                    b.data() << 1
                                                } else {
                                                    b.data()
                                                },
                                                b.len() + 1,
                                            );
                                            if let Some(v) = self.long_map
                                                .get(&b)
                                            {
                                                let _ = self.inner
                                                    .as_mut()
                                                    .unwrap()
                                                    .skip(b.len());
                                                return Ok(v.clone());
                                            }
                                        }
                                        return Err(std::io::Error::new(
                                            std::io::ErrorKind::InvalidData,
                                            "huffman error",
                                        ));
                                    }
                                }
                            }
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "huffman error",
                            ));
                        }
                    }
                    Err(e) => Err(e),
                }
            }

            fn get_ref(&self) -> &Self::BR {
                self.inner.as_ref().unwrap()
            }

            fn get_mut(&mut self) -> &mut Self::BR {
                self.inner.as_mut().unwrap()
            }

            fn into_inner(&mut self) -> Self::BR {
                self.inner.take().unwrap()
            }
        }
    }
}

huffman_decoder_impl!(LeftHuffmanDecoder, false);
huffman_decoder_impl!(RightHuffmanDecoder, true);

pub mod canno_huff_table {
    fn down_heap(buf: &mut Vec<usize>, mut n: usize, len: usize) {
        let tmp = buf[n];
        let mut leaf = (n << 1) + 1;

        while leaf < len {
            if leaf + 1 < len && buf[buf[leaf]] > buf[buf[leaf + 1]] {
                leaf += 1;
            }

            if buf[tmp] < buf[buf[leaf]] {
                break;
            }
            buf[n] = buf[leaf];
            n = leaf;
            leaf = (n << 1) + 1;
        }
        buf[n] = tmp;
    }

    fn create_heap(buf: &mut Vec<usize>) {
        let s = buf.len() >> 1;
        let mut i = (s >> 1) - 1;
        while i > 0 {
            down_heap(buf, i, s);
            i -= 1;
        }
    }

    fn take_package(
        ty: &mut Vec<Vec<usize>>,
        len: &mut Vec<usize>,
        cur: &mut Vec<usize>,
        i: usize,
    ) {
        let x = ty[i][cur[i]];
        if x == len.len() {
            take_package(ty, len, cur, i + 1);
            take_package(ty, len, cur, i + 1);
        } else {
            len[x] -= 1;
        }

        cur[i] += 1;
    }

    /// Reverse package merge
    fn gen_code_lm<F: Fn(usize, usize) -> usize>(
        freq: &[usize],
        lim: usize,
        weight_add_fn: F,
    ) -> Vec<u8> {
        let len = freq.len();
        let mut freqmap = freq.iter()
            .enumerate()
            .map(|(i, &f)| (i, f))
            .collect::<Vec<_>>();
        freqmap.sort_by(|x, y| y.1.cmp(&x.1));
        let (map, sfreq): (Vec<_>, Vec<_>) = freqmap.into_iter().unzip();

        let mut max_elem = vec![0; lim];
        let mut b = vec![0; lim];

        let mut excess = (1 << lim) - len;
        let half = 1 << (lim - 1);
        max_elem[lim - 1] = len;

        for j in 0..lim {
            if excess >= half {
                b[j] = 1;
                excess -= half;
            }
            excess <<= 1;
            if lim >= 2 + j {
                max_elem[lim - 2 - j] = max_elem[lim - 1 - j] / 2 + len;
            }
        }

        max_elem[0] = b[0];
        for j in 1..lim {
            if max_elem[j] > 2 * max_elem[j - 1] + b[j] {
                max_elem[j] = 2 * max_elem[j - 1] + b[j];
            }
        }

        let mut val = (0..lim).map(|i| vec![0; max_elem[i]]).collect::<Vec<_>>();
        let mut ty = (0..lim).map(|i| vec![0; max_elem[i]]).collect::<Vec<_>>();
        let mut c = vec![lim; len];

        for t in 0..max_elem[lim - 1] {
            val[lim - 1][t] = sfreq[t];
            ty[lim - 1][t] = t;
        }

        let mut cur = vec![0; lim];
        if b[lim - 1] == 1 {
            c[0] -= 1;
            cur[lim - 1] += 1;
        }

        let mut j = lim - 1;
        while j > 0 {
            let mut i = 0;
            let mut next = cur[j];

            for t in 0..max_elem[j - 1] {
                let weight = if next + 1 < max_elem[j] {
                    weight_add_fn(val[j][next], val[j][next + 1])
                } else {
                    0
                };
                if weight > sfreq[i] {
                    val[j - 1][t] = weight;
                    ty[j - 1][t] = len;
                    next += 2;
                } else {
                    val[j - 1][t] = sfreq[i];
                    ty[j - 1][t] = i;
                    i += 1;
                    if i >= len {
                        break;
                    }
                }
            }

            j -= 1;
            cur[j] = 0;
            if b[j] == 1 {
                take_package(&mut ty, &mut c, &mut cur, j);
            }
        }

        let mut r = c.iter()
            .zip(map)
            .map(|(&x, i)| (x as u8, i))
            .collect::<Vec<_>>();
        r.sort_unstable_by_key(|v| v.1);
        r.into_iter().map(move |v| v.0).collect::<Vec<_>>()
    }

    fn gen_code<F: Fn(usize, usize) -> usize>(
        freq: &[usize],
        lim: usize,
        weight_add_fn: F,
    ) -> Vec<u8> {
        let mut buf = (freq.len()..(freq.len() << 1))
            .chain(freq.iter().cloned())
            .collect();

        create_heap(&mut buf);

        // Generate Huffman Tree
        let mut i = freq.len() - 1;
        while i > 0 {
            let m1 = buf[0];
            buf[0] = buf[i];
            down_heap(&mut buf, 0, i);
            let m2 = buf[0];
            buf[i] = weight_add_fn(buf[m1], buf[m2]);
            buf[0] = i;
            buf[m1] = i;
            buf[m2] = i;
            down_heap(&mut buf, 0, i);
            i -= 1;
        }

        // Counting
        buf[1] = 0;
        for i in 2..freq.len() {
            buf[i] = buf[buf[i]] + 1;
        }

        let ret = (0..freq.len())
            .map(|i| (buf[buf[i + freq.len()]] + 1) as u8)
            .collect::<Vec<_>>();

        if ret.iter().any(|l| *l as usize > lim) {
            gen_code_lm(freq, lim, weight_add_fn)
        } else {
            ret
        }
    }

    pub fn make_tab_with_fn<F: Fn(usize, usize) -> usize>(
        freq: &[usize],
        lim: usize,
        weight_add_fn: F,
    ) -> Vec<u8> {
        if freq.len() == 0 {
            Vec::new()
        } else {
            let (s, l): (Vec<_>, Vec<_>) = freq.into_iter()
                .enumerate()
                .filter_map(|(i, &t)| if t != 0 { Some((i, t)) } else { None })
                .unzip();
            s.into_iter()
                .zip(gen_code(&l, lim, weight_add_fn))
                .scan(0, move |c, (s, v)| {
                    let r = vec![0; s - *c].into_iter().chain(vec![v]);
                    *c = s + 1;
                    Some(r)
                })
                .flat_map(move |v| v)
                .collect()
        }
    }

    pub fn make_table(freq: &[usize], lim: usize) -> Vec<u8> {
        make_tab_with_fn(freq, lim, |x, y| x + y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn lefthuffman_decode() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = LeftHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = LeftBitReader::new(cursor);
        let mut hdecoder = LeftHuffmanDecoder::<_, u8>::new(reader, &symb_len, 12);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }


    #[test]
    fn lefthuffman_decode_big() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = LeftHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = LeftBitReader::new(cursor);
        let mut hdecoder = LeftHuffmanDecoder::<_, u8>::new(reader, &symb_len, 2);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }

    #[test]
    fn righthuffman_decode() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = RightHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = RightBitReader::new(cursor);
        let mut hdecoder = RightHuffmanDecoder::<_, u8>::new(reader, &symb_len, 4);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }


    #[test]
    fn righthuffman_decode_big() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = RightHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = RightBitReader::new(cursor);
        let mut hdecoder = RightHuffmanDecoder::<_, u8>::new(reader, &symb_len, 2);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }

    #[test]
    fn create_haffman_tab() {
        let freq = vec![0, 1, 1, 2, 2, 4, 4, 8, 8];
        let tab = canno_huff_table::make_table(&freq, 12);

        assert_eq!(
            tab.iter()
                .zip(freq)
                .map(|(x, y)| *x as usize * y)
                .sum::<usize>(),
            80
        );
    }

    #[test]
    fn create_haffman_tab_with_fn() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let freq = vec![0_usize, 1, 1, 2, 2, 4, 4, 8, 8];
        let tab = canno_huff_table::make_tab_with_fn(
            &freq.iter().map(|i| i << 8).collect::<Vec<_>>(),
            12,
            |x, y| (x & !0xFF) + (y & !0xFF) | std::cmp::max(x & 0xFF, y & 0xFF) + 1,
        );

        assert_eq!(tab, symb_len);
    }

    #[test]
    fn create_haffman_tab_with_fn_lim_len() {
        let freq = (0..63).collect::<Vec<_>>();
        let tab = canno_huff_table::make_tab_with_fn(
            &freq.iter().map(|i| i << 8).collect::<Vec<_>>(),
            8,
            |x, y| (x & !0xFF) + (y & !0xFF) | std::cmp::max(x & 0xFF, y & 0xFF) + 1,
        );

        assert!(
            tab.iter()
                .zip(freq.clone())
                .map(|(x, y)| *x as usize * y)
                .sum::<usize>() < freq.iter().sum::<usize>() * 6
        );

        assert!(*tab.iter().max().unwrap() <= 8);
    }

}
