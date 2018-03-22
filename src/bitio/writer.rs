//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use action::Action;
use bitio::direction::Direction;
use bitio::small_bit_vec::SmallBitVec;
use core::borrow::BorrowMut;
use core::marker::PhantomData;
use core::mem::size_of;
use core::ops::{BitOr, Shl, Shr};
use num_traits::{cast, NumCast};
use num_traits::sign::Unsigned;

pub trait BitWriteExt<T, I>
where
    T: Copy
        + BitOr<Output = T>
        + From<u8>
        + Shl<usize, Output = T>
        + Shr<usize, Output = T>
        + Unsigned
        + NumCast,
    I: Iterator<Item = SmallBitVec<T>>,
{
    fn to_bytes<D: Direction, W: BorrowMut<BitWriter<D>>>(
        self,
        writer: W,
        action: Action,
    ) -> BitIterator<T, D, I, W>;
}

impl<T, I> BitWriteExt<T, I::IntoIter> for I
where
    T: Copy
        + BitOr<Output = T>
        + From<u8>
        + Shl<usize, Output = T>
        + Shr<usize, Output = T>
        + Unsigned
        + NumCast,
    I: IntoIterator<Item = SmallBitVec<T>>,
{
    fn to_bytes<D: Direction, W: BorrowMut<BitWriter<D>>>(
        self,
        writer: W,
        action: Action,
    ) -> BitIterator<T, D, I::IntoIter, W> {
        BitIterator::<T, D, I::IntoIter, W>::new(
            self.into_iter(),
            writer,
            action,
        )
    }
}

pub struct BitIterator<T, D, I, W>
where
    T: Copy
        + BitOr<Output = T>
        + From<u8>
        + Shl<usize, Output = T>
        + Shr<usize, Output = T>
        + Unsigned
        + NumCast,
    D: Direction,
    I: Iterator<Item = SmallBitVec<T>>,
    W: BorrowMut<BitWriter<D>>,
{
    writer: W,
    inner: I,
    action: Action,
    buf: T,
    buflen: usize,
    finished: bool,
    phantom: PhantomData<fn() -> D>,
}

impl<T, D, I, W> BitIterator<T, D, I, W>
where
    T: Copy
        + BitOr<Output = T>
        + From<u8>
        + Shl<usize, Output = T>
        + Shr<usize, Output = T>
        + Unsigned
        + NumCast,
    D: Direction,
    I: Iterator<Item = SmallBitVec<T>>,
    W: BorrowMut<BitWriter<D>>,
{
    fn new(inner: I, writer: W, action: Action) -> Self {
        Self {
            writer,
            inner,
            action,
            buf: T::zero(),
            buflen: 0,
            finished: false,
            phantom: PhantomData,
        }
    }
}

impl<T, D, I, W> Iterator for BitIterator<T, D, I, W>
where
    T: Copy
        + BitOr<Output = T>
        + From<u8>
        + Shl<usize, Output = T>
        + Shr<usize, Output = T>
        + Unsigned
        + NumCast,
    D: Direction,
    I: Iterator<Item = SmallBitVec<T>>,
    W: BorrowMut<BitWriter<D>>,
{
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        while self.buflen == 0 {
            let s = match self.inner.next() {
                Some(ref s) => self.writer.borrow_mut().write_bits(s),
                None => {
                    if self.finished {
                        self.finished = false;
                        return None;
                    } else if Action::Flush == self.action
                        || Action::Finish == self.action
                    {
                        self.finished = true;
                        match self.writer.borrow_mut().flush::<T>() {
                            Some((x, y)) if y != 0 => (x, y),
                            _ => return None,
                        }
                    } else {
                        return None;
                    }
                }
            };
            self.buf = s.0;
            self.buflen = s.1;
        }

        let ret = cast::<T, u8>(D::convert(
            self.buf,
            size_of::<T>() << 3,
            size_of::<u8>() << 3,
        )).unwrap();

        self.buf = D::forward(self.buf, size_of::<u8>() << 3);
        self.buflen -= 1;
        Some(ret)
    }
}

#[derive(Clone)]
pub struct BitWriter<D: Direction> {
    buf: u8,
    counter: usize,
    phantom: PhantomData<fn() -> D>,
}

impl<D: Direction> Default for BitWriter<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: Direction> BitWriter<D> {
    pub fn new() -> Self {
        Self {
            buf: 0,
            counter: 0,
            phantom: PhantomData,
        }
    }

    pub fn write_bits<T>(&mut self, data: &SmallBitVec<T>) -> (T, usize)
    where
        T: Copy
            + BitOr<Output = T>
            + From<u8>
            + Shl<usize, Output = T>
            + Shr<usize, Output = T>
            + Unsigned
            + NumCast,
    {
        if data.is_empty() {
            return (T::zero(), 0);
        }

        let len = data.len();
        let data = D::convert(*data.data_ref(), len, size_of::<T>() << 3);
        let clen = len + self.counter;

        let wdata = D::convert::<T>(
            From::from(self.buf),
            size_of::<u8>() << 3,
            size_of::<T>() << 3,
        ) | D::backward(data, self.counter);

        let wlen = clen >> 3;

        self.buf = cast::<T, u8>(D::convert(
            if wlen == 0 {
                wdata
            } else {
                D::forward(data, (wlen << 3) - self.counter)
            },
            size_of::<T>() << 3,
            size_of::<u8>() << 3,
        )).unwrap();
        self.counter = clen - (wlen << 3);
        (wdata, wlen)
    }

    pub fn flush<T>(&mut self) -> Option<(T, usize)>
    where
        T: Copy
            + BitOr<Output = T>
            + From<u8>
            + Shl<usize, Output = T>
            + Shr<usize, Output = T>
            + Unsigned
            + NumCast,
    {
        if self.counter > 0 {
            let c = (size_of::<u8>() << 3) - self.counter;
            Some(self.write_bits::<T>(&SmallBitVec::new(T::zero(), c)))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;
    use bitio::direction::left::Left;
    use bitio::direction::right::Right;

    #[test]
    fn leftbitwriter_write() {
        let mut writer = BitWriter::<Left>::new();
        let ret = vec![
            SmallBitVec::new(0b1_u32, 1),
            SmallBitVec::new(0b10, 2),
            SmallBitVec::new(0b011, 3),
            SmallBitVec::new(0b00, 2),
        ].to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(ret, vec![0b1100_1100_u8]);
    }

    #[test]
    fn leftbitwriter_write_big() {
        let mut writer = BitWriter::<Left>::new();
        let ret = vec![
            SmallBitVec::new(975_u32, 10),
            SmallBitVec::new(475, 10),
            SmallBitVec::new(3784, 12),
        ].to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![243, 221, 190, 200]);
    }

    #[test]
    fn leftbitwriter_write_pad() {
        let mut writer = BitWriter::<Left>::new();
        let ret = vec![
            SmallBitVec::new(1_u32, 1),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(3, 3),
        ].to_bytes(&mut writer, Action::Run)
            .collect::<Vec<_>>();
        assert_eq!(ret.len(), 0);
        let ret = vec![SmallBitVec::<u32>::default(); 0]
            .to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![204]);
    }

    #[test]
    fn leftbitwriter_write_1bit() {
        let mut writer = BitWriter::<Left>::new();
        let ret = vec![SmallBitVec::new(1u32, 1)]
            .to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![128]);
    }

    #[test]
    fn leftbitwriter_zero() {
        let mut writer = BitWriter::<Left>::new();
        let ret = vec![
            SmallBitVec::new(0u32, 10),
            SmallBitVec::new(0, 0),
            SmallBitVec::new(0, 1),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 4),
            SmallBitVec::new(0, 12),
        ].to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![0, 0, 0, 0]);
    }

    #[test]
    fn rightbitwriter_write() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![
            SmallBitVec::new(0b1_u32, 1),
            SmallBitVec::new(0b10, 2),
            SmallBitVec::new(0b011, 3),
            SmallBitVec::new(0b00, 2),
        ].to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();

        assert_eq!(ret, vec![0b0001_1101]);
    }

    #[test]
    fn rightbitwriter_write_big() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![
            SmallBitVec::new(975_u32, 10),
            SmallBitVec::new(475, 10),
            SmallBitVec::new(3784, 12),
        ].to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![0xCF, 0x6F, 0x87, 0xEC]);
    }

    #[test]
    fn rightbitwriter_write_pad() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![
            SmallBitVec::new(1_u32, 1),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(3, 3),
        ].to_bytes(&mut writer, Action::Run)
            .collect::<Vec<_>>();
        assert_eq!(ret.len(), 0);
        let ret = vec![SmallBitVec::<u32>::default(); 0]
            .to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![0b0001_1101]);
    }

    #[test]
    fn rightbitwriter_write_pad_8() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![
            SmallBitVec::new(1_u8, 1),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(3, 3),
        ].to_bytes(&mut writer, Action::Run)
            .collect::<Vec<_>>();
        assert_eq!(ret.len(), 0);
        let ret = vec![SmallBitVec::<u8>::default(); 0]
            .to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![0b0001_1101]);
    }

    #[test]
    fn rightbitwriter_write_pad_16() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![
            SmallBitVec::new(1_u16, 1),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(3, 3),
        ].to_bytes(&mut writer, Action::Run)
            .collect::<Vec<_>>();
        assert_eq!(ret.len(), 0);
        let ret = vec![SmallBitVec::<u16>::default(); 0]
            .to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![0b0001_1101]);
    }

    #[test]
    fn rightbitwriter_write_pad_64() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![
            SmallBitVec::new(1_u64, 1),
            SmallBitVec::new(2, 2),
            SmallBitVec::new(3, 3),
        ].to_bytes(&mut writer, Action::Run)
            .collect::<Vec<_>>();
        assert_eq!(ret.len(), 0);
        let ret = vec![SmallBitVec::<u64>::default(); 0]
            .to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![0b0001_1101]);
    }

    #[test]
    fn rightbitwriter_write_1bit() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![SmallBitVec::new(1u32, 1)]
            .to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![1]);
    }

    #[test]
    fn rightbitwriter_zero() {
        let mut writer = BitWriter::<Right>::new();
        let ret = vec![
            SmallBitVec::new(0u32, 10),
            SmallBitVec::new(0, 0),
            SmallBitVec::new(0, 1),
            SmallBitVec::new(0, 2),
            SmallBitVec::new(0, 3),
            SmallBitVec::new(0, 4),
            SmallBitVec::new(0, 12),
        ].to_bytes(&mut writer, Action::Flush)
            .collect::<Vec<_>>();
        assert_eq!(ret, vec![0, 0, 0, 0]);
    }
}
