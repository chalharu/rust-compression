//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use bitio::Direction;
use bitio::reader::{BitRead, BitReader};
use core::marker::PhantomData;
use error::CompressionError;

pub trait DecodeExt<I>
where
    I: Iterator<Item = u8>,
{
    fn decode<D: Direction, E: Decoder<Direction = D>>(
        self,
        decoder: &mut E,
    ) -> DecodeIterator<I, D, E>
    where
        CompressionError: From<E::Error>;
}

impl<I> DecodeExt<I::IntoIter> for I
where
    I: IntoIterator<Item = u8>,
{
    fn decode<D: Direction, E: Decoder<Direction = D>>(
        self,
        decoder: &mut E,
    ) -> DecodeIterator<I::IntoIter, D, E>
    where
        CompressionError: From<E::Error>,
    {
        DecodeIterator::<I::IntoIter, D, E>::new(self.into_iter(), decoder)
    }
}

pub struct DecodeIterator<'a, I, D, E>
where
    I: Iterator<Item = u8>,
    D: Direction,
    E: Decoder<Direction = D> + 'a,
    CompressionError: From<E::Error>,
{
    decoder: &'a mut E,
    inner: BitReader<D, I>,
    phantom: PhantomData<E>,
}

impl<'a, I, D, E> DecodeIterator<'a, I, D, E>
where
    I: Iterator<Item = u8>,
    D: Direction,
    E: Decoder<Direction = D>,
    CompressionError: From<E::Error>,
{
    fn new(inner: I, decoder: &'a mut E) -> Self {
        Self {
            decoder,
            inner: BitReader::<_, _>::new(inner),
            phantom: PhantomData,
        }
    }
}

impl<'a, I, D, E> Iterator for DecodeIterator<'a, I, D, E>
where
    I: Iterator<Item = u8>,
    D: Direction,
    E: Decoder<Direction = D>,
    CompressionError: From<E::Error>,
{
    type Item = Result<E::Item, E::Error>;

    fn next(&mut self) -> Option<Result<E::Item, E::Error>> {
        match self.decoder.next(&mut self.inner) {
            Ok(Some(s)) => Some(Ok(s)),
            Ok(None) => None,
            Err(s) => Some(Err(s)),
        }
    }
}

pub trait Decoder
where
    CompressionError: From<Self::Error>,
{
    type Error;
    type Direction: Direction;
    type Item;
    fn next<R: BitRead<Self::Direction>>(
        &mut self,
        iter: &mut R,
    ) -> Result<Option<Self::Item>, Self::Error>;

    fn iter<'a, R: BitRead<Self::Direction>>(
        &'a mut self,
        reader: &'a mut R,
    ) -> DecoderChainIterator<Self::Direction, Self, R, Self::Item>
    where
        Self: Sized,
    {
        DecoderChainIterator {
            inner: self,
            reader,
        }
    }
}

pub struct DecoderChainIterator<'a, D, E, R, I>
where
    D: Direction,
    E: Decoder<Direction = D, Item = I> + 'a,
    R: BitRead<D> + 'a,
    CompressionError: From<E::Error>,
{
    inner: &'a mut E,
    reader: &'a mut R,
}

impl<'a, D, E, R, I> Iterator for DecoderChainIterator<'a, D, E, R, I>
where
    D: Direction,
    E: Decoder<Direction = D, Item = I> + 'a,
    R: BitRead<D> + 'a,
    CompressionError: From<E::Error>,
{
    type Item = Result<E::Item, E::Error>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next(self.reader) {
            Ok(Some(s)) => Some(Ok(s)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
