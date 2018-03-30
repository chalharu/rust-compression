//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use action::Action;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::vec_deque::VecDeque;
use core::hash::Hasher;
use core::u8;
use error::CompressionError;
use log::Level;
use lz4::{HASH64K_LOG, LZ4_64KLIMIT, LZ4_MAGIC, LZ4_MAX_INPUT_SIZE, HASH_LOG,
          LASTLITERALS};
#[cfg(feature = "std")]
use std::collections::vec_deque::VecDeque;
use traits::encoder::Encoder;

fn compress_bound(input_size: u32) -> u32 {
    if input_size > LZ4_MAX_INPUT_SIZE {
        0
    } else {
        input_size + (input_size / 255) + 16
    }
}

pub struct Lz4Encoder {
    inner: BlockEncoder,
    queue: VecDeque<u8>,
    finished: bool,
    limit: usize,
    buf: Vec<u8>,
}

impl Default for Lz4Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Lz4Encoder {
    pub fn new() -> Self {
        let mut queue = VecDeque::new();
        write_u32(&mut queue, LZ4_MAGIC);
        // version 01, turn on block independence, but turn off
        // everything else (we have no checksums right now).
        queue.push_back(0b01_100000);
        // Maximum block size is 256KB
        queue.push_back(0b0_101_0000);
        // XXX: this checksum is just plain wrong.
        queue.push_back(0xfb);

        Self {
            inner: BlockEncoder::new(),
            queue: queue,
            finished: false,
            limit: 256 * 1024,
            buf: Vec::with_capacity(1024),
        }
    }

    // Dummy encoder
    fn encode_block(&mut self) -> Result<(), CompressionError> {
        if !self.compress()? {
            write_u32(
                &mut self.queue,
                self.buf.len() as u32 | 0x80000000,
            );
            self.queue.extend(self.buf.iter())
        }
        self.buf.clear();
        Ok(())
    }

    fn compress(&mut self) -> Result<bool, CompressionError> {
        Ok(false)
    }

    /// This function is used to flag that this session of compression is done
    /// with. The stream is finished up (final bytes are written), and then the
    /// wrapped writer is returned.
    fn finish(&mut self) -> Result<(), CompressionError> {
        let result = self.flush();

        for _ in 0..2 {
            write_u32(&mut self.queue, 0);
        }
        result
    }

    fn flush(&mut self) -> Result<(), CompressionError> {
        if self.buf.len() > 0 {
            self.encode_block()
        } else {
            Ok(())
        }
    }
}

fn write_u32(queue: &mut VecDeque<u8>, value: u32) {
    queue.push_back(value as u8);
    queue.push_back((value >> 8) as u8);
    queue.push_back((value >> 16) as u8);
    queue.push_back((value >> 24) as u8);
}

impl Encoder for Lz4Encoder {
    type Error = CompressionError;
    fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: &Action,
    ) -> Option<Result<u8, CompressionError>> {
        while self.queue.is_empty() {
            match iter.next() {
                Some(s) => {
                    self.buf.push(s);
                    if self.buf.len() == self.limit {
                        if let Err(e) = self.encode_block() {
                            return Some(Err(e));
                        }
                    }
                }
                None => {
                    if self.finished {
                        return None;
                    } else {
                        match *action {
                            Action::Flush | Action::Finish => {
                                if let Err(e) = self.finish() {
                                    return Some(Err(e));
                                }
                            }
                            _ => {}
                        }
                        self.finished = true;
                    }
                }
            }
        }
        self.queue.pop_front().map(Ok)
    }
}

struct BlockEncoder {
    finished: bool,
    hashtab: Vec<u32>,
}

impl BlockEncoder {
    pub fn new() -> Self {
        Self {
            finished: false,
            hashtab: vec![0; HASH_LOG as usize],
        }
    }

    fn write_block(
        &mut self,
        is_final: bool,
        queue: &mut VecDeque<u8>,
    ) -> Result<(), CompressionError> {
        Ok(())
    }

    fn next(
        &mut self,
        buf: u8,
        queue: &mut VecDeque<u8>,
    ) -> Result<(), CompressionError> {
        Ok(())
    }

    fn flush(
        &mut self,
        queue: &mut VecDeque<u8>,
    ) -> Result<(), CompressionError> {
        if !self.finished {
            self.write_block(false, queue)
        } else {
            Ok(())
        }
    }

    fn finish(
        &mut self,
        queue: &mut VecDeque<u8>,
    ) -> Result<(), CompressionError> {
        if !self.finished {
            self.finished = true;
            self.write_block(true, queue)
        } else {
            Ok(())
        }
    }
}
