//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use LzssCode;
use Read;
use circular_buffer::CircularBuffer;
use std::io::ErrorKind as ioErrorKind;
use std::io::Read as ioRead;
use std::io::Result as ioResult;

pub struct LzssDecoder<R: Read<LzssCode>> {
    inner: Option<R>,
    buf: CircularBuffer<u8>,
    offset: usize,
}

impl<R: Read<LzssCode>> LzssDecoder<R> {
    pub fn new(inner: R, size_of_window: usize) -> Self {
        Self {
            inner: Some(inner),
            buf: CircularBuffer::new(size_of_window),
            offset: 0,
        }
    }

    fn get_ref(&self) -> &R {
        self.inner.as_ref().unwrap()
    }

    fn get_mut(&mut self) -> &mut R {
        self.inner.as_mut().unwrap()
    }

    fn into_inner(&mut self) -> R {
        self.inner.take().unwrap()
    }
}

impl<R: Read<LzssCode>> ioRead for LzssDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> ioResult<usize> {
        let mut rbuf: [LzssCode; 1] = [Default::default()];
        for i in 0..buf.len() {
            while self.offset == 0 {
                match self.inner.as_mut().unwrap().read(&mut rbuf) {
                    Ok(0) => return Ok(i),
                    Ok(_) => {}
                    Err(ref e) if e.kind() == ioErrorKind::Interrupted => {}
                    Err(e) => return Err(e),
                }

                match rbuf[0] {
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
                }
            }

            self.offset -= 1;
            buf[i] = self.buf[self.offset];
        }
        Ok(buf.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lzss_encoder::LzssEncoder;
    use std::cmp::Ordering;
    use std::io::Write;

    fn comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
        match (lhs, rhs) {
            (LzssCode::Reference {
                 len: llen,
                 pos: lpos,
             },
             LzssCode::Reference {
                 len: rlen,
                 pos: rpos,
             }) => ((llen << 3) - lpos).cmp(&((rlen << 3) - rpos)).reverse(),
            (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
            (_, LzssCode::Symbol(_)) => Ordering::Greater,
            (LzssCode::Symbol(_), _) => Ordering::Less,
        }
    }

    #[test]
    fn test() {
        let testvec = b"aabbaabbaaabbbaaabbbaabbaabb";
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65_536, 256, 3, 3);
        let _ = encoder.write_all(testvec);
        let _ = encoder.flush();

        let mut decoder = LzssDecoder::new(encoder.into_inner(), 65_536);
        let mut ret = Vec::new();
        let _ = decoder.read_to_end(&mut ret);

        assert_eq!(testvec.to_vec(), ret);
    }
}
