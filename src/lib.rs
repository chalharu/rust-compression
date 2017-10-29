#![crate_type = "lib"]

//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

use std::io::{Read, Write};
use std::cmp::min;

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


trait BitWriter<W: Write> {
    fn write(&mut self, buf: &BitVector) -> std::io::Result<usize>;
    fn pad_flush(&mut self) -> std::io::Result<()>;
    fn get_ref(&self) -> &W;
    fn get_mut(&mut self) -> &mut W;
    fn into_inner(&mut self) -> Result<W, std::io::Error>;
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

impl<W: Write> BitWriter<W> for LeftBitWriter<W> {
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

impl<W: Write> BitWriter<W> for RightBitWriter<W> {
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

trait BitReader<R: Read> {
    fn read(&mut self, len: usize) -> std::io::Result<BitVector>;
    fn peek(&mut self, len: usize) -> std::io::Result<BitVector>;
    fn skip(&mut self, len: usize) -> std::io::Result<usize>;
    fn skip_to_byte(&mut self) -> std::io::Result<usize>;
    fn get_ref(&self) -> &R;
    fn get_mut(&mut self) -> &mut R;
}

pub struct LeftBitReader<R: Read> {
    inner: R,
    buf: u32,
    counter: usize,
}

impl<R: Read> LeftBitReader<R> {
    pub fn new(inner: R) -> Self {
        LeftBitReader {
            inner: inner,
            buf: 0,
            counter: 0,
        }
    }
}
impl<R: Read> BitReader<R> for LeftBitReader<R> {
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
            if let Ok(rlen) = self.inner.read(&mut buf) {
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
            if let Ok(rlen) = self.inner.read(&mut buf) {
                if rlen == 0 {
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
        &self.inner
    }

    fn get_mut(&mut self) -> &mut R {
        &mut self.inner
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
        assert_eq!(reader.skip(10).ok(), Some(10));
        assert_eq!(reader.peek(10).ok(), Some(BitVector::new(475, 10)));
        assert_eq!(reader.skip(10).ok(), Some(10));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(3784, 12)));
        assert_eq!(reader.skip_to_byte().ok(), Some(4));
        assert_eq!(reader.peek(15).ok(), Some(BitVector::new(200, 8)));
    }

}
