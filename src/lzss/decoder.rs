//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use cbuffer::CircularBuffer;
use error::CompressionError;
use lzss::LzssCode;
use traits::decoder::Decoder;

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
    use action::Action;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;
    use lzss::encoder::LzssEncoder;
    use lzss::tests::comparison;
    use traits::encoder::Encoder;

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
