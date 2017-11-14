//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use BitReader;
use BitVector;
use LzssCode;
use huffman_decoder::{HuffmanDecoder, LeftHuffmanDecoder};
use read::Read;
use std::cell::RefCell;
use std::io::Error as ioError;
use std::io::ErrorKind as ioErrorKind;
use std::io::Result as ioResult;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct MultiBitReader<R: BitReader>(Rc<RefCell<R>>);

impl<R: BitReader> MultiBitReader<R> {
    pub fn new(inner: R) -> Self {
        MultiBitReader(Rc::new(RefCell::new(inner)))
    }

    pub fn into_inner(self) -> R {
        Rc::try_unwrap(self.0).ok().unwrap().into_inner()
    }
}

impl<R: BitReader> BitReader for MultiBitReader<R> {
    type R = R::R;
    fn read(&mut self, len: usize) -> ioResult<BitVector> {
        self.0.borrow_mut().read(len)
    }

    fn peek(&mut self, len: usize) -> ioResult<BitVector> {
        self.0.borrow_mut().peek(len)
    }

    fn skip(&mut self, len: usize) -> ioResult<usize> {
        self.0.borrow_mut().skip(len)
    }

    fn skip_to_byte(&mut self) -> ioResult<usize> {
        self.0.borrow_mut().skip_to_byte()
    }

    fn into_inner(&mut self) -> ioResult<Self::R> {
        self.0.borrow_mut().into_inner()
    }
}

enum LzhufHuffmanDecoder<R: BitReader> {
    HuffmanDecoder(LeftHuffmanDecoder<MultiBitReader<R>, u16>),
    Default(MultiBitReader<R>, u16),
}

impl<R: BitReader> LzhufHuffmanDecoder<R> {
    pub fn dec(&mut self) -> ioResult<Option<u16>> {
        match self {
            &mut LzhufHuffmanDecoder::HuffmanDecoder(ref mut lhd) => lhd.dec(),
            &mut LzhufHuffmanDecoder::Default(ref mut br, s) => {
                let r = try!(br.read(1));
                if r.len() == 0 { Ok(None) } else { Ok(Some(s)) }
            }
        }
    }
}

pub struct LzhufDecoder<R: BitReader> {
    inner: MultiBitReader<R>,
    offset_len: usize,
    min_match: usize,
    block_len: usize,
    symbol_decoder: Option<LzhufHuffmanDecoder<R>>,
    offset_decoder: Option<LzhufHuffmanDecoder<R>>,
}

impl<R: BitReader + Clone> LzhufDecoder<R> {
    const SEARCH_TAB_LEN: usize = 12;

    pub fn new(inner: R, offset_len: usize, min_match: usize) -> Self {
        Self {
            inner: MultiBitReader::new(inner),
            offset_len,
            min_match,
            block_len: 0,
            symbol_decoder: None,
            offset_decoder: None,
        }
    }

    fn dec_len(&mut self) -> ioResult<u8> {
        let mut c = try!(self.inner.read(3)).data();
        if c == 7 {
            while try!(self.inner.read(1)).data() == 1 {
                c += 1;
            }
        }
        Ok(c as u8)
    }

    fn dec_len_tree(
        &mut self,
        tbit_len: usize,
    ) -> ioResult<LzhufHuffmanDecoder<R>> {
        let len = try!(self.inner.read(tbit_len)).data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                self.inner.clone(),
                try!(self.inner.read(tbit_len)).data() as u16,
            ))
        } else {
            let mut ll = Vec::new();
            while ll.len() < len {
                if ll.len() == 3 {
                    for _ in 0..try!(self.inner.read(2)).data() {
                        ll.push(0);
                    }
                    if ll.len() > len {
                        return Err(ioError::new(
                            ioErrorKind::Other,
                            "LZH invalid zero-run-length",
                        ));
                    }
                    if ll.len() == len {
                        break;
                    }
                }
                ll.push(try!(self.dec_len()));
            }
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(
                LeftHuffmanDecoder::new(self.inner.clone(), &ll, 5),
            ))
        }
    }

    fn dec_symb_tree(
        &mut self,
        len_decoder: &mut LzhufHuffmanDecoder<R>,
    ) -> ioResult<LzhufHuffmanDecoder<R>> {
        let len = try!(self.inner.read(9)).data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                self.inner.clone(),
                try!(self.inner.read(9)).data() as u16,
            ))
        } else {
            let mut ll = Vec::new();
            while ll.len() < len {
                match try!(len_decoder.dec()) {
                    None => {
                        return Err(ioError::new(
                            ioErrorKind::UnexpectedEof,
                            "End of file",
                        ))
                    }
                    Some(0) => ll.push(0),
                    Some(1) => {
                        for _ in 0..(3 + try!(self.inner.read(4)).data()) {
                            ll.push(0);
                        }
                    }
                    Some(2) => {
                        for _ in 0..(20 + try!(self.inner.read(9)).data()) {
                            ll.push(0);
                        }
                    }
                    Some(n) => ll.push((n - 2) as u8),
                }
            }
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(
                LeftHuffmanDecoder::new(
                    self.inner.clone(),
                    &ll,
                    Self::SEARCH_TAB_LEN,
                ),
            ))
        }
    }

    fn dec_offs_tree(
        &mut self,
        pbit_len: usize,
    ) -> ioResult<LzhufHuffmanDecoder<R>> {
        let len = try!(self.inner.read(pbit_len)).data() as usize;
        if len == 0 {
            Ok(LzhufHuffmanDecoder::Default(
                self.inner.clone(),
                try!(self.inner.read(pbit_len)).data() as u16,
            ))
        } else {
            Ok(LzhufHuffmanDecoder::HuffmanDecoder(
                LeftHuffmanDecoder::new(
                    self.inner.clone(),
                    &try!(
                        (0..len)
                            .map(|_| self.dec_len())
                            .collect::<ioResult<Vec<u8>>>()
                    ),
                    Self::SEARCH_TAB_LEN,
                ),
            ))
        }
    }

    fn init_block(&mut self) -> ioResult<bool> {
        match try!(self.inner.read(16).map(|x| (x.data(), x.len()))) {
            (0, 16) => Ok(false),
            (s, 16) => {
                self.block_len = s as usize;
                let mut lt = try!(self.dec_len_tree(5));
                self.symbol_decoder = Some(try!(self.dec_symb_tree(&mut lt)));
                let offlen = self.offset_len;
                self.offset_decoder = Some(try!(self.dec_offs_tree(offlen)));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn pop(&mut self) -> ioResult<Option<LzssCode>> {
        if self.block_len == 0 && !try!(self.init_block()) {
            return Ok(None);
        }
        self.block_len -= 1;
        let sym =
            try!(try!(self.symbol_decoder.as_mut().unwrap().dec()).ok_or(
                ioError::new(ioErrorKind::UnexpectedEof, "end of file"),
            )) as usize;
        if sym <= 255 {
            Ok(Some(LzssCode::Symbol(sym as u8)))
        } else {
            let len = sym - 256 + self.min_match;
            let mut pos = try!(
                try!(self.offset_decoder.as_mut().unwrap().dec())
                    .ok_or(
                        ioError::new(ioErrorKind::UnexpectedEof, "end of file"),
                    )
            ) as usize;
            if pos > 1 {
                pos = (1 << (pos - 1)) |
                    try!(self.inner.read(pos - 1)).data() as usize;
            }
            Ok(Some(LzssCode::Reference { len, pos }))
        }
    }
}

impl<R: BitReader + Clone> Read<LzssCode> for LzhufDecoder<R> {
    fn read(&mut self, buf: &mut [LzssCode]) -> ioResult<usize> {
        for i in 0..buf.len() {
            match try!(self.pop()) {
                Some(v) => {
                    buf[i] = v;
                }
                None => {
                    return Ok(i);
                }
            }
        }
        Ok(buf.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_reader::LeftBitReader;
    use bit_writer::LeftBitWriter;
    use lzhuf_encoder::LzhufEncoder;
    use lzss_encoder::LzssEncoder;
    use std::cmp::Ordering;
    use std::io::Cursor;
    use std::io::Write;

    fn comparison(lhs: LzssCode, rhs: LzssCode) -> Ordering {
        match (lhs, rhs) {
            (LzssCode::Reference {
                 len: llen,
                 pos: lpos,
             },
             LzssCode::Reference {
                 len: rlen,
                 pos: rpos,
             }) => ((llen << 3) - lpos).cmp(&((rlen << 3) - rpos)).reverse(),
            (LzssCode::Symbol(_), LzssCode::Symbol(_)) => Ordering::Equal,
            (_, LzssCode::Symbol(_)) => Ordering::Greater,
            (LzssCode::Symbol(_), _) => Ordering::Less,
        }
    }

    #[test]
    fn test_empty() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut encoder = LzssEncoder::new(
            LzhufEncoder::new(writer, 65_536, 5, 256),
            comparison,
            65_536,
            256,
            3,
            3,
        );
        let _ = encoder.write_all(b"");
        let _ = encoder.flush();

        let mut cursor =
            encoder.into_inner().into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = LeftBitReader::new(cursor);
        let mut decoder = LzhufDecoder::new(reader, 5, 3);
        let mut buf = Vec::new();
        let _ = decoder.read_to_end(&mut buf);
        assert_eq!(buf, vec![]);
    }

    #[test]
    fn test_unit() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut encoder = LzssEncoder::new(
            LzhufEncoder::new(writer, 65_536, 5, 256),
            comparison,
            65_536,
            256,
            3,
            3,
        );
        let _ = encoder.write_all(b"a");
        let _ = encoder.flush();

        let mut cursor =
            encoder.into_inner().into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = LeftBitReader::new(cursor);
        let mut decoder = LzhufDecoder::new(reader, 5, 3);
        let mut buf = Vec::new();
        let _ = decoder.read_to_end(&mut buf);
        assert_eq!(buf, vec![LzssCode::Symbol(b"a"[0])]);
    }

    #[test]
    fn test_arr() {
        let writer = LeftBitWriter::new(Cursor::new(Vec::<u8>::new()));
        let mut encoder = LzssEncoder::new(
            LzhufEncoder::new(writer, 65_536, 5, 256),
            comparison,
            65_536,
            256,
            3,
            3,
        );
        let _ = encoder.write_all(b"aaaaaaaaaaa");
        let _ = encoder.flush();

        let mut cursor =
            encoder.into_inner().into_inner().into_inner().unwrap();
        cursor.set_position(0);

        let reader = LeftBitReader::new(cursor);
        let mut decoder = LzhufDecoder::new(reader, 5, 3);
        let mut buf = Vec::new();
        let _ = decoder.read_to_end(&mut buf);
        assert_eq!(
            buf,
            vec![
                LzssCode::Symbol(b"a"[0]),
                LzssCode::Reference { len: 10, pos: 0 },
            ]
        );
    }
}
