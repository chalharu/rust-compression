//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use bit_vector::BitVector;
use internal;
use num_traits::{NumCast, cast};
use std::io::Error as ioError;
use std::io::ErrorKind as ioErrorKind;
use std::io::Result as ioResult;
use write::Write;

pub trait HuffmanEncoder {
    type BW: Write<BitVector>;
    fn enc<T: NumCast + Clone>(&mut self, data: &T) -> ioResult<usize>;
    fn get_enc_tab(&self) -> &[Option<BitVector>];
    fn get_ref(&self) -> &Self::BW;
    fn get_mut(&mut self) -> &mut Self::BW;
    fn into_inner(&mut self) -> Self::BW;
}

macro_rules! huffman_encoder_impl {
    ($name:ident, $is_rev:expr) => {
        pub struct $name<BW: Write<BitVector>> {
            inner: Option<BW>,
            bit_vec_tab: Vec<Option<BitVector>>,
        }

        impl<BW: Write<BitVector>> $name<BW> {
            pub fn new(inner: BW, symb_len: &[u8]) -> Self {
                Self {
                    inner: Some(inner),
                    bit_vec_tab: internal::creat_huffman_table(symb_len, $is_rev),
                }
            }
        }

        impl<BW: Write<BitVector>> HuffmanEncoder for $name<BW> {
            type BW = BW;
            fn enc<
                T: NumCast + Clone
            >(
                &mut self,
                data: &T,
            ) -> ioResult<usize> {
                if let Some(idx) = cast::<_, usize>(data.clone()) {
                    if idx < self.bit_vec_tab.len() {
                        if let Some(ref bv) = self.bit_vec_tab[idx] {
                            return self.inner.as_mut().unwrap().write(bv);
                        }
                    }
                }
                Err(ioError::new(
                    ioErrorKind::Other,
                    "out of value",
                ))
            }

            fn get_enc_tab(&self) -> &[Option<BitVector>] {
                &self.bit_vec_tab
            }

            fn get_ref(&self) -> &Self::BW {
                self.inner.as_ref().unwrap()
            }

            fn get_mut(&mut self) -> &mut Self::BW {
                self.inner.as_mut().unwrap()
            }

            fn into_inner(&mut self) -> Self::BW {
                self.inner.take().unwrap()
            }
        }
    }
}

huffman_encoder_impl!(LeftHuffmanEncoder, false);
huffman_encoder_impl!(RightHuffmanEncoder, true);

#[cfg(test)]
mod tests {
    use super::*;
    use bit_reader::*;
    use bit_writer::*;
    use std::io::Cursor;

    #[test]
    fn lefthuffman_encode_new() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder =
            LeftHuffmanEncoder::new(writer, &[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab[0], None);
        assert_eq!(tab[1], Some(BitVector::new(0b1100, 4)));
        assert_eq!(tab[2], Some(BitVector::new(0b1101, 4)));
        assert_eq!(tab[3], Some(BitVector::new(0b1110, 4)));
        assert_eq!(tab[4], Some(BitVector::new(0b1111, 4)));
        assert_eq!(tab[5], Some(BitVector::new(0b100, 3)));
        assert_eq!(tab[6], Some(BitVector::new(0b101, 3)));
        assert_eq!(tab[7], Some(BitVector::new(0b00, 2)));
        assert_eq!(tab[8], Some(BitVector::new(0b01, 2)));
        assert_eq!(tab.len(), 9);
    }

    #[test]
    fn lefthuffman_encode_write() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder =
            LeftHuffmanEncoder::new(writer, &[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        for c in "abccddeeeeffffgggggggghhhhhhhh".bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = LeftBitReader::new(cursor);

        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1100, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1101, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1110, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1110, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b100, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b01, 2)));
    }

    #[test]
    fn lefthuffman_encode_new_zero() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder =
            LeftHuffmanEncoder::new(writer, &[0_u8, 0_u8, 0_u8, 0_u8]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab.len(), 0);
    }

    #[test]
    fn righthuffman_encode_new() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder =
            RightHuffmanEncoder::new(writer, &[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab[0], None);
        assert_eq!(tab[1], Some(BitVector::new(0b0011, 4)));
        assert_eq!(tab[2], Some(BitVector::new(0b1011, 4)));
        assert_eq!(tab[3], Some(BitVector::new(0b0111, 4)));
        assert_eq!(tab[4], Some(BitVector::new(0b1111, 4)));
        assert_eq!(tab[5], Some(BitVector::new(0b001, 3)));
        assert_eq!(tab[6], Some(BitVector::new(0b101, 3)));
        assert_eq!(tab[7], Some(BitVector::new(0b00, 2)));
        assert_eq!(tab[8], Some(BitVector::new(0b10, 2)));
        assert_eq!(tab.len(), 9);
    }

    #[test]
    fn righthuffman_encode_write() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder =
            RightHuffmanEncoder::new(writer, &[0_u8, 4, 4, 4, 4, 3, 3, 2, 2]);
        for c in "abccddeeeeffffgggggggghhhhhhhh".bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let mut reader = RightBitReader::new(cursor);

        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b0011, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1011, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b0111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b0111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(4).ok(), Some(BitVector::new(0b1111, 4)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b001, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(3).ok(), Some(BitVector::new(0b101, 3)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b00, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
        assert_eq!(reader.read(2).ok(), Some(BitVector::new(0b10, 2)));
    }

    #[test]
    fn righthuffman_encode_new_zero() {
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let hencoder = RightHuffmanEncoder::new(writer, &[0_u8, 0, 0, 0]);
        let tab = hencoder.get_enc_tab();

        assert_eq!(tab.len(), 0);
    }
}
