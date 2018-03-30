//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(feature = "lz4")]

pub mod decoder;
pub mod encoder;

const MEMORY_USAGE: u32 = 14;

const LZ4_MAGIC: u32 = 0x184d2204;
const LZ4_MAX_INPUT_SIZE: u32 = 0x7E000000;

const MINMATCH: u32 = 4;
const COPYLENGTH: u32 = 8;
const LASTLITERALS: u32 = 5;
const MFLIMIT: u32 = COPYLENGTH + MINMATCH;
const LZ4_64KLIMIT: u32 = (1 << 16) + (MFLIMIT - 1);

const HASH_LOG: u32 = MEMORY_USAGE - 2;
const HASH_TABLESIZE: u32 = 1 << HASH_LOG;
const HASH_ADJUST: u32 = (MINMATCH * 8) - HASH_LOG;

const HASH64K_LOG: u32 = HASH_LOG + 1;
const HASH64K_TABLESIZE: u32 = 1 << HASH64K_LOG;
const HASH64K_ADJUST: u32 = (MINMATCH * 8) - HASH64K_LOG;

#[cfg(test)]
mod tests {
    use action::Action;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;
    use lz4::decoder::Lz4Decoder;
    use lz4::encoder::Lz4Encoder;
    use simple_logger;
    use traits::decoder::DecodeExt;
    use traits::encoder::EncodeExt;

    fn setup() {
        let _ = simple_logger::init();
    }

    fn check_unzip(data: &[u8]) {
        let ret = data
            .into_iter()
            .cloned()
            .encode(&mut Lz4Encoder::new(), Action::Finish)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let ret2 = ret
            .iter()
            .cloned()
            .decode(&mut Lz4Decoder::new())
            .collect::<Result<Vec<_>, _>>();
        if let Err(e) = ret2 {
            debug!("{}", e);
        }
        assert!(ret2 == Ok(data.to_vec()), "invalid unzip");
    }

    #[test]
    fn encode() {
        setup();
        check_unzip(include_bytes!("../../data/sample1.ref"));
        check_unzip(include_bytes!("../../data/sample2.ref"));
        check_unzip(include_bytes!("../../data/sample3.ref"));
        check_unzip(include_bytes!("../../data/sample4.ref"));
    }
}
