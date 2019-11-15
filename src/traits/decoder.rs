//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use crate::bitio::direction::Direction;
use crate::bitio::reader::BitReader;
use crate::core::borrow::BorrowMut;
use crate::core::marker::PhantomData;
use crate::error::CompressionError;
use cfg_if::cfg_if;

pub trait DecodeExt<I>
where
    I: Iterator,
{
    fn decode<D: Decoder<Input = I::Item>>(
        self,
        decoder: &mut D,
    ) -> DecodeIterator<'_, I, D, I>
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
    ) -> DecodeIterator<'_, I::IntoIter, D, I::IntoIter>
    where
        CompressionError: From<D::Error>,
    {
        DecodeIterator::<I::IntoIter, D, I::IntoIter>::new(
            self.into_iter(),
            decoder,
        )
    }
}

#[derive(Debug)]
pub struct DecodeIterator<'a, I, D, B>
where
    I: Iterator<Item = D::Input>,
    D: Decoder,
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

impl<I, D, B> Iterator for DecodeIterator<'_, I, D, B>
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

cfg_if! {
    if #[cfg(any(feature = "bzip2", feature="deflate", feature="lzhuf"))] {
        #[derive(Debug)]
        pub(crate) struct BitDecoder<T, R, B>
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

        #[cfg(any(feature = "bzip2", feature="deflate"))]
        impl<T> BitDecoder<T, BitReader<T::Direction>, T>
        where
            T: BitDecodeService + Default,
            CompressionError: From<T::Error>,
            T::Direction: BorrowMut<T::Direction>,
            BitReader<T::Direction>: BorrowMut<BitReader<T::Direction>>,
        {
            pub(crate) fn new() -> Self {
                Self {
                    reader: BitReader::new(),
                    service: T::default(),
                    phantom: PhantomData,
                }
            }
        }

        #[cfg(any(feature="zlib", feature="deflate", feature="lzhuf"))]
        impl<T, R, B> BitDecoder<T, R, B>
        where
            T: BitDecodeService,
            CompressionError: From<T::Error>,
            R: BorrowMut<BitReader<T::Direction>>,
            B: BorrowMut<T>,
        {
            pub(crate) fn with_service(service: B, reader: R) -> Self {
                Self {
                    reader,
                    service,
                    phantom: PhantomData,
                }
            }
        }
        impl<T> Default for BitDecoder<T, BitReader<T::Direction>, T>
        where
            T: BitDecodeService + Default,
            CompressionError: From<T::Error>,
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

        pub(crate) type BitDecoderImpl<T> =
            BitDecoder<T, BitReader<<T as BitDecodeService>::Direction>, T>;
    }
}

pub(crate) trait BitDecodeService
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
