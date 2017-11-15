//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use bit_vector::BitVector;
use std::cmp::min;
use std::io::Error as ioError;
use std::io::ErrorKind as ioErrorKind;
use std::io::Read;
use std::io::Result as ioResult;

pub trait BitReader {
    type R: Read;
    fn read(&mut self, len: usize) -> ioResult<BitVector>;
    fn peek(&mut self, len: usize) -> ioResult<BitVector>;
    fn skip(&mut self, len: usize) -> ioResult<usize>;
    fn skip_to_byte(&mut self) -> ioResult<usize>;
    fn into_inner(&mut self) -> ioResult<Self::R>;
}

#[derive(Clone)]
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

    pub fn get_ref(&self) -> &R {
        self.inner.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut R {
        self.inner.as_mut().unwrap()
    }
}

impl<R: Read> BitReader for LeftBitReader<R> {
    type R = R;
    fn read(&mut self, len: usize) -> ioResult<BitVector> {
        let r = self.peek(len);
        if let Ok(l) = r {
            self.buf <<= l.len();
            self.counter -= l.len();
        }
        r
    }

    fn peek(&mut self, len: usize) -> ioResult<BitVector> {
        while len > self.counter {
            let ls_count = 32 /* u32 */ - 8 /* u8 */ - (self.counter as isize);
            if ls_count < 0 {
                return Err(ioError::new(ioErrorKind::Other, "len is too long"));
            }
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf |= u32::from(buf[0]) << ls_count;
                    self.counter += 8 /* u8 */;
                    continue;
                }
            }
            if self.counter == 0 {
                return Ok(BitVector::new(0, 0));
            }
            break;
        }
        let l = min(len, self.counter);
        Ok(BitVector::new(self.buf >> (32 - l), l))
    }

    fn skip(&mut self, mut len: usize) -> ioResult<usize> {
        let r = Ok(len);
        while len > self.counter {
            len -= self.counter;
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf = u32::from(buf[0]) << (32 /* u32 */ - 8 /* u8 */);
                    self.counter = 8 /* u8 */;
                    continue;
                }
            }
            self.buf = 0;
            self.counter = 0;
            return Err(ioError::new(ioErrorKind::UnexpectedEof, "end of file"));
        }
        self.buf <<= len;
        self.counter -= len;
        r
    }

    fn skip_to_byte(&mut self) -> ioResult<usize> {
        let s_count = self.counter & 0x07;
        self.skip(s_count)
    }

    fn into_inner(&mut self) -> ioResult<R> {
        try!(self.skip_to_byte());
        Ok(self.inner.take().unwrap())
    }
}

#[derive(Clone)]
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

    pub fn get_ref(&self) -> &R {
        self.inner.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut R {
        self.inner.as_mut().unwrap()
    }
}

impl<R: Read> BitReader for RightBitReader<R> {
    type R = R;
    fn read(&mut self, len: usize) -> ioResult<BitVector> {
        let r = self.peek(len);
        if let Ok(l) = r {
            self.buf >>= l.len();
            self.counter -= l.len();
        }
        r
    }

    fn peek(&mut self, len: usize) -> ioResult<BitVector> {
        while len > self.counter {
            if 32 /* u32 */ <= 8 /* u8 */ + self.counter {
                return Err(ioError::new(ioErrorKind::Other, "len is too long"));
            }
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf |= u32::from(buf[0]) << self.counter;
                    self.counter += 8 /* u8 */;
                    continue;
                }
            }
            if self.counter == 0 {
                return Err(
                    ioError::new(ioErrorKind::UnexpectedEof, "end of file"),
                );
            }
            break;
        }
        let l = min(len, self.counter);
        Ok(BitVector::new(self.buf & ((1 << l) - 1), l))
    }

    fn skip(&mut self, mut len: usize) -> ioResult<usize> {
        let r = Ok(len);
        while len > self.counter {
            len -= self.counter;
            let mut buf = [0_u8; 1];
            if let Ok(rlen) = self.inner.as_mut().unwrap().read(&mut buf) {
                if rlen != 0 {
                    self.buf = u32::from(buf[0]);
                    self.counter = 8 /* u8 */;
                    continue;
                }
            }
            self.buf = 0;
            self.counter = 0;
            return Err(ioError::new(ioErrorKind::UnexpectedEof, "end of file"));
        }
        self.buf >>= len;
        self.counter -= len;
        r
    }

    fn skip_to_byte(&mut self) -> ioResult<usize> {
        let s_count = self.counter & 0x07;
        self.skip(s_count)
    }

    fn into_inner(&mut self) -> ioResult<R> {
        try!(self.skip_to_byte());
        Ok(self.inner.take().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_writer::*;
    use std::io::Cursor;
    use write::Write;

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
}
