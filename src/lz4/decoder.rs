//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::cmp;
use core::hash::Hasher;
use core::ptr;
use error::CompressionError;
use traits::decoder::Decoder;
use xxhash::XXH32;

const MAGIC: u32 = 0x184d2204;

struct BlockDecoder<'a> {
    input: &'a [u8],
    output: &'a mut Vec<u8>,
    cur: usize,

    start: usize,
    end: usize,
}

impl<'a> BlockDecoder<'a> {
    /// Decodes this block of data from 'input' to 'output', returning the
    /// number of valid bytes in the output.
    fn decode(&mut self) -> usize {
        while self.cur < self.input.len() {
            let code = self.bump();
            debug!("block with code: {:x}", code);
            // Extract a chunk of data from the input to the output.
            {
                let len = self.length(code >> 4);
                debug!("consume len {}", len);
                if len > 0 {
                    let end = self.end;
                    self.grow_output(end + len);
                    unsafe {
                        ptr::copy_nonoverlapping(
                            &self.input[self.cur],
                            &mut self.output[end],
                            len,
                        )
                    };
                    self.end += len;
                    self.cur += len;
                }
            }
            if self.cur == self.input.len() {
                break;
            }

            // Read off the next i16 offset
            {
                let back =
                    (self.bump() as usize) | ((self.bump() as usize) << 8);
                debug!("found back {}", back);
                self.start = self.end - back;
            }

            // Slosh around some bytes now
            {
                let mut len = self.length(code & 0xf);
                let literal = self.end - self.start;
                if literal < 4 {
                    static DECR: [usize; 4] = [0, 3, 2, 3];
                    self.cp(4, DECR[literal]);
                } else {
                    len += 4;
                }
                self.cp(len, 0);
            }
        }
        self.end
    }

    fn length(&mut self, code: u8) -> usize {
        let mut ret = code as usize;
        if code == 0xf {
            loop {
                let tmp = self.bump();
                ret += tmp as usize;
                if tmp != 0xff {
                    break;
                }
            }
        }
        ret
    }

    fn bump(&mut self) -> u8 {
        let ret = self.input[self.cur];
        self.cur += 1;
        ret
    }

    #[inline]
    fn cp(&mut self, len: usize, decr: usize) {
        let end = self.end;
        self.grow_output(end + len);
        for i in 0..len {
            self.output[end + i] = (*self.output)[self.start + i];
        }

        self.end += len;
        self.start += len - decr;
    }

    // Extends the output vector to a target number of bytes (in total), but
    // does not actually initialize the new data. The length of the vector is
    // updated, but the bytes will all have undefined values. It is assumed that
    // the next operation is to pave over these bytes (so the initialization is
    // unnecessary).
    #[inline]
    fn grow_output(&mut self, target: usize) {
        if self.output.capacity() < target {
            debug!(
                "growing {} to {}",
                self.output.capacity(),
                target
            );
            //let additional = target - self.output.capacity();
            //self.output.reserve(additional);
            while self.output.len() < target {
                self.output.push(0);
            }
        } else {
            unsafe {
                self.output.set_len(target);
            }
        }
    }
}

pub struct Lz4Decoder {
    temp: Vec<u8>,
    output: Vec<u8>,

    start: usize,
    end: usize,
    eof: bool,

    header: bool,
    blk_checksum: bool,
    stream_checksum: bool,
    max_block_size: usize,
}

impl Lz4Decoder {
    /// Creates a new decoder which will read data from the given stream. The
    /// inner stream can be re-acquired by moving out of the `r` field of this
    /// structure.
    pub fn new() -> Lz4Decoder {
        Lz4Decoder {
            temp: Vec::new(),
            output: Vec::new(),
            header: false,
            blk_checksum: false,
            stream_checksum: false,
            start: 0,
            end: 0,
            eof: false,
            max_block_size: 0,
        }
    }

    fn read_u32<R: Iterator<Item = u8>>(
        iter: &mut R,
    ) -> Result<u32, CompressionError> {
        let mut r = u32::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?);
        r |= u32::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?) << 8;
        r |= u32::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 16;
        Ok(r | u32::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 24)
    }

    fn read_u64<R: Iterator<Item = u8>>(
        iter: &mut R,
    ) -> Result<u64, CompressionError> {
        let mut r = u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?);
        r |= u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?) << 8;
        r |= u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 16;
        r |= u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 24;
        r |= u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 32;
        r |= u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 40;
        r |= u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 48;
        Ok(r | u64::from(iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?)
            << 56)
    }

    fn read_header<R: Iterator<Item = u8>>(
        &mut self,
        iter: &mut R,
    ) -> Result<(), CompressionError> {
        // Make sure the magic number is what's expected.
        if Self::read_u32(iter)? != MAGIC {
            return Err(CompressionError::DataError);
        }

        let mut digest = XXH32::default();
        let flg = iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?;
        digest.write_u8(flg);
        let bd = iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?;
        digest.write_u8(bd);

        // bits 7/6, the version number. Right now this must be 1
        if (flg >> 6) != 0b01 {
            return Err(CompressionError::DataError);
        }
        // bit 5 is the "block independence", don't care about this yet
        // bit 4 is whether blocks have checksums or not
        self.blk_checksum = (flg & 0x10) != 0;
        // bit 3 is whether there is a following stream size
        let stream_size = (flg & 0x08) != 0;
        // bit 2 is whether there is a stream checksum
        self.stream_checksum = (flg & 0x04) != 0;
        // bit 1 is reserved
        // bit 0 is whether there is a preset dictionary
        let preset_dictionary = (flg & 0x01) != 0;

        static MAX_SIZES: [usize; 8] = [
            0,
            0,
            0,
            0,         // all N/A
            64 << 10,  // 64KB
            256 << 10, // 256 KB
            1 << 20,   // 1MB
            4 << 20,
        ]; // 4MB

        // bit 7 is reserved
        // bits 6-4 are the maximum block size
        let max_block_size = MAX_SIZES[(bd >> 4) as usize & 0x7];
        // bits 3-0 are reserved

        // read off other portions of the stream
        let size = if stream_size {
            let size = Self::read_u64(iter)?;
            digest.write_u64(size);
            Some(size)
        } else {
            None
        };
        assert!(
            !preset_dictionary,
            "preset dictionaries not supported yet"
        );

        debug!("blk: {}", self.blk_checksum);
        debug!("stream: {}", self.stream_checksum);
        debug!("max size: {}", max_block_size);
        debug!("stream size: {:?}", size);

        self.max_block_size = max_block_size;

        let cksum = iter.next()
            .ok_or_else(|| CompressionError::UnexpectedEof)?;

        if (digest.finish() >> 8) as u8 != cksum {
            debug!("invalid header checksum : {}", cksum);
            return Err(CompressionError::DataError);
        }

        return Ok(());
    }

    fn decode_block<R: Iterator<Item = u8>>(
        &mut self,
        iter: &mut R,
    ) -> Result<bool, CompressionError> {
        match Self::read_u32(iter)? {
            // final block, we're done here
            0 => return Ok(false),

            // raw block to read
            n if n & 0x80000000 != 0 => {
                let amt = (n & 0x7fffffff) as usize;
                self.output.truncate(0);
                self.output.reserve(amt);
                self.output.extend(iter.take(amt));
                self.start = 0;
                self.end = amt;
            }

            // actual block to decompress
            n => {
                let n = n as usize;
                self.temp.truncate(0);
                self.temp.reserve(n);
                self.temp.extend(iter.take(n));

                let target = cmp::min(self.max_block_size, 4 * n / 3);
                self.output.truncate(0);
                self.output.reserve(target);
                let mut decoder = BlockDecoder {
                    input: &self.temp[..n],
                    output: &mut self.output,
                    cur: 0,
                    start: 0,
                    end: 0,
                };
                self.start = 0;
                self.end = decoder.decode();
            }
        }

        if self.blk_checksum {
            let cksum = Self::read_u32(iter)?;
            let mut digest = XXH32::default();
            digest.write(&self.output[..self.end]);
            if digest.finish() != u64::from(cksum) {
                debug!("invalid block checksum : {}", cksum);
                return Err(CompressionError::DataError);
            }
        }
        return Ok(true);
    }
}

impl<I> Decoder<I> for Lz4Decoder
where
    I: Iterator<Item = u8>,
{
    type Error = CompressionError;
    type Output = u8;
    type Reader = I;

    fn next(
        &mut self,
        iter: &mut Self::Reader,
    ) -> Result<Option<u8>, Self::Error> {
        if self.eof {
            return Ok(None);
        }
        if !self.header {
            self.read_header(iter)?;
            self.header = true;
        }

        if self.start == self.end {
            let keep_going = self.decode_block(iter)?;
            if !keep_going {
                self.eof = true;
                return Ok(None);
            }
        }

        let ret = self.output[self.start];
        self.start += 1;
        Ok(Some(ret))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simple_logger;
    use traits::decoder::DecodeExt;

    fn setup() {
        let _ = simple_logger::init();
    }

    fn check_unzip(actual: &[u8], expected: &[u8]) {
        let ret2 = actual
            .iter()
            .cloned()
            .decode(&mut Lz4Decoder::new())
            .collect::<Result<Vec<_>, _>>();
        if let Err(e) = ret2 {
            debug!("{}", e);
        }
        assert!(ret2 == Ok(expected.to_vec()), "invalid unzip");
    }

    #[test]
    fn decode() {
        setup();

        let reference = include_bytes!("../../data/test.txt");
        check_unzip(
            include_bytes!("../../data/test.lz4.1"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.2"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.3"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.4"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.5"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.6"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.7"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.8"),
            reference,
        );
        check_unzip(
            include_bytes!("../../data/test.lz4.9"),
            reference,
        );
    }
}
