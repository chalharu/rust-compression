//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use core::borrow::BorrowMut;
use core::marker::PhantomData;
use error::CompressionError;

pub trait DecodeExt<I>
where
    I: Iterator<Item = u8>,
{
    fn decode<E>(self, decoder: &mut E) -> DecodeIterator<I, E, E::Reader>
    where
        E: Decoder<I>,
        CompressionError: From<E::Error>;
}

impl<I> DecodeExt<I::IntoIter> for I
where
    I: IntoIterator<Item = u8>,
{
    fn decode<E>(
        self,
        decoder: &mut E,
    ) -> DecodeIterator<I::IntoIter, E, E::Reader>
    where
        E: Decoder<I::IntoIter>,
        CompressionError: From<E::Error>,
    {
        DecodeIterator::<I::IntoIter, E, E::Reader>::new(
            E::Reader::get_reader(self.into_iter()),
            decoder,
        )
    }
}

pub trait Reader<T> {
    fn get_reader(value: T) -> Self;
}

impl<T> Reader<T> for T {
    fn get_reader(value: T) -> Self {
        value
    }
}

pub struct DecodeIterator<'a, I, E, B>
where
    E: Decoder<I> + 'a,
    B: BorrowMut<E::Reader>,
    CompressionError: From<E::Error>,
{
    decoder: &'a mut E,
    inner: B,
    phantom: PhantomData<fn() -> I>,
}

impl<'a, I, E, B> DecodeIterator<'a, I, E, B>
where
    E: Decoder<I>,
    B: BorrowMut<E::Reader>,
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

impl<'a, I, E, B> Iterator for DecodeIterator<'a, I, E, B>
where
    E: Decoder<I>,
    B: BorrowMut<E::Reader>,
    CompressionError: From<E::Error>,
{
    type Item = Result<E::Output, E::Error>;

    fn next(&mut self) -> Option<Result<E::Output, E::Error>> {
        match self.decoder.next(self.inner.borrow_mut()) {
            Ok(Some(s)) => Some(Ok(s)),
            Ok(None) => None,
            Err(s) => Some(Err(s)),
        }
    }
}

pub trait Decoder<I>
where
    CompressionError: From<Self::Error>,
    Self::Reader: Reader<I>,
{
    type Error;
    type Output;
    type Reader;

    fn next(
        &mut self,
        iter: &mut Self::Reader,
    ) -> Result<Option<Self::Output>, Self::Error>;

    fn iter<'a>(
        &'a mut self,
        reader: &'a mut Self::Reader,
    ) -> DecodeIterator<I, Self, &'a mut Self::Reader>
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
