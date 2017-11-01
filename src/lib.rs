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
use std::cmp::min;

use std::collections::HashMap;
use std::io::{Read, Write};

use std::ops::{Add, Sub};

#[derive(Copy, PartialEq, Eq, Clone, Debug, Hash)]
pub struct BitVector {
    data: u32,
    len: usize,
}

impl BitVector {
    pub fn new(data: u32, len: usize) -> Self {
        BitVector { data, len }
    }

    pub fn reverse(&self) -> Self {
        let mut x = self.data;
        x = (x & 0x55555555) << 1 | (x & 0xAAAAAAAA) >> 1;
        x = (x & 0x33333333) << 2 | (x & 0xCCCCCCCC) >> 2;
        x = (x & 0x0F0F0F0F) << 4 | (x & 0xF0F0F0F0) >> 4;
        x = x << 24 | (x & 0xFF00) << 8 | (x & 0xFF0000) >> 8 | x >> 24;
        x >>= 32 - self.len;
        Self::new(x, self.len)
    }
}

pub trait BitWriter {
    type W: Write;
    fn write(&mut self, buf: &BitVector) -> std::io::Result<usize>;
    fn pad_flush(&mut self) -> std::io::Result<()>;
    fn get_ref(&self) -> &Self::W;
    fn get_mut(&mut self) -> &mut Self::W;
    fn into_inner(&mut self) -> Result<Self::W, std::io::Error>;
}

pub struct LeftBitWriter<W: Write> {
    inner: Option<W>,
    buf: u8,
    counter: usize,
}

impl<W: Write> LeftBitWriter<W> {
    pub fn new(inner: W) -> Self {
        LeftBitWriter {
            inner: Some(inner),
            buf: 0,
            counter: 8,
        }
    }
}

impl<W: Write> BitWriter for LeftBitWriter<W> {
    type W = W;
    fn write(&mut self, data: &BitVector) -> std::io::Result<usize> {
        const BIT_LEN: usize = 32 /* u32 */;
        if data.len == 0 {
            return Ok(0);
        }
        let mut len = data.len;
        let mut data = data.data << (BIT_LEN - len);
        let mut r = 0;
        while len >= self.counter {
            let result = self.inner
                .as_mut()
                .unwrap()
                .write(&[self.buf | (data >> (BIT_LEN - self.counter)) as u8; 1]);
            if let Ok(l) = result {
                if l == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "failed to write the data",
                    ));
                }
                len -= self.counter;
                data <<= self.counter;
                r += self.counter;
                self.buf = 0;
                self.counter = 8 /* u8 */;
            } else {
                return result;
            }
        }

        self.buf |= (data >> (BIT_LEN - self.counter)) as u8;
        self.counter -= len;
        Ok(r + len)
    }

    fn pad_flush(&mut self) -> std::io::Result<()> {
        let c = self.counter;
        if c != 8 {
            let r = self.write(&BitVector::new(0, c));
            if let Err(e) = r {
                return Err(e);
            }
        }
        self.inner.as_mut().unwrap().flush()
    }

    fn get_ref(&self) -> &W {
        self.inner.as_ref().unwrap()
    }

    fn get_mut(&mut self) -> &mut W {
        self.inner.as_mut().unwrap()
    }

    fn into_inner(&mut self) -> Result<W, std::io::Error> {
        match self.pad_flush() {
            Err(e) => Err(e),
            Ok(()) => Ok(self.inner.take().unwrap()),
        }
    }
}

impl<W: Write> Drop for LeftBitWriter<W> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            // dtors should not panic, so we ignore a failed flush
            let _r = self.pad_flush();
        }
    }
}

pub struct RightBitWriter<W: Write> {
    inner: Option<W>,
    buf: u8,
    counter: usize,
}

impl<W: Write> RightBitWriter<W> {
    pub fn new(inner: W) -> Self {
        RightBitWriter {
            inner: Some(inner),
            buf: 0,
            counter: 8,
        }
    }
}

impl<W: Write> BitWriter for RightBitWriter<W> {
    type W = W;
    fn write(&mut self, data: &BitVector) -> std::io::Result<usize> {
        const BIT_LEN: usize = 8 /* u8 */;
        let mut len = data.len;
        let mut data = data.data;
        let mut r = 0;
        while len >= self.counter {
            let result = self.inner
                .as_mut()
                .unwrap()
                .write(&[self.buf | (data << (BIT_LEN - self.counter)) as u8; 1]);
            if let Ok(l) = result {
                if l == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "failed to write the data",
                    ));
                }
                len -= self.counter;
                data >>= self.counter;
                r += self.counter;
                self.buf = 0;
                self.counter = BIT_LEN;
            } else {
                return result;
            }
        }

        self.buf |= (data << (BIT_LEN - self.counter)) as u8;
        self.counter -= len;
        Ok(r + len)
    }

    fn pad_flush(&mut self) -> std::io::Result<()> {
        let c = self.counter;
        if c != 8 {
            let r = self.write(&BitVector::new(0, c));
            if let Err(e) = r {
                return Err(e);
            }
        }
        self.inner.as_mut().unwrap().flush()
    }

    fn get_ref(&self) -> &W {
        self.inner.as_ref().unwrap()
    }

    fn get_mut(&mut self) -> &mut W {
        self.inner.as_mut().unwrap()
    }

    fn into_inner(&mut self) -> Result<W, std::io::Error> {
        match self.pad_flush() {
            Err(e) => Err(e),
            Ok(()) => Ok(self.inner.take().unwrap()),
        }
    }
}

impl<W: Write> Drop for RightBitWriter<W> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            // dtors should not panic, so we ignore a failed flush
            let _r = self.pad_flush();
        }
    }
}

pub trait BitReader {
    type R: Read;
    fn read(&mut self, len: usize) -> std::io::Result<BitVector>;
    fn peek(&mut self, len: usize) -> std::io::Result<BitVector>;
    fn skip(&mut self, len: usize) -> std::io::Result<usize>;
    fn skip_to_byte(&mut self) -> std::io::Result<usize>;
    fn get_ref(&self) -> &Self::R;
    fn get_mut(&mut self) -> &mut Self::R;
    fn into_inner(&mut self) -> Result<Self::R, std::io::Error>;
}

pub struct LeftBitReader<R: Read> {
    inner: Option<R>,
    buf: u32,
    counter: usize,
}

impl<R: Read> LeftBitReader<R> {
    pub fn new(inner: R) -> Self {
        LeftBitReader {
            inner: Some(inner),
            buf: 0,
            counter: 0,
        }
    }
}

impl<R: Read> BitReader for LeftBitReader<R> {
    type R = R;
    fn read(&mut self, len: usize) -> std::io::Result<BitVector> {
        let r = self.peek(len);
        if let Ok(l) = r {
            self.buf <<= l.len;
            self.counter -= l.len;
        }
        r
    }

    fn peek(&mut self, len: usize) -> std::io::Result<BitVector> {
        while len > self.counter {
            let ls_count = 32 /* u32 */ - 8 /* u8 */ - (self.counter as isize);
            if ls_count < 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "len is too long",
                ));
            }
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf |= (buf[0] as u32) << ls_count;
                    self.counter += 8 /* u8 */;
                    continue;
                }
            }
            if self.counter == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "end of file",
                ));
            }
            break;
        }
        let l = min(len, self.counter);
        Ok(BitVector::new(self.buf >> (32 - l), l))
    }

    fn skip(&mut self, mut len: usize) -> std::io::Result<usize> {
        let r = Ok(len);
        while len > self.counter {
            len -= self.counter;
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf = (buf[0] as u32) << (32 /* u32 */ - 8 /* u8 */);
                    self.counter = 8 /* u8 */;
                    continue;
                }
            }
            self.buf = 0;
            self.counter = 0;
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "end of file",
            ));
        }
        self.buf <<= len;
        self.counter -= len;
        r
    }

    fn skip_to_byte(&mut self) -> std::io::Result<usize> {
        let s_count = self.counter & 0x07;
        self.skip(s_count)
    }

    fn get_ref(&self) -> &R {
        self.inner.as_ref().unwrap()
    }

    fn get_mut(&mut self) -> &mut R {
        self.inner.as_mut().unwrap()
    }

    fn into_inner(&mut self) -> Result<R, std::io::Error> {
        match self.skip_to_byte() {
            Err(e) => Err(e),
            Ok(_) => Ok(self.inner.take().unwrap()),
        }
    }
}

pub struct RightBitReader<R: Read> {
    inner: Option<R>,
    buf: u32,
    counter: usize,
}

impl<R: Read> RightBitReader<R> {
    pub fn new(inner: R) -> Self {
        RightBitReader {
            inner: Some(inner),
            buf: 0,
            counter: 0,
        }
    }
}

impl<R: Read> BitReader for RightBitReader<R> {
    type R = R;
    fn read(&mut self, len: usize) -> std::io::Result<BitVector> {
        let r = self.peek(len);
        if let Ok(l) = r {
            self.buf >>= l.len;
            self.counter -= l.len;
        }
        r
    }

    fn peek(&mut self, len: usize) -> std::io::Result<BitVector> {
        while len > self.counter {
            if 32 /* u32 */ <= 8 /* u8 */ + self.counter {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "len is too long",
                ));
            }
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf |= (buf[0] as u32) << self.counter;
                    self.counter += 8 /* u8 */;
                    continue;
                }
            }
            if self.counter == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "end of file",
                ));
            }
            break;
        }
        let l = min(len, self.counter);
        Ok(BitVector::new(self.buf & ((1 << l) - 1), l))
    }

    fn skip(&mut self, mut len: usize) -> std::io::Result<usize> {
        let r = Ok(len);
        while len > self.counter {
            len -= self.counter;
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf = buf[0] as u32;
                    self.counter = 8 /* u8 */;
                    continue;
                }
            }
            self.buf = 0;
            self.counter = 0;
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "end of file",
            ));
        }
        self.buf >>= len;
        self.counter -= len;
        r
    }

    fn skip_to_byte(&mut self) -> std::io::Result<usize> {
        let s_count = self.counter & 0x07;
        self.skip(s_count)
    }

    fn get_ref(&self) -> &R {
        self.inner.as_ref().unwrap()
    }

    fn get_mut(&mut self) -> &mut R {
        self.inner.as_mut().unwrap()
    }

    fn into_inner(&mut self) -> Result<R, std::io::Error> {
        match self.skip_to_byte() {
            Err(e) => Err(e),
            Ok(_) => Ok(self.inner.take().unwrap()),
        }
    }
}

trait MinValue {
    fn min_value() -> Self;
}

trait MaxValue {
    fn max_value() -> Self;
}

impl MinValue for u8 {
    fn min_value() -> Self {
        u8::min_value()
    }
}

impl MaxValue for u8 {
    fn max_value() -> Self {
        u8::max_value()
    }
}

impl MinValue for u16 {
    fn min_value() -> Self {
        u16::min_value()
    }
}

impl MaxValue for u16 {
    fn max_value() -> Self {
        u16::max_value()
    }
}

trait BucketSort {
    type Item;
    fn bucket_sort<K: Clone + Add + Sub<Output = K> + NumCast, F: Fn(&Self::Item) -> K>(
        &self,
        key_selector: F,
        min: K,
        max: K,
    ) -> Vec<Self::Item>;

    fn bucket_sort_all<K, F>(&self, key_selector: F) -> Vec<Self::Item>
    where
        K: MaxValue + MinValue + Clone + Add + Sub<Output = K> + NumCast,
        F: Fn(&Self::Item) -> K,
    {
        self.bucket_sort(key_selector, MinValue::min_value(), MaxValue::max_value())
    }
}

impl<T: Clone> BucketSort for [T] {
    type Item = T;
    fn bucket_sort<K: Clone + Add + Sub<Output = K> + NumCast, F: Fn(&T) -> K>(
        &self,
        key_selector: F,
        min: K,
        max: K,
    ) -> Vec<T> {
        let mut ret = self.to_vec();
        let mut bucket = vec![0; cast::<K, usize>(max - min.clone()).unwrap() + 2];

        for i in 0..self.len() {
            bucket[cast::<_, usize>(key_selector(&self[i]) - min.clone()).unwrap() + 1] += 1;
        }
        for i in 2..bucket.len() {
            bucket[i] += bucket[i - 1];
        }
        for i in 0..self.len() {
            let val = self[i].clone();
            let idx = cast::<_, usize>(key_selector(&val) - min.clone()).unwrap();
            ret[bucket[idx]] = val;
            bucket[idx] += 1;
        }
        ret
    }
}

fn creat_huffman_table(symb_len: &[u8], is_reverse: bool) -> Vec<Option<BitVector>> {
    let symbs = symb_len
        .into_iter()
        .enumerate()
        .filter(|&(_, &t)| t != 0)
        .collect::<Vec<_>>();
    if symbs.len() > 0 {
        let min_symb = symbs[0].0;
        let max_symb = symbs.last().unwrap().0;
        symbs
            .bucket_sort_all(|x| *x.1)
            .into_iter()
            .scan((0, 0), move |c, (s, &l)| {
                let code = c.1 << if c.0 < l { l - c.0 } else { 0 };
                *c = (l, code + 1);
                Some((
                    s,
                    if is_reverse {
                        BitVector::new(code, l as usize).reverse()
                    } else {
                        BitVector::new(code, l as usize)
                    },
                ))
            })
            .collect::<Vec<_>>()
            .bucket_sort(|x| x.0, min_symb, max_symb)
            .into_iter()
            .scan(0, move |c, (s, v)| {
                let r = vec![None; s - *c].into_iter().chain(vec![Some(v)]);
                *c = s + 1;
                Some(r)
            })
            .flat_map(move |v| v)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    }
}

pub trait HuffmanEncoder {
    type BW: BitWriter;
    fn enc<T: NumCast + Clone>(&mut self, data: &T) -> std::io::Result<usize>;
    fn get_enc_tab(&self) -> &[Option<BitVector>];
    fn get_ref(&self) -> &Self::BW;
    fn get_mut(&mut self) -> &mut Self::BW;
    fn into_inner(&mut self) -> Self::BW;
}

macro_rules! huffman_encoder_impl {
    ($name:ident, $is_rev:expr) => {
        pub struct $name<BW: BitWriter> {
            inner: Option<BW>,
            bit_vec_tab: Vec<Option<BitVector>>,
        }

        impl<BW: BitWriter> $name<BW> {
            pub fn new(inner: BW, symb_len: &[u8]) -> Self {
                Self {
                    inner: Some(inner),
                    bit_vec_tab: creat_huffman_table(symb_len, $is_rev),
                }
            }
        }

        impl<BW: BitWriter> HuffmanEncoder for $name<BW> {
            type BW = BW;
            fn enc<
                T: NumCast + Clone
            >(
                &mut self,
                data: &T,
            ) -> std::io::Result<usize> {
                if let Some(idx) = cast::<_, usize>(data.clone()) {
                    if idx < self.bit_vec_tab.len() {
                        if let Some(ref bv) = self.bit_vec_tab[idx] {
                            return self.inner.as_mut().unwrap().write(bv);
                        }
                    }
                }
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "out of value",
                ))
            }

            fn get_enc_tab(&self) -> &[Option<BitVector>] {
                &self.bit_vec_tab
            }

            fn get_ref(&self) -> &Self::BW {
                self.inner.as_ref().unwrap()
            }

            fn get_mut(&mut self) -> &mut Self::BW {
                self.inner.as_mut().unwrap()
            }

            fn into_inner(&mut self) -> Self::BW {
                self.inner.take().unwrap()
            }
        }
    }
}

huffman_encoder_impl!(LeftHuffmanEncoder, false);
huffman_encoder_impl!(RightHuffmanEncoder, true);

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
                let huff_tab = creat_huffman_table(symb_len, IS_REV);
                let mut stab = vec![None; 1 << stab_bits];
                let mut long_map = HashMap::new();
                for (i, h) in huff_tab.into_iter().enumerate() {
                    if let Some(b) = h {
                        let val = cast::<_, T>(i).unwrap();
                        if stab_bits >= b.len {
                            let ld = stab_bits - b.len;
                            let head =
                                if !IS_REV { b.data << ld } else { b.data };
                            for j in 0..(1 << ld) {
                                if !IS_REV {
                                    stab[(head | j) as usize] =
                                        Some((val.clone(), b.len as u8));
                                } else {
                                    stab[(head | (j << b.len)) as usize] =
                                        Some((val.clone(), b.len as u8));
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
                            (c.data << (self.stab_bits - c.len))
                        } else {
                            c.data
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
                                    if b.len == l {
                                        if let Some(v) = self.long_map.get(&b) {
                                            let _ = self.inner
                                                .as_mut()
                                                .unwrap()
                                                .skip(b.len);
                                            return Ok(v.clone());
                                        }
                                    } else {
                                        while b.len < 32 {
                                            l += 1;
                                            b = BitVector::new(
                                                if !$is_rev {
                                                    b.data << 1
                                                } else {
                                                    b.data
                                                },
                                                b.len + 1,
                                            );
                                            if let Some(v) = self.long_map
                                                .get(&b)
                                            {
                                                let _ = self.inner
                                                    .as_mut()
                                                    .unwrap()
                                                    .skip(b.len);
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
        r.sort_by_key(|v| v.1);
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
    fn bitvector_reverse() {
        assert_eq!(
            BitVector::new(0xC71F, 17).reverse(),
            BitVector::new(0x1F1C6, 17)
        );
    }

    #[test]
    fn leftbitwriter_write() {
        let mut writer = LeftBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));
        assert_eq!(writer.get_ref()[0], 0b11001100);
        assert_eq!(writer.get_ref().len(), 1);
    }

    #[test]
    fn leftbitwriter_write_big() {
        let mut writer = LeftBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));
        assert_eq!(writer.get_ref()[0], 243);
        assert_eq!(writer.get_ref()[1], 221);
        assert_eq!(writer.get_ref()[2], 190);
        assert_eq!(writer.get_ref()[3], 200);
        assert_eq!(writer.get_ref().len(), 4);
    }

    #[test]
    fn leftbitwriter_write_pad() {
        let mut writer = LeftBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(1, 1));
        let _ = writer.write(&BitVector::new(2, 2));
        let _ = writer.write(&BitVector::new(3, 3));
        assert_eq!(writer.get_ref().len(), 0);
        let _ = writer.pad_flush();
        assert_eq!(writer.get_ref()[0], 204);
        assert_eq!(writer.get_ref().len(), 1);
    }

    #[test]
    fn leftbitwriter_write_1bit() {
        let mut writer = LeftBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(1, 1));
        let inner = writer.into_inner().unwrap();
        assert_eq!(inner[0], 128);
    }

    #[test]
    fn leftbitwriter_zero() {
        let mut writer = LeftBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(0, 10));
        let _ = writer.write(&BitVector::new(0, 0));
        let _ = writer.write(&BitVector::new(0, 1));
        let _ = writer.write(&BitVector::new(0, 2));
        let _ = writer.write(&BitVector::new(0, 3));
        let _ = writer.write(&BitVector::new(0, 4));
        let _ = writer.write(&BitVector::new(0, 12));
        assert_eq!(writer.get_ref()[0], 0);
        assert_eq!(writer.get_ref()[1], 0);
        assert_eq!(writer.get_ref()[2], 0);
        assert_eq!(writer.get_ref()[3], 0);
        assert_eq!(writer.get_ref().len(), 4);
    }

    #[test]
    fn rightbitwriter_write() {
        let mut writer = RightBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));
        assert_eq!(writer.get_ref()[0], 0b00011101);
        assert_eq!(writer.get_ref().len(), 1);
    }

    #[test]
    fn rightbitwriter_write_big() {
        let mut writer = RightBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));
        assert_eq!(writer.get_ref()[0], 0xCF);
        assert_eq!(writer.get_ref()[1], 0x6F);
        assert_eq!(writer.get_ref()[2], 0x87);
        assert_eq!(writer.get_ref()[3], 0xEC);
        assert_eq!(writer.get_ref().len(), 4);
    }

    #[test]
    fn rightbitwriter_write_pad() {
        let mut writer = RightBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(1, 1));
        let _ = writer.write(&BitVector::new(2, 2));
        let _ = writer.write(&BitVector::new(3, 3));
        assert_eq!(writer.get_ref().len(), 0);
        let _ = writer.pad_flush();
        assert_eq!(writer.get_ref()[0], 0b00011101);
        assert_eq!(writer.get_ref().len(), 1);
    }

    #[test]
    fn rightbitwriter_write_1bit() {
        let mut writer = RightBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(1, 1));
        let inner = writer.into_inner().unwrap();
        assert_eq!(inner[0], 1);
    }

    #[test]
    fn rightbitwriter_zero() {
        let mut writer = RightBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(0, 10));
        let _ = writer.write(&BitVector::new(0, 0));
        let _ = writer.write(&BitVector::new(0, 1));
        let _ = writer.write(&BitVector::new(0, 2));
        let _ = writer.write(&BitVector::new(0, 3));
        let _ = writer.write(&BitVector::new(0, 4));
        let _ = writer.write(&BitVector::new(0, 12));
        assert_eq!(writer.get_ref()[0], 0);
        assert_eq!(writer.get_ref()[1], 0);
        assert_eq!(writer.get_ref()[2], 0);
        assert_eq!(writer.get_ref()[3], 0);
        assert_eq!(writer.get_ref().len(), 4);
    }

    #[test]
    fn leftbitreader_read() {
        let mut writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.read(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
    }

    #[test]
    fn leftbitreader_read_big() {
        let mut writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.read(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.read(10).ok(), Some(BitVector::new(475, 10)));
        assert_eq!(reader.read(12).ok(), Some(BitVector::new(3784, 12)));
    }

    #[test]
    fn leftbitreader_peek() {
        let mut writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.peek(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.read(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.peek(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
    }

    #[test]
    fn leftbitreader_peek_big() {
        let mut writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.peek(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.read(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.peek(10).ok(), Some(BitVector::new(475, 10)));
        assert_eq!(reader.read(10).ok(), Some(BitVector::new(475, 10)));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(3784, 12)));
        assert_eq!(reader.read(15).ok(), Some(BitVector::new(3784, 12)));
    }

    #[test]
    fn leftbitreader_zeros() {
        let mut writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(32, 16));
        let _ = writer.write(&BitVector::new(8, 5));
        let _ = writer.write(&BitVector::new(0, 3));
        let _ = writer.write(&BitVector::new(1, 3));
        let _ = writer.write(&BitVector::new(0, 3));
        let _ = writer.write(&BitVector::new(3, 2));
        let _ = writer.write(&BitVector::new(0, 3));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.read(16).ok(), Some(BitVector::new(32, 16)));
        assert_eq!(reader.read(5).ok(), Some(BitVector::new(8, 5)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(1, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(3, 2)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0, 3)));
    }

    #[test]
    fn leftbitreader_skip() {
        let mut writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.peek(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.skip(1).ok(), Some(1));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.skip(2).ok(), Some(2));
        assert_eq!(reader.peek(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.skip(3).ok(), Some(3));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.skip_to_byte().ok(), Some(2));
    }

    #[test]
    fn leftbitreader_skip_big() {
        let mut writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.peek(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.skip(20).ok(), Some(20));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(3784, 12)));
        assert_eq!(reader.skip_to_byte().ok(), Some(4));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(200, 8)));
    }

    #[test]
    fn rightbitreader_read() {
        let mut writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.read(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
    }

    #[test]
    fn rightbitreader_read_big() {
        let mut writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.read(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.read(10).ok(), Some(BitVector::new(475, 10)));
        assert_eq!(reader.read(12).ok(), Some(BitVector::new(3784, 12)));
    }

    #[test]
    fn rightbitreader_peek() {
        let mut writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.peek(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.read(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.peek(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
    }

    #[test]
    fn rightbitreader_peek_big() {
        let mut writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.peek(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.read(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.peek(10).ok(), Some(BitVector::new(475, 10)));
        assert_eq!(reader.read(10).ok(), Some(BitVector::new(475, 10)));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(3784, 12)));
        assert_eq!(reader.read(15).ok(), Some(BitVector::new(3784, 12)));
    }

    #[test]
    fn rightbitreader_zeros() {
        let mut writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(32, 16));
        let _ = writer.write(&BitVector::new(8, 5));
        let _ = writer.write(&BitVector::new(0, 3));
        let _ = writer.write(&BitVector::new(1, 3));
        let _ = writer.write(&BitVector::new(0, 3));
        let _ = writer.write(&BitVector::new(3, 2));
        let _ = writer.write(&BitVector::new(0, 3));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.read(16).ok(), Some(BitVector::new(32, 16)));
        assert_eq!(reader.read(5).ok(), Some(BitVector::new(8, 5)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(1, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(3, 2)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0, 3)));
    }

    #[test]
    fn rightbitreader_skip() {
        let mut writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.peek(1).ok(), Some(BitVector::new(0b1, 1)));
        assert_eq!(reader.skip(1).ok(), Some(1));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.skip(2).ok(), Some(2));
        assert_eq!(reader.peek(3).ok(), Some(BitVector::new(0b011, 3)));
        assert_eq!(reader.skip(3).ok(), Some(3));
        assert_eq!(reader.peek(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.skip_to_byte().ok(), Some(2));
    }

    #[test]
    fn rightbitreader_skip_big() {
        let mut writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let _ = writer.write(&BitVector::new(975, 10));
        let _ = writer.write(&BitVector::new(475, 10));
        let _ = writer.write(&BitVector::new(3784, 12));

        let mut cursor = writer.into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.peek(10).ok(), Some(BitVector::new(975, 10)));
        assert_eq!(reader.skip(20).ok(), Some(20));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(3784, 12)));
        assert_eq!(reader.skip_to_byte().ok(), Some(4));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(0xEC, 8)));
    }

    #[test]
    fn lefthuffman_encode_new() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder = LeftHuffmanEncoder::new(writer, &vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab[0], None);
        assert_eq!(tab[1], Some(BitVector::new(0b1100, 4)));
        assert_eq!(tab[2], Some(BitVector::new(0b1101, 4)));
        assert_eq!(tab[3], Some(BitVector::new(0b1110, 4)));
        assert_eq!(tab[4], Some(BitVector::new(0b1111, 4)));
        assert_eq!(tab[5], Some(BitVector::new(0b100, 3)));
        assert_eq!(tab[6], Some(BitVector::new(0b101, 3)));
        assert_eq!(tab[7], Some(BitVector::new(0b00, 2)));
        assert_eq!(tab[8], Some(BitVector::new(0b01, 2)));
        assert_eq!(tab.len(), 9);
    }

    #[test]
    fn lefthuffman_encode_write() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = LeftHuffmanEncoder::new(writer, &vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        for c in "abccddeeeeffffgggggggghhhhhhhh".bytes() {
            let _ = hencoder.enc(&(c as u32 - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1100, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1101, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1110, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1110, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
    }

    #[test]
    fn lefthuffman_encode_new_zero() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder = LeftHuffmanEncoder::new(writer, &vec![0_u8, 0_u8, 0_u8, 0_u8]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab.len(), 0);
    }

    #[test]
    fn righthuffman_encode_new() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder = RightHuffmanEncoder::new(writer, &vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab[0], None);
        assert_eq!(tab[1], Some(BitVector::new(0b0011, 4)));
        assert_eq!(tab[2], Some(BitVector::new(0b1011, 4)));
        assert_eq!(tab[3], Some(BitVector::new(0b0111, 4)));
        assert_eq!(tab[4], Some(BitVector::new(0b1111, 4)));
        assert_eq!(tab[5], Some(BitVector::new(0b001, 3)));
        assert_eq!(tab[6], Some(BitVector::new(0b101, 3)));
        assert_eq!(tab[7], Some(BitVector::new(0b00, 2)));
        assert_eq!(tab[8], Some(BitVector::new(0b10, 2)));
        assert_eq!(tab.len(), 9);
    }

    #[test]
    fn righthuffman_encode_write() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = RightHuffmanEncoder::new(writer, &vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        for c in "abccddeeeeffffgggggggghhhhhhhh".bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b0011, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1011, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b0111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b0111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
    }

    #[test]
    fn righthuffman_encode_new_zero() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder = RightHuffmanEncoder::new(writer, &vec![0_u8, 0_u8, 0_u8, 0_u8]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab.len(), 0);
    }

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
