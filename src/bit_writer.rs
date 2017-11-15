//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use bit_vector::BitVector;
use std::io::Error as ioError;
use std::io::ErrorKind as ioErrorKind;
use std::io::Result as ioResult;
use std::io::Write as ioWrite;
use write::Write;

#[derive(Clone)]
pub struct LeftBitWriter<W: ioWrite> {
    inner: Option<W>,
    buf: u8,
    counter: usize,
}

impl<W: ioWrite> LeftBitWriter<W> {
    pub fn new(inner: W) -> Self {
        LeftBitWriter {
            inner: Some(inner),
            buf: 0,
            counter: 8,
        }
    }

    pub fn get_ref(&self) -> &W {
        self.inner.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.inner.as_mut().unwrap()
    }

    pub fn into_inner(&mut self) -> Result<W, ioError> {
        try!(self.flush());
        Ok(self.inner.take().unwrap())
    }
}

impl<W: ioWrite> Write<BitVector> for LeftBitWriter<W> {
    fn write(&mut self, data: &BitVector) -> ioResult<usize> {
        const BIT_LEN: usize = 32 /* u32 */;
        if data.is_empty() {
            return Ok(0);
        }
        let mut len = data.len();
        let mut data = data.data() << (BIT_LEN - len);
        let mut r = 0;
        while len >= self.counter {
            let result = self.inner.as_mut().unwrap().write(
                &[self.buf | (data >> (BIT_LEN - self.counter)) as u8;
                    1],
            );
            if let Ok(l) = result {
                if l == 0 {
                    return Err(ioError::new(
                        ioErrorKind::WriteZero,
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

    fn flush(&mut self) -> ioResult<()> {
        let c = self.counter;
        if c != 8 {
            try!(self.write(&BitVector::new(0, c)));
        }
        self.inner.as_mut().unwrap().flush()
    }
}

impl<W: ioWrite> Drop for LeftBitWriter<W> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            // dtors should not panic, so we ignore a failed flush
            let _r = self.flush();
        }
    }
}

#[derive(Clone)]
pub struct RightBitWriter<W: ioWrite> {
    inner: Option<W>,
    buf: u8,
    counter: usize,
}

impl<W: ioWrite> RightBitWriter<W> {
    pub fn new(inner: W) -> Self {
        RightBitWriter {
            inner: Some(inner),
            buf: 0,
            counter: 8,
        }
    }

    pub fn get_ref(&self) -> &W {
        self.inner.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.inner.as_mut().unwrap()
    }

    pub fn into_inner(&mut self) -> Result<W, ioError> {
        try!(self.flush());
        Ok(self.inner.take().unwrap())
    }
}

impl<W: ioWrite> Write<BitVector> for RightBitWriter<W> {
    fn write(&mut self, data: &BitVector) -> ioResult<usize> {
        const BIT_LEN: usize = 8 /* u8 */;
        let mut len = data.len();
        let mut data = data.data();
        let mut r = 0;
        while len >= self.counter {
            let result = self.inner.as_mut().unwrap().write(
                &[self.buf | (data << (BIT_LEN - self.counter)) as u8;
                    1],
            );
            if let Ok(l) = result {
                if l == 0 {
                    return Err(ioError::new(
                        ioErrorKind::WriteZero,
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

    fn flush(&mut self) -> ioResult<()> {
        let c = self.counter;
        if c != 8 {
            try!(self.write(&BitVector::new(0, c)));
        }
        self.inner.as_mut().unwrap().flush()
    }
}

impl<W: ioWrite> Drop for RightBitWriter<W> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            // dtors should not panic, so we ignore a failed flush
            let _r = self.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leftbitwriter_write() {
        let mut writer = LeftBitWriter::new(Vec::<u8>::new());
        let _ = writer.write(&BitVector::new(0b1, 1));
        let _ = writer.write(&BitVector::new(0b10, 2));
        let _ = writer.write(&BitVector::new(0b011, 3));
        let _ = writer.write(&BitVector::new(0b00, 2));
        assert_eq!(writer.get_ref()[0], 0b1100_1100);
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
        let _ = writer.flush();
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
        assert_eq!(writer.get_ref()[0], 0b0001_1101);
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
        let _ = writer.flush();
        assert_eq!(writer.get_ref()[0], 0b0001_1101);
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
}
