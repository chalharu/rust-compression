//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use bit_reader::BitReader;
use bit_vector::BitVector;
use internal;
use num_traits::{NumCast, cast};
use std::collections::HashMap;

pub trait HuffmanDecoder {
    type BR: BitReader;
    type Item: Clone + NumCast;

    fn dec(&mut self) -> ::std::io::Result<Self::Item>;
    fn get_ref(&self) -> &Self::BR;
    fn get_mut(&mut self) -> &mut Self::BR;
    fn into_inner(&mut self) -> Self::BR;
}

macro_rules! huffman_decoder_impl {
    ($name:ident, $is_rev:expr) => {
        pub struct $name<BR: BitReader, T: NumCast + Clone> {
            inner: Option<BR>,
            stab_bits: usize,
            stab: Vec<Option<(T, u8)>>,
            long_map: HashMap<BitVector, T>,
        }

        impl<BR: BitReader, T: NumCast + Clone + ::std::fmt::Debug> $name<BR, T> {
            pub fn new(inner: BR, symb_len: &[u8], stab_bits: usize) -> Self {
                const IS_REV: bool = $is_rev;
                let huff_tab = internal::creat_huffman_table(symb_len, IS_REV);
                let mut stab = vec![None; 1 << stab_bits];
                let mut long_map = HashMap::new();
                for (i, h) in huff_tab.into_iter().enumerate() {
                    if let Some(b) = h {
                        let val = cast::<_, T>(i).unwrap();
                        if stab_bits >= b.len() {
                            let ld = stab_bits - b.len();
                            let head =
                                if !IS_REV { b.data() << ld } else { b.data() };
                            for j in 0..(1 << ld) {
                                if !IS_REV {
                                    stab[(head | j) as usize] =
                                        Some((val.clone(), b.len() as u8));
                                } else {
                                    stab[(head | (j << b.len())) as usize] =
                                        Some((val.clone(), b.len() as u8));
                                }
                            }
                        } else {
                            long_map.insert(b, val);
                        }
                    }
                }
                Self {
                    inner: Some(inner),
                    stab_bits,
                    stab,
                    long_map,
                }
            }
        }

        impl<BR: BitReader, T: NumCast + Clone> HuffmanDecoder
            for $name<BR, T> {
            type BR = BR;
            type Item = T;

            fn dec(&mut self) -> ::std::io::Result<Self::Item> {
                match self.inner.as_mut().unwrap().peek(self.stab_bits) {
                    Ok(c) => {
                        let c = if !$is_rev {
                            (c.data() << (self.stab_bits - c.len()))
                        } else {
                            c.data()
                        } as usize;
                        if let Some(ref v) = self.stab[c] {
                            let _ =
                                self.inner.as_mut().unwrap().skip(v.1 as usize);
                            Ok(v.0.clone())
                        } else {
                            let mut l = self.stab_bits;
                            while l < 32 {
                                l += 1;
                                if let Ok(mut b) = self.inner
                                    .as_mut()
                                    .unwrap()
                                    .peek(l)
                                {
                                    if b.len() == l {
                                        if let Some(v) = self.long_map.get(&b) {
                                            let _ = self.inner
                                                .as_mut()
                                                .unwrap()
                                                .skip(b.len());
                                            return Ok(v.clone());
                                        }
                                    } else {
                                        while b.len() < 32 {
                                            l += 1;
                                            b = BitVector::new(
                                                if !$is_rev {
                                                    b.data() << 1
                                                } else {
                                                    b.data()
                                                },
                                                b.len() + 1,
                                            );
                                            if let Some(v) = self.long_map
                                                .get(&b)
                                            {
                                                let _ = self.inner
                                                    .as_mut()
                                                    .unwrap()
                                                    .skip(b.len());
                                                return Ok(v.clone());
                                            }
                                        }
                                        return Err(::std::io::Error::new(
                                            ::std::io::ErrorKind::InvalidData,
                                            "huffman error",
                                        ));
                                    }
                                }
                            }
                            return Err(::std::io::Error::new(
                                ::std::io::ErrorKind::InvalidData,
                                "huffman error",
                            ));
                        }
                    }
                    Err(e) => Err(e),
                }
            }

            fn get_ref(&self) -> &Self::BR {
                self.inner.as_ref().unwrap()
            }

            fn get_mut(&mut self) -> &mut Self::BR {
                self.inner.as_mut().unwrap()
            }

            fn into_inner(&mut self) -> Self::BR {
                self.inner.take().unwrap()
            }
        }
    }
}

huffman_decoder_impl!(LeftHuffmanDecoder, false);
huffman_decoder_impl!(RightHuffmanDecoder, true);

#[cfg(test)]
mod tests {
    use super::*;
    use bit_reader::*;
    use bit_writer::*;
    use huffman_encoder::*;
    use std::io::Cursor;

    #[test]
    fn lefthuffman_decode() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = LeftHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = LeftBitReader::new(cursor);
        let mut hdecoder =
            LeftHuffmanDecoder::<_, u8>::new(reader, &symb_len, 12);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }


    #[test]
    fn lefthuffman_decode_big() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = LeftHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = LeftBitReader::new(cursor);
        let mut hdecoder =
            LeftHuffmanDecoder::<_, u8>::new(reader, &symb_len, 2);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }

    #[test]
    fn righthuffman_decode() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = RightHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = RightBitReader::new(cursor);
        let mut hdecoder =
            RightHuffmanDecoder::<_, u8>::new(reader, &symb_len, 4);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }


    #[test]
    fn righthuffman_decode_big() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let s = "abccddeeeeffffgggggggghhhhhhhh";
        let writer = RightBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut hencoder = RightHuffmanEncoder::new(writer, &symb_len);
        for c in s.bytes() {
            let _ = hencoder.enc(&(c - 0x60));
        }

        let mut cursor = hencoder.into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = RightBitReader::new(cursor);
        let mut hdecoder =
            RightHuffmanDecoder::<_, u8>::new(reader, &symb_len, 2);

        let mut ac = Vec::<u8>::new();
        while let Ok(c) = hdecoder.dec() {
            ac.push(c + 0x60);
        }

        assert_eq!(String::from_utf8(ac).ok().unwrap(), s);
    }
}
