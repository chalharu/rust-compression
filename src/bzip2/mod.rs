//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(feature = "bzip2")]

pub mod decoder;
pub mod encoder;
pub mod error;
mod mtf;

const HEADER_B: u8 = 0x42;
const HEADER_Z: u8 = 0x5a;
#[allow(non_upper_case_globals)]
const HEADER_h: u8 = 0x68;
const HEADER_0: u8 = 0x30;

const BZ_G_SIZE: usize = 50;

#[cfg(test)]
mod tests {
    use action::Action;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;
    use bzip2::decoder::BZip2Decoder;
    use bzip2::encoder::BZip2Encoder;
    use simple_logger;
    use traits::decoder::DecodeExt;
    use traits::encoder::EncodeExt;

    fn setup() {
        let _ = simple_logger::init();
    }

    #[test]
    fn test_unit() {
        setup();
        let ret = b"a\n"
            .iter()
            .cloned()
            .encode(&mut BZip2Encoder::new(9), Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        assert_eq!(
            ret,
            Ok(vec![
                0x42, 0x5A, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59,
                0x63, 0x3E, 0xD6, 0xE2, 0x00, 0x00, 0x00, 0xC1, 0x00, 0x00,
                0x10, 0x20, 0x00, 0x20, 0x00, 0x21, 0x00, 0x82, 0xB1, 0x77,
                0x24, 0x53, 0x85, 0x09, 0x06, 0x33, 0xED, 0x6E, 0x20,
            ])
        );

        let ret2 = ret.unwrap()
            .iter()
            .cloned()
            .decode(&mut BZip2Decoder::new())
            .collect::<Result<Vec<_>, _>>();
        if let Err(e) = ret2 {
            debug!("{}", e);
        }
        assert_eq!(ret2, Ok(b"a\n".to_vec()));
    }

    fn check_unzip(actual: &[u8], expected: &[u8]) {
        let ret2 = actual
            .iter()
            .cloned()
            .decode(&mut BZip2Decoder::new())
            .collect::<Result<Vec<_>, _>>();
        if let Err(e) = ret2 {
            debug!("{}", e);
        }
        assert!(ret2 == Ok(expected.to_vec()), "invalid unzip");
    }

    #[test]
    fn test_sample1() {
        setup();

        let mut encoder = BZip2Encoder::new(1);
        let ret = include_bytes!("../../data/sample1.ref")
            .iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        check_unzip(&ret, include_bytes!("../../data/sample1.ref"));

        check_unzip(
            include_bytes!("../../data/sample1.bz2"),
            include_bytes!("../../data/sample1.ref"),
        );
    }

    #[test]
    fn test_sample2() {
        setup();

        let mut encoder = BZip2Encoder::new(2);
        let ret = include_bytes!("../../data/sample2.ref")
            .iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        check_unzip(&ret, include_bytes!("../../data/sample2.ref"));

        check_unzip(
            include_bytes!("../../data/sample2.bz2"),
            include_bytes!("../../data/sample2.ref"),
        );
    }

    #[test]
    fn test_sample3() {
        setup();

        let mut encoder = BZip2Encoder::new(3);
        let ret = include_bytes!("../../data/sample3.ref")
            .iter()
            .cloned()
            .encode(&mut encoder, Action::Finish)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        check_unzip(&ret, include_bytes!("../../data/sample3.ref"));

        check_unzip(
            include_bytes!("../../data/sample3.bz2"),
            include_bytes!("../../data/sample3.ref"),
        );
    }

    #[test]
    fn test_sample4() {
        setup();
        check_unzip(
            include_bytes!("../../data/sample4.bz2"),
            include_bytes!("../../data/sample4.ref"),
        );
    }

    #[test]
    fn test_long() {
        setup();
        let data = b"a".iter()
                .cycle()
                .take(1000)
                .cloned()
                .collect::<Vec<u8>>();

        let compressed = data
            .iter()
            .cloned()
            .encode(&mut BZip2Encoder::new(9), Action::Finish)
            .collect::<Result<Vec<_>, _>>();

        let decompressed = compressed.unwrap()
            .iter()
            .cloned()
            .decode(&mut BZip2Decoder::new())
            .collect::<Result<Vec<_>, _>>();

        if let Err(e) = decompressed {
            debug!("{}", e);
        }
        assert_eq!(decompressed, Ok(data));
    }
}
