//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
//!

use action::Action;
use error::CompressionError;

pub trait EncodeExt<I>
where
    I: Iterator<Item = u8>,
{
    fn encode<E: Encoder>(
        self,
        encoder: &mut E,
        action: Action,
    ) -> EncodeIterator<I, E>
    where
        CompressionError: From<E::Error>;
}

impl<I> EncodeExt<I::IntoIter> for I
where
    I: IntoIterator<Item = u8>,
{
    fn encode<E: Encoder>(
        self,
        encoder: &mut E,
        action: Action,
    ) -> EncodeIterator<I::IntoIter, E>
    where
        CompressionError: From<E::Error>,
    {
        EncodeIterator::<I::IntoIter, E>::new(self.into_iter(), encoder, action)
    }
}

pub struct EncodeIterator<'a, I, E>
where
    I: Iterator<Item = u8>,
    E: Encoder + 'a,
    CompressionError: From<E::Error>,
{
    encoder: &'a mut E,
    action: Action,
    inner: I,
}

impl<'a, I, E> EncodeIterator<'a, I, E>
where
    I: Iterator<Item = u8>,
    E: Encoder,
    CompressionError: From<E::Error>,
{
    fn new(inner: I, encoder: &'a mut E, action: Action) -> Self {
        Self {
            encoder,
            inner,
            action,
        }
    }
}

impl<'a, I, E> Iterator for EncodeIterator<'a, I, E>
where
    I: Iterator<Item = u8>,
    E: Encoder,
    CompressionError: From<E::Error>,
{
    type Item = Result<u8, E::Error>;

    fn next(&mut self) -> Option<Result<u8, E::Error>> {
        self.encoder.next(&mut self.inner, self.action)
    }
}

pub trait Encoder
where
    CompressionError: From<Self::Error>,
{
    type Error;
    fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: Action,
    ) -> Option<Result<u8, Self::Error>>;
}
