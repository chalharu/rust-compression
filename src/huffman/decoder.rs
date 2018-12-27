//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::borrow::ToOwned;
#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::direction::Direction;
use bitio::reader::BitRead;
use bitio::small_bit_vec::{SmallBitVec, SmallBitVecReverse};
use core::cmp;
use core::marker::PhantomData;
use core::ops::{Add, BitAnd, Shl, Shr, Sub};
use huffman::create_huffman_table;

pub struct HuffmanDecoder<D: Direction> {
    stab_bits: usize,
    stab: Vec<SymbolTableItem>,
    phantom: PhantomData<fn() -> D>,
}

#[derive(Clone, PartialEq)]
enum HuffmanLeaf {
    Leaf(u16),
    Branch(Box<HuffmanLeaf>, Box<HuffmanLeaf>),
    None,
}

impl HuffmanLeaf {
    #[inline]
    pub fn new() -> Self {
        HuffmanLeaf::None
    }

    pub fn add<T>(
        &mut self,
        code: &SmallBitVec<T>,
        value: u16,
    ) -> Result<(), String>
    where
        T: BitAnd<Output = T>
            + Clone
            + Shr<usize, Output = T>
            + From<u8>
            + PartialEq<T>,
    {
        if code.is_empty() {
            *self = HuffmanLeaf::Leaf(value);
        } else {
            if let HuffmanLeaf::None = *self {
                *self = HuffmanLeaf::Branch(
                    Box::new(Self::new()),
                    Box::new(Self::new()),
                );
            } else if let HuffmanLeaf::Leaf(_) = *self {
                return Err("ignore huffman table".to_owned());
            }

            if let HuffmanLeaf::Branch(ref mut lft, ref mut rgt) = *self {
                let next = SmallBitVec::<T>::new(
                    code.data_ref().clone() >> 1,
                    code.len() - 1,
                );
                try!(
                    if (code.data_ref().clone() & T::from(1)) == T::from(0) {
                        lft
                    } else {
                        rgt
                    }.add(&next, value)
                );
            } else {
                unreachable!();
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
enum SymbolTableItem {
    Short(u16, u8),
    Long(HuffmanLeaf),
    None,
}

impl<D: Direction> HuffmanDecoder<D> {
    pub fn new(symb_len: &[u8], mut stab_bits: usize) -> Result<Self, String> {
        let max_len = symb_len
            .iter()
            .cloned()
            .max()
            .unwrap_or_else(|| 0) as usize;
        stab_bits = cmp::min(max_len, stab_bits);

        if max_len < 16 {
            Self::new_t::<u16, _>(symb_len, stab_bits, |d| d as usize)
        } else if max_len < 32 {
            Self::new_t::<u32, _>(symb_len, stab_bits, |d| d as usize)
        } else {
            Err("length error".to_owned())
        }
    }

    fn new_t<T, F>(
        symb_len: &[u8],
        stab_bits: usize,
        cast_to_usize: F,
    ) -> Result<Self, String>
    where
        T: Add<Output = T>
            + BitAnd<Output = T>
            + Clone
            + PartialOrd<T>
            + Shl<u8, Output = T>
            + Shl<usize, Output = T>
            + Shr<usize, Output = T>
            + Sub<Output = T>
            + From<u8>,
        F: Fn(T) -> usize,
        SmallBitVec<T>: SmallBitVecReverse,
    {
        let huff_tab = create_huffman_table::<T>(symb_len, false);
        let mut stab = vec![SymbolTableItem::None; 1 << stab_bits];
        for (i, h) in huff_tab.into_iter().enumerate() {
            if let Some(b) = h {
                if stab_bits >= b.len() {
                    let ld = stab_bits - b.len();
                    let head = cast_to_usize(if !D::is_reverse() {
                        b.data_ref().clone() << ld
                    } else {
                        b.reverse().data_ref().clone()
                    });
                    for j in 0..(1 << ld) {
                        if !D::is_reverse() {
                            stab[head | j] =
                                SymbolTableItem::Short(i as u16, b.len() as u8);
                        } else {
                            stab[head | (j << b.len())] =
                                SymbolTableItem::Short(i as u16, b.len() as u8);
                        }
                    }
                } else {
                    let ld = b.len() - stab_bits;
                    let head = if !D::is_reverse() {
                        b.data_ref().clone() >> ld
                    } else {
                        b.reverse().data_ref().clone()
                            & ((T::from(1) << stab_bits) - T::from(1))
                    };
                    let body = SmallBitVec::new(
                        b.reverse().data_ref().clone() >> stab_bits,
                        ld,
                    );
                    match &mut stab[cast_to_usize(head)] {
                        &mut SymbolTableItem::Short(_, _) => unreachable!(),
                        &mut SymbolTableItem::Long(ref mut store) => {
                            try!(store.add(&body, i as u16));
                        }
                        d => {
                            let mut l = HuffmanLeaf::new();
                            try!(l.add(&body, i as u16));
                            *d = SymbolTableItem::Long(l);
                        }
                    }
                }
            }
        }
        Ok(Self {
            stab_bits,
            stab,
            phantom: PhantomData,
        })
    }

    pub fn dec<R: BitRead<D>>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<u16>, String> {
        let c = try!(reader.peek_bits::<usize>(self.stab_bits));
        if c.is_empty() {
            return Ok(None);
        }
        let c = if !D::is_reverse() {
            *c.data_ref() << (self.stab_bits - c.len())
        } else {
            *c.data_ref()
        };
        if let SymbolTableItem::Short(ref v, ref l) = self.stab[c] {
            try!(reader.skip_bits(*l as usize));
            Ok(Some(*v))
        } else if let SymbolTableItem::Long(ref leaf) = self.stab[c] {
            try!(reader.skip_bits(self.stab_bits));
            let mut lleaf = leaf;

            // 32ビット以上はエラーとするコードもあるが、
            // そもそもハフマンテーブル自体そこまで長く作成できない。
            loop {
                match *lleaf {
                    HuffmanLeaf::Leaf(v) => return Ok(Some(v)),
                    HuffmanLeaf::Branch(ref lft, ref rgt) => {
                        lleaf = if let Ok(b) = reader.read_bits::<u8>(1) {
                            if *b.data_ref() == 0 {
                                lft
                            } else {
                                rgt
                            }
                        } else {
                            return Err("reader error".to_owned());
                        };
                    }
                    HuffmanLeaf::None => {
                        return Err("huffman table error".to_owned())
                    }
                }
            }
        } else {
            unreachable!();
        }
    }
}
