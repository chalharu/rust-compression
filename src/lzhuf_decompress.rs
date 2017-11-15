//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use Action;
use Decompress;
use LeftBitReader;
use LzhufCompression;
use LzssCode;
use RcIOQueue;
use lzhuf_decoder::LzhufDecoder;
use lzss_decoder::LzssDecoder;
use std::cmp::Ordering;
use std::io::{ErrorKind, Read, Result, Write};

pub struct LzhufDecompress {
    method: LzhufCompression,
    queue: RcIOQueue,
    decoder: LzssDecoder<LzhufDecoder<LeftBitReader<RcIOQueue>>>,
    total_in: u64,
    total_out: u64,
}

impl LzhufDecompress {
    const MIN_MATCH: usize = 3;
    pub fn new(method: LzhufCompression) -> Self {
        let queue = RcIOQueue::new();
        let reader = LeftBitReader::new(queue.clone());
        let decoder =
            LzssDecoder::new(
                LzhufDecoder::new(reader, method.offset_bits(), Self::MIN_MATCH),
                1 << method.dictionary_bits(),
            );

        Self {
            method,
            queue,
            decoder,
            total_in: 0,
            total_out: 0,
        }
    }
}

impl Decompress for LzhufDecompress {
    fn total_in(&self) -> u64 {
        self.total_in
    }

    fn total_out(&self) -> u64 {
        self.total_out
    }

    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<(usize, usize)> {
        let r = try!(self.queue.write(input));
        let w = try!(self.decoder.read(output));

        self.total_in += r as u64;
        self.total_out += w as u64;

        Ok((r, w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Compress;
    use lzhuf_compress::LzhufCompress;
    use rand::{Rand, Rng, SeedableRng, XorShiftRng};

    #[test]
    fn test_std() {
        check(b"aabbaabbaaabbbaaabbbaabbaabb" as &[u8]);
    }

    #[test]
    fn test_unit() {
        check(b"a" as &[u8]);
    }

    #[test]
    fn test_empty() {
        check(b"" as &[u8]);
    }

    #[test]
    fn test_long() {
        check(
            &(b"a"
                  .into_iter()
                  .cycle()
                  .take(260)
                  .cloned()
                  .collect::<Vec<u8>>()),
        );
    }

    #[test]
    fn test_multiblocks() {
        let mut rng = XorShiftRng::from_seed(
            [189522394, 1694417663, 1363148323, 4087496301],
        );

        check(&(rng.gen_iter().take(1048576).collect::<Vec<_>>()));
    }

    fn check(testvec: &[u8]) {
        let mut testslice = &testvec[0..];
        let mut encoder = LzhufCompress::new(LzhufCompression::Lh5);
        let mut decoder = LzhufDecompress::new(LzhufCompression::Lh5);
        let mut enc_buf = Vec::with_capacity(2000000);
        let mut dec_buf = Vec::with_capacity(2000000);

        while !testslice.is_empty() {
            let r = encoder
                .compress_vec(&testslice, &mut enc_buf, Action::Finish)
                .ok()
                .unwrap();
            testslice = &testslice[r.0..];
        }
        while encoder
            .compress_vec(testslice, &mut enc_buf, Action::Finish)
            .ok()
            .unwrap()
            .0 != 0
        {}

        let mut encslice = &enc_buf[0..];

        while !encslice.is_empty() {
            let r = decoder
                .decompress_vec(&encslice, &mut dec_buf)
                .ok()
                .unwrap();
            encslice = &encslice[r.0..];
        }
        while decoder
            .decompress_vec(encslice, &mut dec_buf)
            .ok()
            .unwrap()
            .0 != 0
        {}

        assert_eq!(testvec[0..], dec_buf[0..]);
    }
}
