//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! http://mozilla.org/MPL/2.0/ .

pub mod bucket_sort;
pub mod cano_huff_table;
pub mod circular_buffer;

use bit_vector::BitVector;
use bucket_sort::BucketSort;

pub trait MinValue {
    fn min_value() -> Self;
}

pub trait MaxValue {
    fn max_value() -> Self;
}

impl MinValue for u8 {
    fn min_value() -> Self {
        u8::min_value()
    }
}

impl MaxValue for u8 {
    fn max_value() -> Self {
        u8::max_value()
    }
}

impl MinValue for u16 {
    fn min_value() -> Self {
        u16::min_value()
    }
}

impl MaxValue for u16 {
    fn max_value() -> Self {
        u16::max_value()
    }
}

pub fn creat_huffman_table(
    symb_len: &[u8],
    is_reverse: bool,
) -> Vec<Option<BitVector>> {
    let symbs = symb_len
        .into_iter()
        .enumerate()
        .filter(|&(_, &t)| t != 0)
        .collect::<Vec<_>>();
    if symbs.len() > 0 {
        let min_symb = symbs[0].0;
        let max_symb = symbs.last().unwrap().0;
        symbs
            .bucket_sort_all_by_key(|x| *x.1)
            .into_iter()
            .scan((0, 0), move |c, (s, &l)| {
                let code = c.1 << if c.0 < l { l - c.0 } else { 0 };
                *c = (l, code + 1);
                Some((
                    s,
                    if is_reverse {
                        BitVector::new(code, l as usize).reverse()
                    } else {
                        BitVector::new(code, l as usize)
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
            .flat_map(move |v| v)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    }
}
