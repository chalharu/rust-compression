//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use LzssCode;
use Write;
use circular_buffer::CircularBuffer;
use std::cmp::{min, Ordering};
use std::io::Result as ioResult;
use std::io::Write as ioWrite;
use std::mem;
use std::ops::Index;
use std::rc::Rc;
use std::u16;

struct HashTab {
    search_tab: Vec<u16>,
    flag_tab: Vec<u8>,
    len: usize,
}
impl HashTab {
    const HASH_SIZE: usize = 16;
    const TAB_LEN: usize = 1 << Self::HASH_SIZE;
    #[cfg(target_pointer_width = "32")]
    const HASH_FRAC: usize = 0x7A7C4F9F;
    #[cfg(target_pointer_width = "64")]
    const HASH_FRAC: usize = 0x7A7C4F9F7A7C4F9F;
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
    const HASH_FRAC: usize = 0x7A7C4F9F7A7C4F9F;

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
        let f = self.flag_tab[hash >> 2] >> ((hash & 0b11) << 1);
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

#[derive(Clone, Debug)]
struct MatchInfo {
    len: usize,
    pos: u16,
}

struct SlideDict<F: Fn(LzssCode, LzssCode) -> Ordering> {
    comparison: Rc<F>,
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
        comparison: Rc<F>,
    ) -> Self {
        Self {
            comparison,
            min_match,
            max_pos,
            buf: CircularBuffer::new(size_of_buf),
            pos: CircularBuffer::new(size_of_buf),
            append_buf: Vec::new(),
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
        let icap = self.buf.get_raw_ref().len();
        let cap = icap - 1;
        let p = self.buf.get_raw_pos();
        pos1 = if p < pos1 { icap + p - pos1 } else { p - pos1 };
        pos2 = if p < pos2 { icap + p - pos2 } else { p - pos2 };

        if pos1 > pos2 {
            mem::swap(&mut pos1, &mut pos2);
        }

        let mut l = 0;
        while self.buf.get_raw_ref()[pos1] == self.buf.get_raw_ref()[pos2]
            && l < max_match
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
        self.buf.append(&mut data.to_vec());
        let mut buf = self.append_buf.clone();
        let mm = self.min_match;
        buf.append(&mut data.to_vec());
        if self.buf.len() >= self.min_match {
            for i in 0..(buf.len() - mm + 1) {
                self.push_pos(&buf[i..(i + mm)]);
            }
        }
        if buf.len() >= self.min_match {
            self.append_buf = buf[(buf.len() - self.min_match)..].to_vec();
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
        max_match = min(max_match, offset);

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
            }).or(Some(new_info));

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

fn compare_match_info<F: Fn(LzssCode, LzssCode) -> Ordering>(
    comp: &Rc<F>,
    arg1: &MatchInfo,
    arg2: &MatchInfo,
) -> Ordering {
    (comp.as_ref())(
        LzssCode::Reference {
            len: arg1.len,
            pos: arg1.pos as usize,
        },
        LzssCode::Reference {
            len: arg2.len,
            pos: arg2.pos as usize,
        },
    )
}

impl<F: Fn(LzssCode, LzssCode) -> Ordering> Index<usize> for SlideDict<F> {
    type Output = u8;

    #[inline]
    fn index(&self, idx: usize) -> &u8 {
        &self.buf[idx]
    }
}

pub struct LzssEncoder<
    W: Write<LzssCode>,
    F: Fn(LzssCode, LzssCode) -> Ordering,
> {
    inner: Option<W>,
    slide: SlideDict<F>,
    min_match: usize,
    max_match: usize,
    lazy_level: usize,
    offset: usize,
    comp: Rc<F>,
}

impl<W: Write<LzssCode>, F: Fn(LzssCode, LzssCode) -> Ordering>
    LzssEncoder<W, F> {
    pub fn new(
        inner: W,
        comp: F,
        size_of_window: usize,
        max_match: usize,
        min_match: usize,
        lazy_level: usize,
    ) -> Self {
        let comp = Rc::new(comp);
        Self {
            inner: Some(inner),
            slide: SlideDict::new(
                size_of_window + max_match + lazy_level + 1,
                size_of_window,
                min_match,
                comp.clone(),
            ),
            max_match,
            min_match,
            lazy_level,
            offset: 0,
            comp,
        }
    }

    pub fn get_ref(&self) -> &W {
        self.inner.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.inner.as_mut().unwrap()
    }

    pub fn into_inner(&mut self) -> W {
        self.inner.take().unwrap()
    }

    fn encode(&mut self) -> ioResult<()> {
        let info = self.slide.search_dic(self.offset, self.max_match);

        if let Some(info) = info.and_then(|x| if x.len >= self.min_match {
            Some(x)
        } else {
            None
        }) {
            let lazy_level = min(info.len, self.lazy_level);
            let mut out_info = info.clone();
            let mut lazy_index = 0;
            for i in 1..lazy_level {
                if out_info.len >= self.max_match {
                    break;
                }
                if let Some(item) =
                    self.slide.search_dic(self.offset - i, self.max_match)
                {
                    if item.len > self.min_match
                        && compare_match_info(&self.comp, &item, &out_info)
                            == Ordering::Less
                    {
                        out_info = item;
                        lazy_index = i;
                    }
                }
            }

            match lazy_index {
                0 => {}
                1 => if let Err(e) = self.inner
                    .as_mut()
                    .unwrap()
                    .write(&LzssCode::Symbol(self.slide[self.offset - 1]))
                {
                    return Err(e);
                },
                2 => {
                    if let Err(e) = self.inner
                        .as_mut()
                        .unwrap()
                        .write(&LzssCode::Symbol(self.slide[self.offset - 1]))
                    {
                        return Err(e);
                    }
                    if let Err(e) = self.inner
                        .as_mut()
                        .unwrap()
                        .write(&LzssCode::Symbol(self.slide[self.offset - 2]))
                    {
                        return Err(e);
                    }
                }
                _ => if let Err(e) =
                    self.inner.as_mut().unwrap().write(&LzssCode::Reference {
                        len: lazy_index,
                        pos: info.pos as usize - 1,
                    }) {
                    return Err(e);
                },
            }
            if let Err(e) =
                self.inner.as_mut().unwrap().write(&LzssCode::Reference {
                    len: out_info.len,
                    pos: out_info.pos as usize - 1,
                }) {
                return Err(e);
            }
            self.offset -= out_info.len + lazy_index;
        } else {
            if let Err(e) = self.inner
                .as_mut()
                .unwrap()
                .write(&LzssCode::Symbol(self.slide[self.offset - 1]))
            {
                return Err(e);
            }
            self.offset -= 1;
        }
        Ok(())
    }
}

impl<W: Write<LzssCode>, F: Fn(LzssCode, LzssCode) -> Ordering> ioWrite
    for LzssEncoder<W, F> {
    fn write(&mut self, buf: &[u8]) -> ioResult<usize> {
        if buf.is_empty() {
            Ok(0)
        } else {
            let size_of_read = min(
                self.max_match + self.lazy_level - self.offset + 1,
                buf.len(),
            );
            self.slide.append(&buf[..size_of_read]);
            self.offset += size_of_read;
            while self.offset > self.max_match + self.lazy_level {
                if let Err(e) = self.encode() {
                    return Err(e);
                }
            }
            Ok(size_of_read)
        }
    }

    fn flush(&mut self) -> ioResult<()> {
        while self.offset > 0 {
            if let Err(e) = self.encode() {
                return Err(e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
        match (lhs, rhs) {
            (
                LzssCode::Reference {
                    len: llen,
                    pos: lpos,
                },
                LzssCode::Reference {
                    len: rlen,
                    pos: rpos,
                },
            ) => ((llen << 3) - lpos).cmp(&((rlen << 3) - rpos)).reverse(),
            (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
            (_, LzssCode::Symbol(_)) => Ordering::Greater,
            (LzssCode::Symbol(_), _) => Ordering::Less,
        }
    }

    #[test]
    fn test_unit() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(b"a");
        let _ = encoder.flush();

        assert_eq!(encoder.into_inner(), vec![LzssCode::Symbol(b"a"[0])]);
    }

    #[test]
    fn test_2len() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(b"aa");
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![LzssCode::Symbol(b"a"[0]), LzssCode::Symbol(b"a"[0])]
        );
    }

    #[test]
    fn test_3len() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(b"aaa");
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"a"[0]),
            ]
        );
    }

    #[test]
    fn test_4len() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(b"aaaa");
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Reference { len: 3, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_short_len() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(b"aaaaaaaaaaa");
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Reference { len: 10, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_middle_repeat() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(
            &(b"a".into_iter()
                .cycle()
                .take(256)
                .cloned()
                .collect::<Vec<u8>>()),
        );
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Reference { len: 255, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_long_repeat() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(
            &(b"a".into_iter()
                .cycle()
                .take(259)
                .cloned()
                .collect::<Vec<u8>>()),
        );
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Reference { len: 256, pos: 0 },
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"a"[0]),
            ]
        );
    }

    #[test]
    fn test_long_repeat2() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(
            &(b"a".into_iter()
                .cycle()
                .take(260)
                .cloned()
                .collect::<Vec<u8>>()),
        );
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Reference { len: 256, pos: 0 },
                LzssCode::Reference { len: 3, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_5() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(b"aaabbbaaabbb");
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"b"[0]),
                LzssCode::Symbol(b"b"[0]),
                LzssCode::Symbol(b"b"[0]),
                LzssCode::Reference { len: 6, pos: 5 },
            ]
        );
    }

    #[test]
    fn test_6() {
        let mut encoder =
            LzssEncoder::new(Vec::new(), comparison, 65536, 256, 3, 3);
        let _ = encoder.write_all(b"aabbaabbaaabbbaaabbbaabbaabb");
        let _ = encoder.flush();

        assert_eq!(
            encoder.into_inner(),
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"b"[0]),
                LzssCode::Symbol(b"b"[0]),
                LzssCode::Reference { len: 6, pos: 3 },
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Symbol(b"b"[0]),
                LzssCode::Reference { len: 10, pos: 5 },
                LzssCode::Reference { len: 6, pos: 3 },
            ]
        );
    }
}
