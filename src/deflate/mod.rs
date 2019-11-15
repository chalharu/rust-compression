//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(feature = "deflate")]

pub(crate) mod decoder;
pub(crate) mod encoder;

use crate::bitio::small_bit_vec::SmallBitVec;
use crate::core::u16;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

fn fix_symbol_table() -> Vec<u8> {
    let mut r = vec![8; 144];
    r.append(&mut vec![9; 112]);
    r.append(&mut vec![7; 24]);
    r.append(&mut vec![8; 8]);
    r
}

fn fix_offset_table() -> &'static [u8; 32] {
    &[5; 32]
}

#[derive(Debug)]
struct CodeTable {
    codes: Vec<u8>,
    offsets: Vec<u16>,
    ext_bits: Vec<u8>,
}

impl CodeTable {
    fn convert(&self, value: u16) -> (u8, SmallBitVec<u16>) {
        let pos = self.codes[value as usize];
        (
            pos,
            SmallBitVec::new(
                value - self.offsets[pos as usize],
                self.ext_bits(pos as usize),
            ),
        )
    }

    fn ext_bits(&self, pos: usize) -> usize {
        self.ext_bits[pos] as usize
    }

    fn convert_back(&self, pos: usize, ext: u16) -> u16 {
        self.offsets[pos] + ext
    }
}

fn gen_codes(len: usize, offsets: &[u16]) -> Vec<u8> {
    let mut codes = Vec::with_capacity(len);
    let mut j = 0;
    for i in 0..(len as u16) {
        while offsets[j as usize + 1] <= i {
            j += 1;
        }
        codes.push(j);
    }
    codes
}

fn gen_len_tab() -> CodeTable {
    let mut offsets = Vec::with_capacity(30);
    let mut ext_bits = Vec::with_capacity(29);
    for i in 0..8 {
        offsets.push(i);
        ext_bits.push(0);
    }

    for i in 8..28 {
        let n = (i >> 2) - 1;
        offsets.push(u16::from(i & 3 | 4) << n);
        ext_bits.push(n);
    }

    // 28
    offsets.push(255);
    ext_bits.push(0);

    // 29
    offsets.push(u16::MAX);

    let codes = gen_codes(256, &offsets);

    CodeTable {
        codes,
        offsets,
        ext_bits,
    }
}

fn gen_off_tab() -> CodeTable {
    let mut offsets = Vec::with_capacity(31);
    let mut ext_bits = Vec::with_capacity(30);
    for i in 0..4 {
        offsets.push(i);
        ext_bits.push(0);
    }

    for i in 4..30 {
        let n = (i >> 1) - 1;
        offsets.push(u16::from(i & 1 | 2) << n);
        ext_bits.push(n);
    }

    // 30
    offsets.push(u16::MAX);

    let codes = gen_codes(0x8000, &offsets);

    CodeTable {
        codes,
        offsets,
        ext_bits,
    }
}

#[cfg(test)]
mod tests {
    use crate::action::Action;
    use crate::deflate::decoder::Deflater;
    use crate::deflate::encoder::Inflater;
    use crate::traits::decoder::DecodeExt;
    use crate::traits::encoder::EncodeExt;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;
    use rand::distributions::Standard;
    use rand::{thread_rng, Rng};

    fn check(testarray: &[u8]) {
        let encoded = testarray
            .to_vec()
            .encode(&mut Inflater::new(), Action::Finish)
            .collect::<Result<Vec<_>, _>>();
        let decoded = encoded
            .unwrap()
            .decode(&mut Deflater::new())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(testarray.to_vec(), decoded);
    }

    #[test]
    fn test_empty() {
        check(&[]);
    }

    #[test]
    fn test_unit() {
        check(b"a");
    }

    #[test]
    fn test_arr() {
        check(b"aaaaaaaaaaa");
    }

    #[test]
    fn test_std() {
        check(b"aabbaabbaaabbbaaabbbaabbaabb");
    }

    #[test]
    fn test_long() {
        check(&(b"a".iter().cycle().take(260).cloned().collect::<Vec<u8>>()));
    }

    #[test]
    fn test_long2() {
        check(
            &((144..256)
                .cycle()
                .take(224)
                .map(|x| x as u8)
                .collect::<Vec<u8>>()),
        )
    }

    #[test]
    fn test_multiblocks() {
        let rng = thread_rng();

        check(&(rng.sample_iter(&Standard).take(323_742).collect::<Vec<_>>()));
    }

    #[test]
    fn test_multiblocks2() {
        let rng = thread_rng();

        check(&(rng.sample_iter(&Standard).take(323_742).collect::<Vec<_>>()));
    }

    #[test]
    fn test_multiblocks3() {
        let rng = thread_rng();

        check(
            &(rng
                .sample_iter(&Standard)
                .take(0xF_FFFF)
                .collect::<Vec<_>>()),
        );
    }

    fn test_rand_with_len(len: usize) {
        let rng = thread_rng();

        check(&(rng.sample_iter(&Standard).take(len).collect::<Vec<_>>()));
    }

    #[test]
    fn test_multiblocks6() {
        test_rand_with_len(6);
    }

    #[test]
    fn test_multiblocks4() {
        test_rand_with_len(0x10_000);
    }

    #[test]
    fn test_multiblocks5() {
        test_rand_with_len(0x10_0001);
    }
}
