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
    I: Iterator,
{
    fn encode<E: Encoder<In = I::Item>>(
        self,
        encoder: &mut E,
        action: Action,
    ) -> EncodeIterator<I, E>
    where
        CompressionError: From<E::Error>;
}

impl<I> EncodeExt<I::IntoIter> for I
where
    I: IntoIterator,
{
    fn encode<E: Encoder<In = I::Item>>(
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
    I: Iterator<Item = E::In>,
    E: Encoder + 'a,
    CompressionError: From<E::Error>,
{
    encoder: &'a mut E,
    action: Action,
    inner: I,
}

impl<'a, I, E> EncodeIterator<'a, I, E>
where
    I: Iterator<Item = E::In>,
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
    I: Iterator<Item = E::In>,
    E: Encoder,
    CompressionError: From<E::Error>,
{
    type Item = Result<E::Out, E::Error>;

    fn next(&mut self) -> Option<Result<E::Out, E::Error>> {
        self.encoder.next(&mut self.inner, self.action)
    }
}

pub trait Encoder
where
    CompressionError: From<Self::Error>,
{
    type Error;
    type In;
    type Out;
    fn next<I: Iterator<Item = Self::In>>(
        &mut self,
        iter: &mut I,
        action: Action,
    ) -> Option<Result<Self::Out, Self::Error>>;
}
