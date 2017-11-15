//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use Action;
use Compress;
use LeftBitWriter;
use LzhufCompression;
use LzssCode;
use RcIOQueue;
use lzhuf_encoder::LzhufEncoder;
use lzss_encoder::LzssEncoder;
use std::cmp::Ordering;
use std::io::{ErrorKind, Read, Result, Write};

type Encoder = LzssEncoder<
    LzhufEncoder<LeftBitWriter<RcIOQueue>>,
    fn(LzssCode, LzssCode) -> Ordering,
>;

pub struct LzhufCompress {
    method: LzhufCompression,
    queue: RcIOQueue,
    encoder: Encoder,
    total_in: u64,
    total_out: u64,
}

fn lzss_comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
    match (lhs, rhs) {
        (LzssCode::Reference {
             len: llen,
             pos: lpos,
         },
         LzssCode::Reference {
             len: rlen,
             pos: rpos,
         }) => {
            (((llen as isize) << 3) - lpos as isize)
                .cmp(&(((rlen as isize) << 3) - rpos as isize))
                .reverse()
        }
        (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
        (_, LzssCode::Symbol(_)) => Ordering::Greater,
        (LzssCode::Symbol(_), _) => Ordering::Less,
    }
}

impl LzhufCompress {
    const MIN_MATCH: usize = 3;
    const MAX_MATCH: usize = 256;
    const LAZY_LEVEL: usize = 3;
    pub fn new(method: LzhufCompression) -> Self {
        let dic_len = 1 << method.dictionary_bits();
        let queue = RcIOQueue::new();
        let writer = LeftBitWriter::new(queue.clone());
        let encoder: Encoder = LzssEncoder::new(
            LzhufEncoder::new(
                writer,
                dic_len,
                method.offset_bits(),
                Self::MAX_MATCH,
            ),
            lzss_comparison,
            dic_len,
            Self::MAX_MATCH,
            Self::MIN_MATCH,
            Self::LAZY_LEVEL,
        );
        Self {
            method,
            queue,
            encoder,
            total_in: 0,
            total_out: 0,
        }
    }
}

impl Compress for LzhufCompress {
    fn total_in(&self) -> u64 {
        self.total_in
    }

    fn total_out(&self) -> u64 {
        self.total_out
    }

    fn compress(
        &mut self,
        mut input: &[u8],
        output: &mut [u8],
        action: Action,
    ) -> Result<(usize, usize)> {
        let mut r = 0;
        while !input.is_empty() && output.len() >= self.queue.len() {
            match self.encoder.write(input) {
                Ok(0) => break,
                Ok(n) => {
                    r += n;
                    input = &input[n..];
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        match action {
            Action::Run => {}
            _ => {
                if input.is_empty() {
                    try!(self.encoder.flush());
                }
            }
        }
        let w = try!(self.queue.read(output));

        self.total_in += r as u64;
        self.total_out += w as u64;
        Ok((r, w))
    }
}
