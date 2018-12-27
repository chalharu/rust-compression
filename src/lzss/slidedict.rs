//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use cbuffer::CircularBuffer;
use core::cmp::{self, Ordering};
use core::mem;
use core::ops::Index;
use core::slice;
use core::u16;
use lzss::LzssCode;
use lzss::MatchInfo;
use lzss::compare_match_info;

struct HashTab {
    search_tab: Vec<u16>,
    flag_tab: Vec<u8>,
    len: usize,
}

impl HashTab {
    const HASH_SIZE: usize = 16;
    const TAB_LEN: usize = 1 << Self::HASH_SIZE;
    #[cfg(target_pointer_width = "32")]
    const HASH_FRAC: usize = 0x7A7C_4F9F;
    #[cfg(target_pointer_width = "64")]
    const HASH_FRAC: usize = 0x7A7C_4F9F_7A7C_4F9F;
    #[cfg(target_pointer_width = "32")]
    const USIZE_WIDTH: usize = 32;
    #[cfg(target_pointer_width = "64")]
    const USIZE_WIDTH: usize = 64;

    #[cfg(all(not(target_pointer_width = "64"),
              not(target_pointer_width = "32")))]
    fn usize_width() -> usize {
        usize::count_zeros(0_usize)
    }

    #[cfg(any(target_pointer_width = "64", target_pointer_width = "32"))]
    #[inline]
    fn usize_width() -> usize {
        Self::USIZE_WIDTH
    }

    #[cfg(all(not(target_pointer_width = "64"),
              not(target_pointer_width = "32")))]
    const HASH_FRAC: usize = 0x7A7C_4F9F_7A7C_4F9F;

    #[inline]
    pub fn new() -> Self {
        Self {
            search_tab: vec![0_u16; Self::TAB_LEN as usize],
            flag_tab: vec![0_u8; (Self::TAB_LEN as usize) >> 2],
            len: 0,
        }
    }

    #[inline]
    fn gen_change(&mut self) {
        for i in 0..self.flag_tab.len() {
            self.flag_tab[i] = (self.flag_tab[i] & 0b0101_0101) << 1;
        }
        self.len = 0;
    }

    #[inline]
    fn get_hash(data: &[u8]) -> usize {
        let mut hash = 0_usize;
        for d in data {
            hash = (hash << 8) | (hash >> 24) ^ usize::from(*d);
        }
        hash.overflowing_mul(Self::HASH_FRAC).0
            >> (Self::usize_width() - Self::HASH_SIZE)
    }

    #[inline]
    fn push_tab(&mut self, hash: usize) {
        self.search_tab[hash] = self.len as u16;
        self.flag_tab[hash >> 2] |= 1 << ((hash & 0b11) << 1);
        self.len += 1;
        if self.len >= Self::TAB_LEN {
            self.gen_change();
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Option<usize> {
        let hash = Self::get_hash(data);
        let f = (self.flag_tab[hash >> 2] >> ((hash & 0b11) << 1)) & 0b11;
        let ret = if f != 0 {
            let p = self.search_tab[hash] as usize;
            if f & 1 == 1 {
                Some(self.len - p)
            } else {
                Some(Self::TAB_LEN + self.len - p)
            }
        } else {
            None
        };
        self.push_tab(hash);
        ret
    }
}

pub(crate) struct SlideDict<F: Fn(LzssCode, LzssCode) -> Ordering> {
    comparison: F,
    buf: CircularBuffer<u8>,
    pos: CircularBuffer<usize>,
    max_pos: usize,
    min_match: usize,
    hash_tab: HashTab,
    append_buf: Vec<u8>,
}

impl<F: Fn(LzssCode, LzssCode) -> Ordering> SlideDict<F> {
    const MATCH_SEARCH_COUNT: usize = 256;

    pub fn new(
        size_of_buf: usize,
        max_pos: usize,
        min_match: usize,
        comparison: F,
    ) -> Self {
        Self {
            comparison,
            min_match,
            max_pos,
            buf: CircularBuffer::new(size_of_buf),
            pos: CircularBuffer::new(size_of_buf),
            append_buf: Vec::with_capacity(
                size_of_buf - max_pos + min_match - 1,
            ),
            hash_tab: HashTab::new(),
        }
    }

    #[inline]
    fn push_pos(&mut self, data: &[u8]) {
        match self.hash_tab.push(data) {
            Some(pos) => self.pos.push(pos),
            _ => self.pos.push(self.max_pos + 1),
        }
    }

    fn check_match(
        &self,
        mut pos1: usize,
        mut pos2: usize,
        max_match: usize,
    ) -> usize {
        let rawbuf = self.buf.get_raw_ref();
        let icap = rawbuf.len();
        let cap = icap - 1;
        let p = self.buf.get_raw_pos();
        pos1 = if p < pos1 {
            icap + p - pos1
        } else {
            p - pos1
        };
        pos2 = if p < pos2 {
            icap + p - pos2
        } else {
            p - pos2
        };

        if pos1 > pos2 {
            mem::swap(&mut pos1, &mut pos2);
        }

        let mut l = 0;
        while unsafe {
            *rawbuf.get_unchecked(pos1) == *rawbuf.get_unchecked(pos2)
        } && l < max_match
        {
            l += 1;
            if pos2 == cap {
                pos2 = pos1 + 1;
                pos1 = 0;
            } else {
                pos1 += 1;
                pos2 += 1;
            }
        }
        l
    }

    pub fn append(&mut self, data: &[u8]) {
        self.buf.append(data);
        let mm = self.min_match;
        self.append_buf.append(&mut data.to_vec());
        if self.buf.len() >= self.min_match {
            for i in 0..=(self.append_buf.len() - mm) {
                let v = unsafe {
                    slice::from_raw_parts(
                        self.append_buf.as_ptr().add(i),
                        mm,
                    )
                };
                self.push_pos(v);
            }
        }
        if self.append_buf.len() >= self.min_match {
            let bl = self.min_match - 1;
            for i in 0..bl {
                let j = self.append_buf.len() - self.min_match + i + 1;
                self.append_buf[i] = self.append_buf[j];
            }
            unsafe {
                self.append_buf.set_len(bl);
            }
        }
    }

    pub fn search_dic(
        &mut self,
        offset: usize,
        mut max_match: usize,
    ) -> Option<MatchInfo> {
        if offset < self.min_match {
            return None;
        }

        let pos_offset = offset - self.min_match;

        let mut pos = self.pos[pos_offset];
        max_match = cmp::min(max_match, offset);

        let mut info = None;

        let mut pos_count = Self::MATCH_SEARCH_COUNT - 1;

        while pos <= self.max_pos && pos_count > 0 {
            let nlen = self.check_match(offset, offset + pos, max_match);
            let new_info = MatchInfo {
                len: nlen,
                pos: pos as u16,
            };

            info = info.and_then(|iinfo: MatchInfo| {
                if iinfo.len >= nlen
                    || compare_match_info(&self.comparison, &iinfo, &new_info)
                        == Ordering::Less
                {
                    Some(iinfo)
                } else {
                    None
                }
            }).or_else(|| Some(new_info));

            if nlen == max_match {
                pos_count = 0;
            } else {
                pos_count -= 1;
            }

            pos += self.pos[pos_offset + pos as usize];
        }
        info
    }
}

impl<F: Fn(LzssCode, LzssCode) -> Ordering> Index<usize> for SlideDict<F> {
    type Output = u8;

    #[inline]
    fn index(&self, idx: usize) -> &u8 {
        &self.buf[idx]
    }
}
