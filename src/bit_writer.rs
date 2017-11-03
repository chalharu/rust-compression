//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

use bit_vector::BitVector;
use std::io::Write;

pub trait BitWriter {
    type W: Write;
    fn write(&mut self, buf: &BitVector) -> ::std::io::Result<usize>;
    fn pad_flush(&mut self) -> ::std::io::Result<()>;
    fn get_ref(&self) -> &Self::W;
    fn get_mut(&mut self) -> &mut Self::W;
    fn into_inner(&mut self) -> Result<Self::W, ::std::io::Error>;
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
    fn write(&mut self, data: &BitVector) -> ::std::io::Result<usize> {
        const BIT_LEN: usize = 32 /* u32 */;
        if data.len() == 0 {
            return Ok(0);
        }
        let mut len = data.len();
        let mut data = data.data() << (BIT_LEN - len);
        let mut r = 0;
        while len >= self.counter {
            let result = self.inner.as_mut().unwrap().write(
                &[self.buf | (data >> (BIT_LEN - self.counter)) as u8; 1],
            );
            if let Ok(l) = result {
                if l == 0 {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::WriteZero,
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

    fn pad_flush(&mut self) -> ::std::io::Result<()> {
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

    fn into_inner(&mut self) -> Result<W, ::std::io::Error> {
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
    fn write(&mut self, data: &BitVector) -> ::std::io::Result<usize> {
        const BIT_LEN: usize = 8 /* u8 */;
        let mut len = data.len();
        let mut data = data.data();
        let mut r = 0;
        while len >= self.counter {
            let result = self.inner.as_mut().unwrap().write(
                &[self.buf | (data << (BIT_LEN - self.counter)) as u8; 1],
            );
            if let Ok(l) = result {
                if l == 0 {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::WriteZero,
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

    fn pad_flush(&mut self) -> ::std::io::Result<()> {
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

    fn into_inner(&mut self) -> Result<W, ::std::io::Error> {
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
}
