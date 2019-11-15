//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use crate::cbuffer::CircularBuffer;
use crate::error::CompressionError;
use crate::lzss::LzssCode;
use crate::traits::decoder::Decoder;

/// # Examples
///
/// ```rust
/// use compression::prelude::*;
/// use std::cmp::Ordering;
///
/// fn main() {
///     pub fn comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
///         match (lhs, rhs) {
///             (
///                 LzssCode::Reference {
///                     len: llen,
///                     pos: lpos,
///                 },
///                 LzssCode::Reference {
///                     len: rlen,
///                     pos: rpos,
///                 },
///             ) => ((llen << 3) + rpos).cmp(&((rlen << 3) + lpos)).reverse(),
///             (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
///             (_, LzssCode::Symbol(_)) => Ordering::Greater,
///             (LzssCode::Symbol(_), _) => Ordering::Less,
///         }
///     }
///     # #[cfg(feature = "lzss")]
///     let compressed = b"aabbaabbaabbaabb\n"
///         .into_iter()
///         .cloned()
///         .encode(&mut LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3), Action::Finish)
///         .collect::<Result<Vec<_>, _>>()
///         .unwrap();
///
///     # #[cfg(feature = "lzss")]
///     let decompressed = compressed
///         .iter()
///         .cloned()
///         .decode(&mut LzssDecoder::new(0x1_0000))
///         .collect::<Result<Vec<_>, _>>()
///         .unwrap();
/// }
/// ```
#[derive(Debug)]
pub struct LzssDecoder {
    buf: CircularBuffer<u8>,
    offset: usize,
}

impl LzssDecoder {
    pub fn new(size_of_window: usize) -> Self {
        Self {
            buf: CircularBuffer::new(size_of_window),
            offset: 0,
        }
    }

    pub fn with_dict(size_of_window: usize, dict: &[u8]) -> Self {
        let mut buf = CircularBuffer::new(size_of_window);
        buf.append(dict);
        Self { buf, offset: 0 }
    }
}

impl Decoder for LzssDecoder {
    type Input = LzssCode;
    type Error = CompressionError;
    type Output = u8;

    fn next<I: Iterator<Item = Self::Input>>(
        &mut self,
        s: &mut I,
    ) -> Option<Result<Self::Output, Self::Error>> {
        while self.offset == 0 {
            match s.next() {
                Some(s) => match s {
                    LzssCode::Symbol(s) => {
                        self.buf.push(s);
                        self.offset += 1;
                    }
                    LzssCode::Reference { len, pos } => {
                        self.offset += len;
                        for _ in 0..len {
                            let d = self.buf[pos];
                            self.buf.push(d);
                        }
                    }
                },
                None => return None,
            }
        }
        self.offset -= 1;
        Some(Ok(self.buf[self.offset]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::lzss::encoder::LzssEncoder;
    use crate::lzss::tests::comparison;
    use crate::traits::encoder::Encoder;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    #[test]
    fn test() {
        let testvec = b"aabbaabbaaabbbaaabbbaabbaabb";
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = testvec.iter().cloned();
        let enc_ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        let mut decoder = LzssDecoder::new(0x1_0000);
        let mut dec_iter = enc_ret.into_iter();
        let ret = (0..)
            .scan((), |_, _| decoder.next(&mut dec_iter))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(testvec.to_vec(), ret);
    }
}
