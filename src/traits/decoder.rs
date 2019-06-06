//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use bitio::direction::Direction;
use bitio::reader::BitReader;
use core::borrow::BorrowMut;
use core::marker::PhantomData;
use error::CompressionError;

pub trait DecodeExt<I>
where
    I: Iterator,
{
    fn decode<D: Decoder<Input = I::Item>>(
        self,
        decoder: &mut D,
    ) -> DecodeIterator<I, D, I>
    where
        CompressionError: From<D::Error>;
}

impl<I> DecodeExt<I::IntoIter> for I
where
    I: IntoIterator,
{
    fn decode<D: Decoder<Input = I::Item>>(
        self,
        decoder: &mut D,
    ) -> DecodeIterator<I::IntoIter, D, I::IntoIter>
    where
        CompressionError: From<D::Error>,
    {
        DecodeIterator::<I::IntoIter, D, I::IntoIter>::new(
            self.into_iter(),
            decoder,
        )
    }
}

pub struct DecodeIterator<'a, I, D, B>
where
    I: Iterator<Item = D::Input>,
    D: Decoder + 'a,
    B: BorrowMut<I>,
    CompressionError: From<D::Error>,
{
    decoder: &'a mut D,
    inner: B,
    phantom: PhantomData<fn() -> I>,
}

impl<'a, I, D, B> DecodeIterator<'a, I, D, B>
where
    I: Iterator<Item = D::Input>,
    D: Decoder,
    B: BorrowMut<I>,
    CompressionError: From<D::Error>,
{
    pub(crate) fn new(inner: B, decoder: &'a mut D) -> Self {
        Self {
            decoder,
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, I, D, B> Iterator for DecodeIterator<'a, I, D, B>
where
    I: Iterator<Item = D::Input>,
    D: Decoder,
    B: BorrowMut<I>,
    CompressionError: From<D::Error>,
{
    type Item = Result<D::Output, D::Error>;

    fn next(&mut self) -> Option<Result<D::Output, D::Error>> {
        self.decoder.next(self.inner.borrow_mut())
    }
}

pub trait Decoder
where
    CompressionError: From<Self::Error>,
{
    type Error;
    type Input;
    type Output;
    fn next<I: Iterator<Item = Self::Input>>(
        &mut self,
        iter: &mut I,
    ) -> Option<Result<Self::Output, Self::Error>>;
}

pub struct BitDecoder<T, R, B>
where
    T: BitDecodeService,
    CompressionError: From<T::Error>,
    R: BorrowMut<BitReader<T::Direction>>,
    B: BorrowMut<T>,
{
    reader: R,
    service: B,
    phantom: PhantomData<fn() -> T>,
}

impl<T> BitDecoder<T, BitReader<T::Direction>, T>
where
    T: BitDecodeService,
    CompressionError: From<T::Error>,
    T: Default,
    T::Direction: BorrowMut<T::Direction>,
    BitReader<T::Direction>: BorrowMut<BitReader<T::Direction>>,
{
    pub fn new() -> Self {
        Self {
            reader: BitReader::new(),
            service: T::default(),
            phantom: PhantomData,
        }
    }
}

impl<T, R, B> BitDecoder<T, R, B>
where
    T: BitDecodeService,
    CompressionError: From<T::Error>,
    R: BorrowMut<BitReader<T::Direction>>,
    B: BorrowMut<T>,
{
    pub fn with_service(service: B, reader: R) -> Self {
        Self {
            reader,
            service,
            phantom: PhantomData,
        }
    }
}
impl<T> Default for BitDecoder<T, BitReader<T::Direction>, T>
where
    T: BitDecodeService,
    CompressionError: From<T::Error>,
    T: Default,
    T::Direction: BorrowMut<T::Direction>,
    BitReader<T::Direction>: BorrowMut<BitReader<T::Direction>>,
{
    fn default() -> Self {
        Self {
            reader: BitReader::new(),
            service: T::default(),
            phantom: PhantomData,
        }
    }
}

impl<T> From<T> for BitDecoder<T, BitReader<T::Direction>, T>
where
    T: BitDecodeService,
    CompressionError: From<T::Error>,
    T::Direction: BorrowMut<T::Direction>,
    BitReader<T::Direction>: BorrowMut<BitReader<T::Direction>>,
{
    fn from(iter: T) -> Self {
        Self {
            reader: BitReader::<T::Direction>::new(),
            service: iter,
            phantom: PhantomData,
        }
    }
}

impl<T, R, B> Decoder for BitDecoder<T, R, B>
where
    T: BitDecodeService,
    CompressionError: From<T::Error>,
    R: BorrowMut<BitReader<T::Direction>>,
    B: BorrowMut<T>,
{
    type Error = T::Error;
    type Input = u8;
    type Output = T::Output;
    fn next<I: Iterator<Item = Self::Input>>(
        &mut self,
        iter: &mut I,
    ) -> Option<Result<Self::Output, Self::Error>> {
        self.service
            .borrow_mut()
            .next(self.reader.borrow_mut(), iter)
            .transpose()
    }
}

pub trait BitDecodeService
where
    Self::Direction: Direction,
    CompressionError: From<Self::Error>,
{
    type Direction;
    type Error;
    type Output;
    fn next<I: Iterator<Item = u8>>(
        &mut self,
        reader: &mut BitReader<Self::Direction>,
        iter: &mut I,
    ) -> Result<Option<Self::Output>, Self::Error>;
}

pub type BitDecoderImpl<T> =
    BitDecoder<T, BitReader<<T as BitDecodeService>::Direction>, T>;
