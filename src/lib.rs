#![crate_type = "lib"]

//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

extern crate num_iter;
extern crate num_traits;

use std::io::{Read, Write};
use std::cmp::min;

use std::ops::{Add, Sub};

use num_traits::{cast, NumCast};

#[derive(Copy, PartialEq, Eq, Clone, Debug)]
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
            if 32 /* u32 */ < 8 /* u8 */ + self.counter {
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
    type Key;
    type Item;
    fn bucket_sort(&self, min: Self::Key, max: Self::Key) -> Vec<Self::Item>;
    fn bucket_sort_all(&self) -> Vec<Self::Item>
    where
        Self::Key: MaxValue + MinValue,
    {
        self.bucket_sort(MinValue::min_value(), MaxValue::max_value())
    }
}

impl<T: Clone + Add + Sub<Output = T> + NumCast, U: Clone> BucketSort for [(T, U)] {
    type Key = T;
    type Item = (T, U);
    fn bucket_sort(&self, min: T, max: T) -> Vec<(T, U)> {
        let mut ret = self.to_vec();
        let mut bucket = vec![0; cast::<T, usize>(max - min.clone()).unwrap() + 2];

        for i in 0..self.len() {
            bucket[cast::<T, usize>(self[i].clone().0 - min.clone()).unwrap() + 1] += 1;
        }
        for i in 2..bucket.len() {
            bucket[i] += bucket[i - 1];
        }
        for i in 0..self.len() {
            let val = self[i].clone();
            let idx = cast::<_, usize>(val.clone().0 - min.clone()).unwrap();
            ret[bucket[idx]] = val;
            bucket[idx] += 1;
        }
        ret
    }
}

pub struct HuffmanEncoder<BW: BitWriter> {
    inner: Option<BW>,
    bit_vec_tab: Vec<Option<BitVector>>,
}

impl<BW: BitWriter> HuffmanEncoder<BW> {
    pub fn new(inner: BW, symb_len: &[u8]) -> Self {
        let symbs = symb_len
            .into_iter()
            .enumerate()
            .filter_map(move |(i, &t)| if t != 0 { Some((t, i)) } else { None })
            .collect::<Vec<_>>()
            .bucket_sort_all()
            .into_iter()
            .scan((0, 0), move |c, (l, s)| {
                let code = c.1 << if c.0 < l { l - c.0 } else { 0 };
                *c = (l, code + 1);
                Some((s, BitVector::new(code, l as usize)))
            })
            .collect::<Vec<_>>()
            .bucket_sort(0, symb_len.len())
            .into_iter()
            .scan(0, move |c, (s, v)| {
                let r = vec![None; s - *c].into_iter().chain(vec![Some(v)]);
                *c = s + 1;
                Some(r)
            })
            .flat_map(move |v| v)
            .collect::<Vec<_>>();

        HuffmanEncoder {
            inner: Some(inner),
            bit_vec_tab: symbs,
        }
    }

    pub fn enc<T: NumCast + Clone>(&mut self, data: &T) -> std::io::Result<usize> {
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

    pub fn get_enc_tab(&self) -> &[Option<BitVector>] {
        &self.bit_vec_tab
    }

    pub fn get_ref(&self) -> &BW {
        self.inner.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut BW {
        self.inner.as_mut().unwrap()
    }

    pub fn into_inner(&mut self) -> BW {
        self.inner.take().unwrap()
    }
}

/*
pub trait HuffmanDecoder<T, BR: BitReader> {
    fn dec(&self) -> std::io::Result<T>;
    fn get_ref(&self) -> &R;
    fn get_mut(&mut self) -> &mut R;
    fn into_inner(&mut self) -> Result<R, std::io::Error>;
}
*/

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
    fn huffman_encode_new() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder = HuffmanEncoder::new(writer, &vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
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
    fn huffman_encode_write() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = HuffmanEncoder::new(writer, &vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        for c in "abccddeeeeffffgggggggghhhhhhhh".chars().into_iter() {
            let _ = hencoder.enc(&(c as u32 - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

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

}
