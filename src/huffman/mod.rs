//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(any(feature = "bzip2", feature = "deflate", feature = "lzhuf"))]

pub(crate) mod cano_huff_table;
pub(crate) mod decoder;
pub(crate) mod encoder;

use crate::bitio::small_bit_vec::{SmallBitVec, SmallBitVecReverse};
use crate::bucket_sort::BucketSort;
use crate::core::ops::{Add, Shl};
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

fn create_huffman_table<
    T: PartialOrd<T> + Shl<u8, Output = T> + Clone + From<u8> + Add<Output = T>,
>(
    symb_len: &[u8],
    is_reverse: bool,
) -> Vec<Option<SmallBitVec<T>>>
where
    SmallBitVec<T>: SmallBitVecReverse,
{
    let symbs = symb_len
        .iter()
        .enumerate()
        .filter(|&(_, &t)| t != 0)
        .collect::<Vec<_>>();
    if !symbs.is_empty() {
        let min_symb = symbs[0].0;
        let max_symb = symbs.last().unwrap().0;
        symbs
            .bucket_sort_all_by_key(|x| *x.1)
            .into_iter()
            .scan((0, T::from(0)), move |c, (s, &l)| {
                let code = c.1.clone() << if c.0 < l { l - c.0 } else { 0 };
                *c = (l, code.clone() + T::from(1));
                Some((
                    s,
                    if is_reverse {
                        SmallBitVec::<T>::new(code, l as usize).reverse()
                    } else {
                        SmallBitVec::<T>::new(code, l as usize)
                    },
                ))
            })
            .collect::<Vec<_>>()
            .bucket_sort_by_key(|x| x.0, min_symb, max_symb)
            .into_iter()
            .scan(0, move |c, (s, v)| {
                let r = vec![None; s - *c].into_iter().chain(vec![Some(v)]);
                *c = s + 1;
                Some(r)
            })
            .flatten()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::action::Action;
    use crate::bitio::direction::left::Left;
    use crate::bitio::direction::right::Right;
    use crate::bitio::direction::Direction;
    use crate::bitio::reader::BitReader;
    use crate::bitio::writer::{BitWriteExt, BitWriter};
    use crate::huffman::decoder::HuffmanDecoder;
    use crate::huffman::encoder::HuffmanEncoder;
    #[cfg(not(feature = "std"))]
    #[allow(unused_imports)]
    use alloc::vec;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    fn enc_and_dec_checker<D: Direction>(
        symb_len: &[u8],
        testarray: &[u16],
        stab_bits: usize,
    ) {
        let hencoder = HuffmanEncoder::<D, u16>::new(symb_len);
        let mut hdecoder =
            HuffmanDecoder::<D>::new(symb_len, stab_bits).unwrap();

        let mut writer = BitWriter::<D>::new();
        let mut vec = testarray
            .iter()
            .map(|c| hencoder.enc(*c).unwrap())
            .to_bytes(&mut writer, Action::Flush);

        let mut reader = BitReader::<D>::new();

        let mut ac = Vec::<u16>::new();
        while let Ok(Some(c)) = hdecoder.dec(&mut reader, &mut vec) {
            ac.push(c);
        }
        assert_eq!(ac, testarray.to_vec());
    }

    #[test]
    fn lefthuffman_decode() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let test_array = "abccddeeeeffffgggggggghhhhhhhh"
            .bytes()
            .map(|b| u16::from(b - 0x60))
            .collect::<Vec<u16>>();

        enc_and_dec_checker::<Left>(&symb_len, &test_array, 4);
    }

    #[test]
    fn lefthuffman_decode_big() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let test_array = "abccddeeeeffffgggggggghhhhhhhh"
            .bytes()
            .map(|b| u16::from(b - 0x60))
            .collect::<Vec<u16>>();

        enc_and_dec_checker::<Left>(&symb_len, &test_array, 2);
    }

    #[test]
    fn righthuffman_decode() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let test_array = "abccddeeeeffffgggggggghhhhhhhh"
            .bytes()
            .map(|b| u16::from(b - 0x60))
            .collect::<Vec<u16>>();

        enc_and_dec_checker::<Right>(&symb_len, &test_array, 4);
    }

    #[test]
    fn righthuffman_decode_big() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let test_array = "abccddeeeeffffgggggggghhhhhhhh"
            .bytes()
            .map(|b| u16::from(b - 0x60))
            .collect::<Vec<u16>>();

        enc_and_dec_checker::<Right>(&symb_len, &test_array, 2);
    }
}
