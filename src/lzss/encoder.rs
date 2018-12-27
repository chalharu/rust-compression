//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use action::Action;
#[cfg(not(feature = "std"))]
use alloc::collections::vec_deque::VecDeque;
use core::cmp::{self, Ordering};
use lzss::LzssCode;
use lzss::compare_match_info;
use lzss::slidedict::SlideDict;
#[cfg(feature = "std")]
use std::collections::vec_deque::VecDeque;

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

    #[cfg(any(feature = "deflate", test))]
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
        let info = self.slide
            .search_dic(self.offset, self.max_match);

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
                if let Some(item) = self.slide
                    .search_dic(self.offset - i, self.max_match)
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
                    pos: info.pos as usize - 1,
                }),
            }
            self.lzss_queue.push_back(LzssCode::Reference {
                len: out_info.len,
                pos: out_info.pos as usize - 1,
            });
            self.offset -= out_info.len + lazy_index;
        } else {
            let c = self.slide[self.offset - 1];
            self.lzss_queue.push_back(LzssCode::Symbol(c));
            self.offset -= 1;
        }
    }

    pub fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: &Action,
    ) -> Option<LzssCode> {
        while self.lzss_queue.is_empty() {
            match iter.next() {
                Some(s) => self.next_in(s),
                None => {
                    if self.finished {
                        self.finished = false;
                        return None;
                    } else {
                        if Action::Flush == *action || Action::Finish == *action
                        {
                            self.flush()
                        };
                        self.finished = true;
                    }
                }
            }
        }
        self.lzss_queue.pop_front()
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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;
    use lzss::tests::comparison;

    #[test]
    fn test_unit() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);

        let mut iter = b"a".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        assert_eq!(ret, vec![LzssCode::Symbol(b'a')]);
    }

    #[test]
    fn test_2len() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"a".iter().cloned();
        let mut ret = (0..)
            .scan((), |_, _| encoder.next(&mut iter, &Action::Run))
            .collect::<Vec<_>>();
        let mut iter = b"a".iter().cloned();
        ret.append(&mut (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>());

        assert_eq!(
            ret,
            vec![LzssCode::Symbol(b'a'), LzssCode::Symbol(b'a')]
        );
    }

    #[test]
    fn test_3len() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"aaa".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
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
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
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
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference {
                    len: 10,
                    pos: 0,
                },
            ]
        );
    }

    #[test]
    fn test_middle_repeat() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);

        let mut iter = b"a".iter()
            .cycle()
            .take(256)
            .cloned()
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference {
                    len: 255,
                    pos: 0,
                },
            ]
        );
    }

    #[test]
    fn test_long_repeat() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"a".iter()
            .cycle()
            .take(259)
            .cloned()
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference {
                    len: 256,
                    pos: 0,
                },
                LzssCode::Symbol(b'a'),
                LzssCode::Symbol(b'a'),
            ]
        );
    }

    #[test]
    fn test_long_repeat2() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"a".iter()
            .cycle()
            .take(260)
            .cloned()
            .collect::<Vec<u8>>()
            .into_iter();
        let ret = (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Symbol(b'a'),
                LzssCode::Reference {
                    len: 256,
                    pos: 0,
                },
                LzssCode::Reference { len: 3, pos: 0 },
            ]
        );
    }

    #[test]
    fn test_5() {
        let mut encoder = LzssEncoder::new(comparison, 0x1_0000, 256, 3, 3);
        let mut iter = b"aaabbbaaabbb".iter().cloned();
        let ret = (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
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
        let mut iter = b"aabbaabbaaabbbaaabbbaabbaabb"
            .iter()
            .cloned();
        let ret = (0..)
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
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
                LzssCode::Reference {
                    len: 10,
                    pos: 5,
                },
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
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        let mut result = (0..256)
            .map(|x| LzssCode::Symbol(x as u8))
            .collect::<Vec<_>>();
        result.append(&mut vec![
            LzssCode::Reference {
                len: 256,
                pos: 255
            };
            255
        ]);
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
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        let mut result = (0..256)
            .map(|x| LzssCode::Symbol(x as u8))
            .collect::<Vec<_>>();
        result.append(&mut vec![
            LzssCode::Reference {
                len: 256,
                pos: 255
            };
            2
        ]);
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
            &((0..256)
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
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Reference {
                    len: 256,
                    pos: 255
                };
                2
            ]
        );
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
            .scan((), |_, _| {
                encoder.next(&mut iter, &Action::Flush)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ret,
            vec![
                LzssCode::Reference {
                    len: 256,
                    pos: 256,
                },
                LzssCode::Reference {
                    len: 256,
                    pos: 255,
                },
            ]
        );
    }
}
