//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use crate::action::Action;
use crate::core::cmp::{self, Ordering};
use crate::error::CompressionError;
use crate::lzss::compare_match_info;
use crate::lzss::slidedict::SlideDict;
use crate::lzss::LzssCode;
use crate::traits::encoder::Encoder;
#[cfg(not(feature = "std"))]
use alloc::collections::vec_deque::VecDeque;
#[cfg(feature = "std")]
use std::collections::vec_deque::VecDeque;

/// # Examples
///
/// ```rust
/// use compression::prelude::*;
/// use std::cmp::Ordering;
///
/// fn main() {
///     pub fn comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
///         match (lhs, rhs) {
///             (
///                 LzssCode::Reference {
///                     len: llen,
///                     pos: lpos,
///                 },
///                 LzssCode::Reference {
///                     len: rlen,
///                     pos: rpos,
///                 },
///             ) => ((llen << 3) + rpos).cmp(&((rlen << 3) + lpos)).reverse(),
///             (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
///             (_, LzssCode::Symbol(_)) => Ordering::Greater,
///             (LzssCode::Symbol(_), _) => Ordering::Less,
///         }
///     }
///     # #[cfg(feature = "lzss")]
///     let compressed = b"aabbaabbaabbaabb\n"
///         .into_iter()
///         .cloned()
///         .encode(&mut LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3), Action::Finish)
///         .collect::<Result<Vec<_>, _>>()
///         .unwrap();
///
///     # #[cfg(feature = "lzss")]
///     let decompressed = compressed
///         .iter()
///         .cloned()
///         .decode(&mut LzssDecoder::new(0x1_0000))
///         .collect::<Result<Vec<_>, _>>()
///         .unwrap();
/// }
/// ```
#[derive(Debug)]
pub struct LzssEncoder<F>
where
    F: Fn(LzssCode, LzssCode) -> Ordering + Copy,
{
    slide: SlideDict<F>,
    min_match: usize,
    max_match: usize,
    lazy_level: usize,
    offset: usize,
    comp: F,
    lzss_queue: VecDeque<LzssCode>,
    finished: bool,
}

impl<F> LzssEncoder<F>
where
    F: Fn(LzssCode, LzssCode) -> Ordering + Copy,
{
    pub fn new(
        comp: F,
        size_of_window: usize,
        max_match: usize,
        min_match: usize,
        lazy_level: usize,
    ) -> Self {
        Self {
            slide: SlideDict::new(
                size_of_window + max_match + lazy_level + 1,
                size_of_window,
                min_match,
                comp,
            ),
            max_match,
            min_match,
            lazy_level,
            offset: 0,
            comp,
            lzss_queue: VecDeque::new(),
            finished: false,
        }
    }

    pub fn with_dict(
        comp: F,
        size_of_window: usize,
        max_match: usize,
        min_match: usize,
        lazy_level: usize,
        dict: &[u8],
    ) -> Self {
        let mut slide = SlideDict::new(
            size_of_window + max_match + lazy_level + 1,
            size_of_window,
            min_match,
            comp,
        );
        let dictstart = dict.len() - cmp::min(size_of_window, dict.len());
        slide.append(&dict[dictstart..]);
        Self {
            slide,
            max_match,
            min_match,
            lazy_level,
            offset: 0,
            comp,
            lzss_queue: VecDeque::new(),
            finished: false,
        }
    }

    fn encode(&mut self) {
        let info = self.slide.search_dic(self.offset, self.max_match);

        if let Some(info) = info.and_then(|x| {
            if x.len >= self.min_match {
                Some(x)
            } else {
                None
            }
        }) {
            let lazy_level = cmp::min(info.len, self.lazy_level);
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
                s if s < self.min_match => {
                    for i in (0..s).map(|c| c + 1) {
                        let c = self.slide[self.offset - i];
                        self.lzss_queue.push_back(LzssCode::Symbol(c));
                    }
                }
                _ => self.lzss_queue.push_back(LzssCode::Reference {
                    len: lazy_index,
                    pos: info.pos as usize,
                }),
            }
            self.lzss_queue.push_back(LzssCode::Reference {
                len: out_info.len,
                pos: out_info.pos as usize,
            });
            self.offset -= out_info.len + lazy_index;
        } else {
            let c = self.slide[self.offset - 1];
            self.lzss_queue.push_back(LzssCode::Symbol(c));
            self.offset -= 1;
        }
    }

    fn next_in(&mut self, data: u8) {
        if self.max_match + self.lazy_level > self.offset {
            self.slide.append(&[data]);
            self.offset += 1;
        }
        while self.offset >= self.max_match + self.lazy_level {
            self.encode();
        }
    }

    fn flush(&mut self) {
        while self.offset > 0 {
            self.encode();
        }
    }
}

impl<F> Encoder for LzssEncoder<F>
where
    F: Fn(LzssCode, LzssCode) -> Ordering + Copy,
{
    type Error = CompressionError;
    type In = u8;
    type Out = LzssCode;

    fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: Action,
    ) -> Option<Result<LzssCode, CompressionError>> {
        while self.lzss_queue.is_empty() {
            match iter.next() {
                Some(s) => self.next_in(s),
                None => {
                    if self.finished {
                        self.finished = false;
                        return None;
                    } else {
                        if Action::Flush == action || Action::Finish == action {
                            self.flush()
                        };
                        self.finished = true;
                    }
                }
            }
        }
        self.lzss_queue.pop_front().map(Ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzss::tests::comparison;
    #[cfg(not(feature = "std"))]
    #[allow(unused_imports)]
    use alloc::vec;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    #[test]
    fn test_unit() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);

        let mut iter = b"a".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(ret, vec![LzssCode::Symbol(b'a')]);
    }

    #[test]
    fn test_2len() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"a".iter().cloned();
        let mut ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Run))
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let mut iter = b"a".iter().cloned();
        ret.append(
            &mut (0..)
                .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
                .map(Result::unwrap)
                .collect::<Vec<_>>(),
        );

        assert_eq!(ret, vec![LzssCode::Symbol(b'a'), LzssCode::Symbol(b'a')]);
    }

    #[test]
    fn test_3len() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"aaa".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'a'),
            ]
        );
    }

    #[test]
    fn test_4len() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);

        let mut iter = b"aaaa".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference { len: 3, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_short_len() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"aaaaaaaaaaa".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference { len: 10, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_middle_repeat() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);

        let mut iter = b"a"
            .iter()
            .cycle()
            .take(256)
            .cloned()
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference { len: 255, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_long_repeat() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"a"
            .iter()
            .cycle()
            .take(259)
            .cloned()
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference { len: 256, pos: 0 },
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'a'),
            ]
        );
    }

    #[test]
    fn test_long_repeat2() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"a"
            .iter()
            .cycle()
            .take(260)
            .cloned()
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference { len: 256, pos: 0 },
                LzssCode::Reference { len: 3, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_5() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"aaabbbaaabbb".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'b'),
                LzssCode::Symbol(b'b'),
                LzssCode::Symbol(b'b'),
                LzssCode::Reference { len: 6, pos: 5 },
            ]
        );
    }

    #[test]
    fn test_6() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"aabbaabbaaabbbaaabbbaabbaabb".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'b'),
                LzssCode::Symbol(b'b'),
                LzssCode::Reference { len: 6, pos: 3 },
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'b'),
                LzssCode::Reference { len: 10, pos: 5 },
                LzssCode::Reference { len: 6, pos: 3 },
            ]
        );
    }

    #[test]
    fn test_7() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = (0..256)
            .cycle()
            .take(0x1_0000)
            .map(|x| x as u8)
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        let mut result = (0..256)
            .map(|x| LzssCode::Symbol(x as u8))
            .collect::<Vec<_>>();
        result
            .append(&mut vec![LzssCode::Reference { len: 256, pos: 255 }; 255]);
        assert_eq!(ret, result);
    }

    #[test]
    fn test_8() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = (0..256)
            .cycle()
            .take(768)
            .map(|x| x as u8)
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        let mut result = (0..256)
            .map(|x| LzssCode::Symbol(x as u8))
            .collect::<Vec<_>>();
        result.append(&mut vec![LzssCode::Reference { len: 256, pos: 255 }; 2]);
        assert_eq!(ret, result);
    }

    #[test]
    fn test_9() {
        let mut encoder = LzssEncoder::with_dict(
            comparison,
            0x1_0000,
            256,
            3,
            3,
            &((0..256).map(|x| x as u8).collect::<Vec<u8>>()),
        );

        let mut iter = (0..256)
            .cycle()
            .take(512)
            .map(|x| x as u8)
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(ret, vec![LzssCode::Reference { len: 256, pos: 255 }; 2]);
    }

    #[test]
    fn test_10() {
        let mut encoder = LzssEncoder::with_dict(
            comparison,
            0x1_0000,
            256,
            3,
            3,
            &((0..256)
                .cycle()
                .take(0x1_0001)
                .map(|x| x as u8)
                .collect::<Vec<u8>>()),
        );

        let mut iter = (0..256)
            .cycle()
            .take(512)
            .map(|x| x as u8)
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Reference { len: 256, pos: 256 },
                LzssCode::Reference { len: 256, pos: 255 },
            ]
        );
    }

    #[test]
    fn test_11() {
        let mut source = b"abc".to_vec();
        source.append(
            &mut b"d"
                .iter()
                .cycle()
                .take(0x1_0000 - 3)
                .cloned()
                .collect::<Vec<u8>>(),
        );
        source.append(&mut b"abc".to_vec());
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = source.into_iter();

        let ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, Action::Flush))
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let mut result = vec![
            LzssCode::Symbol(b'a'),
            LzssCode::Symbol(b'b'),
            LzssCode::Symbol(b'c'),
            LzssCode::Symbol(b'd'),
        ];
        result.append(
            &mut [LzssCode::Reference { len: 256, pos: 0 }]
                .iter()
                .cycle()
                .take(255)
                .cloned()
                .collect::<Vec<_>>(),
        );
        result.push(LzssCode::Reference { len: 252, pos: 0 });
        result.push(LzssCode::Reference { len: 3, pos: 65535 });

        assert_eq!(ret, result);
    }
}
