//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::borrow::ToOwned;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::direction::Direction;
use bitio::small_bit_vec::{SmallBitVec, SmallBitVecReverse};
use core::marker::PhantomData;
use core::ops::{Add, Shl};
use huffman::create_huffman_table;
use num_traits::{cast, NumCast};

pub struct HuffmanEncoder<D: Direction, T> {
    bit_vec_tab: Vec<Option<SmallBitVec<T>>>,
    phantom: PhantomData<fn() -> D>,
}

impl<D, T> HuffmanEncoder<D, T>
where
    D: Direction,
    T: Clone + PartialOrd<T> + Shl<u8, Output = T> + Add<Output = T> + From<u8>,
    SmallBitVec<T>: SmallBitVecReverse,
{
    pub fn new(symb_len: &[u8]) -> Self {
        Self {
            bit_vec_tab: create_huffman_table(symb_len, D::is_reverse()),
            phantom: PhantomData,
        }
    }

    pub fn enc<U: NumCast + Clone>(
        &self,
        data: &U,
    ) -> Result<SmallBitVec<T>, String> {
        if let Some(idx) = cast::<_, usize>(data.clone()) {
            if idx < self.bit_vec_tab.len() {
                if let Some(ref bv) = self.bit_vec_tab[idx] {
                    return Ok(bv.clone());
                }
            }
        }
        Err("out of value(huffman encodeing)".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitio::direction::left::Left;
    use bitio::direction::right::Right;

    #[test]
    fn lefthuffman_encode_new() {
        let hencoder =
            HuffmanEncoder::<Left, u16>::new(&[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        let tab = hencoder.bit_vec_tab;

        assert_eq!(tab[0], None);
        assert_eq!(tab[1], Some(SmallBitVec::new(0b1100, 4)));
        assert_eq!(tab[2], Some(SmallBitVec::new(0b1101, 4)));
        assert_eq!(tab[3], Some(SmallBitVec::new(0b1110, 4)));
        assert_eq!(tab[4], Some(SmallBitVec::new(0b1111, 4)));
        assert_eq!(tab[5], Some(SmallBitVec::new(0b100, 3)));
        assert_eq!(tab[6], Some(SmallBitVec::new(0b101, 3)));
        assert_eq!(tab[7], Some(SmallBitVec::new(0b00, 2)));
        assert_eq!(tab[8], Some(SmallBitVec::new(0b01, 2)));
        assert_eq!(tab.len(), 9);
    }

    #[test]
    fn lefthuffman_encode_write() {
        let hencoder =
            HuffmanEncoder::<Left, u16>::new(&[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);

        assert_eq!(
            hencoder.enc(&(b'a' - 0x60)).ok(),
            Some(SmallBitVec::new(0b1100, 4))
        );
        assert_eq!(
            hencoder.enc(&(b'b' - 0x60)).ok(),
            Some(SmallBitVec::new(0b1101, 4))
        );
        assert_eq!(
            hencoder.enc(&(b'c' - 0x60)).ok(),
            Some(SmallBitVec::new(0b1110, 4))
        );
        assert_eq!(
            hencoder.enc(&(b'd' - 0x60)).ok(),
            Some(SmallBitVec::new(0b1111, 4))
        );
        assert_eq!(
            hencoder.enc(&(b'e' - 0x60)).ok(),
            Some(SmallBitVec::new(0b100, 3))
        );
        assert_eq!(
            hencoder.enc(&(b'f' - 0x60)).ok(),
            Some(SmallBitVec::new(0b101, 3))
        );
        assert_eq!(
            hencoder.enc(&(b'g' - 0x60)).ok(),
            Some(SmallBitVec::new(0b00, 2))
        );
        assert_eq!(
            hencoder.enc(&(b'h' - 0x60)).ok(),
            Some(SmallBitVec::new(0b01, 2))
        );
    }

    #[test]
    fn lefthuffman_encode_new_zero() {
        let hencoder =
            HuffmanEncoder::<Left, u16>::new(&[0_u8, 0_u8, 0_u8, 0_u8]);
        let tab = hencoder.bit_vec_tab;

        assert_eq!(tab.len(), 0);
    }

    #[test]
    fn lefthuffman_encode_all() {
        let hencoder = HuffmanEncoder::<Left, u16>::new(&[8; 256]);
        let tab = hencoder.bit_vec_tab;

        for i in 0..256 {
            assert_eq!(tab[i as usize], Some(SmallBitVec::new(i, 8)));
        }
        assert_eq!(tab.len(), 256);
    }

    #[test]
    fn righthuffman_encode_new() {
        let hencoder =
            HuffmanEncoder::<Right, u16>::new(&[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        let tab = hencoder.bit_vec_tab;

        assert_eq!(tab[0], None);
        assert_eq!(tab[1], Some(SmallBitVec::new(0b0011, 4)));
        assert_eq!(tab[2], Some(SmallBitVec::new(0b1011, 4)));
        assert_eq!(tab[3], Some(SmallBitVec::new(0b0111, 4)));
        assert_eq!(tab[4], Some(SmallBitVec::new(0b1111, 4)));
        assert_eq!(tab[5], Some(SmallBitVec::new(0b001, 3)));
        assert_eq!(tab[6], Some(SmallBitVec::new(0b101, 3)));
        assert_eq!(tab[7], Some(SmallBitVec::new(0b00, 2)));
        assert_eq!(tab[8], Some(SmallBitVec::new(0b10, 2)));
        assert_eq!(tab.len(), 9);
    }

    #[test]
    fn righthuffman_encode_write() {
        let hencoder =
            HuffmanEncoder::<Right, u16>::new(&[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);

        assert_eq!(
            b"abcdefgh"
                .iter()
                .map(|x| x - 0x60)
                .map(|x| hencoder.enc(&x).unwrap())
                .collect::<Vec<_>>(),
            vec![
                SmallBitVec::new(0b0011, 4),
                SmallBitVec::new(0b1011, 4),
                SmallBitVec::new(0b0111, 4),
                SmallBitVec::new(0b1111, 4),
                SmallBitVec::new(0b001, 3),
                SmallBitVec::new(0b101, 3),
                SmallBitVec::new(0b00, 2),
                SmallBitVec::new(0b10, 2),
            ]
        );
    }

    #[test]
    fn righthuffman_encode_new_zero() {
        let hencoder = HuffmanEncoder::<Right, u16>::new(&[0_u8, 0, 0, 0]);
        let tab = hencoder.bit_vec_tab;

        assert_eq!(tab.len(), 0);
    }
}
