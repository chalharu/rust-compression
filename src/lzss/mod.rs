//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(any(feature = "deflate", feature = "lzhuf"))]

mod slidedict;
pub mod encoder;
pub mod decoder;

use core::cmp::Ordering;

#[derive(Clone, Debug, PartialEq)]
pub enum LzssCode {
    Symbol(u8),
    Reference { len: usize, pos: usize },
}

impl Default for LzssCode {
    fn default() -> Self {
        LzssCode::Symbol(0)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MatchInfo {
    pub len: usize,
    pub pos: u16,
}

fn compare_match_info<F: Fn(LzssCode, LzssCode) -> Ordering>(
    comp: F,
    arg1: &MatchInfo,
    arg2: &MatchInfo,
) -> Ordering {
    comp(
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

#[cfg(test)]
mod tests {
    use super::*;

    pub fn comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
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
            ) => ((llen << 3) + rpos).cmp(&((rlen << 3) + lpos)).reverse(),
            (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
            (_, LzssCode::Symbol(_)) => Ordering::Greater,
            (LzssCode::Symbol(_), _) => Ordering::Less,
        }
    }
}
