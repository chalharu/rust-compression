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

pub trait DecodeExt<I, R>
where
    I: Iterator,
{
    fn decode<E: Decoder<R>>(self, decoder: &mut E) -> DecodeIterator<R, E, R>
    where
        CompressionError: From<E::Error>;
}

impl<I, D> DecodeExt<I::IntoIter, BitReader<D, I::IntoIter>> for I
where
    D: Direction,
    I: IntoIterator<Item = u8>,
{
    fn decode<E: Decoder<BitReader<D, I::IntoIter>>>(
        self,
        decoder: &mut E,
    ) -> DecodeBitIterator<D, I::IntoIter, E>
    where
        D: Direction,
        E: Decoder<BitReader<D, I::IntoIter>>,
        CompressionError: From<E::Error>,
    {
        DecodeIterator::<
            BitReader<D, I::IntoIter>,
            E,
            BitReader<D, I::IntoIter>,
        >::new(BitReader::<D, _>::new(self.into_iter()), decoder)
    }
}

type DecodeBitIterator<'a, D, I, E> =
    DecodeIterator<'a, BitReader<D, I>, E, BitReader<D, I>>;

pub struct DecodeIterator<'a, R, E, B>
where
    E: Decoder<R> + 'a,
    B: BorrowMut<R>,
    CompressionError: From<E::Error>,
{
    decoder: &'a mut E,
    inner: B,
    phantom: PhantomData<fn() -> R>,
}

impl<'a, R, E, B> DecodeIterator<'a, R, E, B>
where
    E: Decoder<R>,
    B: BorrowMut<R>,
    CompressionError: From<E::Error>,
{
    fn new(inner: B, decoder: &'a mut E) -> Self {
        Self {
            decoder,
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, R, E, B> Iterator for DecodeIterator<'a, R, E, B>
where
    E: Decoder<R>,
    B: BorrowMut<R>,
    CompressionError: From<E::Error>,
{
    type Item = Result<E::Output, E::Error>;

    fn next(&mut self) -> Option<Result<E::Output, E::Error>> {
        match self.decoder.next(&mut self.inner.borrow_mut()) {
            Ok(Some(s)) => Some(Ok(s)),
            Ok(None) => None,
            Err(s) => Some(Err(s)),
        }
    }
}

pub trait Decoder<R>
where
    CompressionError: From<Self::Error>,
{
    type Error;
    type Output;
    fn next(
        &mut self,
        iter: &mut R,
    ) -> Result<Option<Self::Output>, Self::Error>;

    fn iter<'a>(
        &'a mut self,
        reader: &'a mut R,
    ) -> DecodeIterator<R, Self, &'a mut R>
    where
        Self: Sized,
    {
        DecodeIterator {
            decoder: self,
            inner: reader,
            phantom: PhantomData,
        }
    }
}
