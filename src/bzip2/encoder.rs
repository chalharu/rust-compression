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
use alloc::collections::vec_deque::VecDeque;
use bitio::direction::Direction;
use bitio::direction::left::Left;
use bitio::small_bit_vec::SmallBitVec;
use bitio::writer::BitWriter;
use bitset::BitArray;
use bzip2::mtf::MtfPosition;
use bzip2::{HEADER_0, HEADER_h, BZ_G_SIZE, HEADER_B, HEADER_Z};
use core::cmp;
use core::fmt;
use core::hash::{BuildHasher, Hasher};
use core::u8;
use crc32::{BuiltinDigest, IEEE_NORMAL};
use error::CompressionError;
use huffman::cano_huff_table::make_tab_with_fn;
use huffman::encoder::HuffmanEncoder;
use log::Level;
#[cfg(feature = "std")]
use std::collections::vec_deque::VecDeque;
use suffix_array::sais::bwt;
use traits::encoder::Encoder;

pub struct BZip2Encoder {
    inner: EncoderInner,
    writer: BitWriter<Left>,
    queue: VecDeque<SmallBitVec<u32>>,

    finished: bool,
    bitbuf: u32,
    bitbuflen: usize,
    bit_finished: bool,
}

impl Default for BZip2Encoder {
    fn default() -> Self {
        Self::new(9)
    }
}

impl BZip2Encoder {
    pub fn new(level: usize) -> Self {
        if level < 1 || level > 9 {
            panic!("invalid level");
        }

        Self {
            inner: EncoderInner::new(level),
            writer: BitWriter::new(),
            queue: VecDeque::new(),
            finished: false,
            bitbuf: 0,
            bitbuflen: 0,
            bit_finished: false,
        }
    }

    fn next_bits<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: Action,
    ) -> Option<Result<SmallBitVec<u32>, CompressionError>> {
        while self.queue.is_empty() {
            match iter.next() {
                Some(s) => {
                    if let Err(e) = self.inner.next(s, &mut self.queue) {
                        return Some(Err(e));
                    }
                }
                None => {
                    if self.finished {
                        self.finished = false;
                        return None;
                    } else {
                        match action {
                            Action::Flush => {
                                if let Err(e) =
                                    self.inner.flush(&mut self.queue)
                                {
                                    return Some(Err(e));
                                }
                            }
                            Action::Finish => {
                                if let Err(e) =
                                    self.inner.finish(&mut self.queue)
                                {
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
impl Encoder for BZip2Encoder {
    type Error = CompressionError;
    fn next<I: Iterator<Item = u8>>(
        &mut self,
        iter: &mut I,
        action: &Action,
    ) -> Option<Result<u8, CompressionError>> {
        while self.bitbuflen == 0 {
            let s = match self.next_bits(iter, *action) {
                Some(Err(e)) => return Some(Err(e)),
                Some(Ok(ref s)) => self.writer.write_bits(s),
                None => {
                    if self.bit_finished {
                        self.bit_finished = false;
                        return None;
                    } else {
                        match *action {
                            Action::Finish | Action::Flush => {
                                self.bit_finished = true;
                                match self.writer.flush::<u32>() {
                                    Some((x, y)) if y != 0 => (x, y),
                                    _ => return None,
                                }
                            }
                            _ => {
                                return None;
                            }
                        }
                    }
                }
            };
            self.bitbuf = s.0;
            self.bitbuflen = s.1;
        }

        let ret = Left::convert(self.bitbuf, 32, 8) as u8;

        self.bitbuf = Left::forward(self.bitbuf, 8);
        self.bitbuflen -= 1;
        Some(Ok(ret))
    }
}

struct EncoderInner {
    block_buf: Vec<u8>,
    finished: bool,
    block_size_100k: usize,
    block_max_len: usize,
    combined_crc: u32,
    block_no: usize,
    block_crc: BuiltinDigest,
    rle_buffer: u8,
    rle_count: usize,
    in_use: BitArray,
    mtf_buffer: Vec<u16>,
    num_z: u64,
}

impl EncoderInner {
    fn prepare_new_block(&mut self) {
        self.block_no += 1;
        self.block_crc = IEEE_NORMAL.build_hasher();
        self.block_buf.clear();
        self.in_use.set_all(false);
    }

    pub fn new(level: usize) -> Self {
        let block_max_len = level * 100_000 - 19;
        Self {
            block_buf: Vec::with_capacity(level * 100_000),
            finished: false,
            block_size_100k: level,
            block_max_len,
            block_crc: IEEE_NORMAL.build_hasher(),
            rle_buffer: 0,
            rle_count: 0,
            block_no: 1,
            combined_crc: 0,
            in_use: BitArray::new(256),
            mtf_buffer: vec![0_u16; level * 100_000 + 1], // EOBの分増やす
            num_z: 0,
        }
    }

    fn write(
        &mut self,
        queue: &mut VecDeque<SmallBitVec<u32>>,
        val: SmallBitVec<u32>,
    ) {
        self.num_z += val.len() as u64;
        queue.push_back(val);
    }

    fn write_u8(&mut self, queue: &mut VecDeque<SmallBitVec<u32>>, val: u8) {
        self.write(queue, SmallBitVec::new(u32::from(val), 8));
    }

    fn write_u16(&mut self, queue: &mut VecDeque<SmallBitVec<u32>>, val: u16) {
        self.write(queue, SmallBitVec::new(u32::from(val), 16));
    }

    fn write_u32(&mut self, queue: &mut VecDeque<SmallBitVec<u32>>, val: u32) {
        self.write(queue, SmallBitVec::new(val, 32));
    }

    fn write_block(
        &mut self,
        is_final: bool,
        queue: &mut VecDeque<SmallBitVec<u32>>,
    ) -> Result<(), CompressionError> {
        if is_final {
            self.write_rle();
            self.rle_count = 0;
        }

        let nblock = self.block_buf.len();
        let block_crc = self.block_crc.finish() as u32;

        self.combined_crc =
            ((self.combined_crc << 1) | (self.combined_crc >> 31)) ^ block_crc;

        debug!(
            "    block {}: crc = 0x{:08X}, combined CRC = 0x{:08X}, size = {}",
            self.block_no, block_crc, self.combined_crc, nblock
        );

        if self.block_no == 1 {
            self.write_u8(queue, HEADER_B);
            self.write_u8(queue, HEADER_Z);
            self.write_u8(queue, HEADER_h);
            let bs100k = self.block_size_100k as u8;
            self.write_u8(queue, HEADER_0 + bs100k);
        }

        if nblock > 0 {
            self.write_u8(queue, 0x31);
            self.write_u8(queue, 0x41);
            self.write_u8(queue, 0x59);
            self.write_u8(queue, 0x26);
            self.write_u8(queue, 0x53);
            self.write_u8(queue, 0x59);

            /*-- Now the block's CRC, so it is in a known place. --*/
            self.write_u32(queue, block_crc);

            /*--
                Now a single bit indicating (non-)randomisation.
                As of version 0.9.5, we use a better sorting algorithm
                which makes randomisation unnecessary.  So always set
                the randomised bit to 'no'.  Of course, the decoder
                still needs to be able to handle randomised blocks
                so as to maintain backwards compatibility with
                older versions of bzip2.
            --*/
            self.write(queue, SmallBitVec::new(0, 1));

            try!(self.write_blockdata(queue));
            self.prepare_new_block();
        }
        /*-- If this is the last block, add the stream trailer. --*/
        if is_final {
            self.write_u8(queue, 0x17);
            self.write_u8(queue, 0x72);
            self.write_u8(queue, 0x45);
            self.write_u8(queue, 0x38);
            self.write_u8(queue, 0x50);
            self.write_u8(queue, 0x90);
            let comcrc = self.combined_crc;
            self.write_u32(queue, comcrc);
            debug!(
                "    final combined CRC = 0x{:08X}   ",
                self.combined_crc
            );
        }
        Ok(())
    }

    // const BZ_N_GROUPS: usize = 6;
    const BZ_N_ITERS: usize = 4;
    const BZ_MAX_SELECTORS: usize = (2 + (900_000 / BZ_G_SIZE));

    const BZ_LESSER_ICOST: u8 = 0;
    const BZ_GREATER_ICOST: u8 = 15;

    fn write_blockdata(
        &mut self,
        queue: &mut VecDeque<SmallBitVec<u32>>,
    ) -> Result<(), CompressionError> {
        let mut in_use_count = 0;
        let mut unseq2seq = [0_u8; 256];

        for (d, _) in unseq2seq
            .iter_mut()
            .zip(self.in_use.iter())
            .filter(|&(_, u)| u)
        {
            *d = in_use_count as u8;
            in_use_count += 1;
        }

        let eob = in_use_count + 1;

        let mut mtf_table = MtfPosition::new(in_use_count);

        let mut zero_count = 0;
        let mut mtf_freq = vec![0; in_use_count + 2];
        let mut mtf_count = 0;

        for (i, &s) in bwt(&self.block_buf, usize::from(u8::max_value()))
            .iter()
            .enumerate()
        {
            debug_assert!(mtf_count <= i, "generateMTFValues(1)");

            /* MTF */
            let c = {
                let j = if s == 0 {
                    self.write(queue, SmallBitVec::new(i as u32, 24));
                    self.block_buf.len()
                } else {
                    s
                } - 1;
                let val = usize::from(unseq2seq[self.block_buf[j] as usize]);
                debug_assert!(val < in_use_count, "generateMTFValues(2a)");
                mtf_table.pop(val) as u16 + 1
            };

            /* ZLE */
            if c == 1 {
                zero_count += 1;
            } else {
                self.zle_write(zero_count, &mut mtf_freq, &mut mtf_count);
                zero_count = 0;
                self.mtf_buffer[mtf_count] = c;
                mtf_count += 1;
                mtf_freq[c as usize] += 1;
            }
        }

        self.zle_write(zero_count, &mut mtf_freq, &mut mtf_count);
        self.mtf_buffer[mtf_count] = eob as u16;
        mtf_count += 1;
        mtf_freq[eob] += 1;

        debug!(
            "      {} in block, {} after MTF & 1-2 coding, {}+2 syms in use",
            self.block_buf.len(),
            mtf_count,
            in_use_count
        );

        let alpha_size = in_use_count + 2;

        /*--- Decide how many coding tables to use ---*/
        let group_num = match mtf_count {
            c if c < 200 => 2,
            c if c < 600 => 3,
            c if c < 1200 => 4,
            c if c < 2400 => 5,
            _ => 6,
        };

        /*--- Generate an initial set of coding tables ---*/
        let mut len = (0..group_num)
            .rev()
            .map(|x| x + 1)
            .scan((mtf_count as u32, 0_isize), |prev, n_part| {
                let t_freq = prev.0 / n_part as u32;
                let mut ge = prev.1 - 1;
                let mut a_freq = 0;

                while a_freq < t_freq && ge < alpha_size as isize - 1 {
                    ge += 1;
                    a_freq += mtf_freq[ge as usize];
                }

                if ge > prev.1 && n_part != group_num && n_part != 1
                    && (((group_num - n_part) & 1) == 1)
                {
                    a_freq -= mtf_freq[ge as usize];
                    ge -= 1;
                }

                debug!(
                    "      initial group {}, [{} .. {}], has {} syms ({:4.1}%)",
                    n_part,
                    prev.1,
                    ge,
                    a_freq,
                    (100.0 * f64::from(a_freq)) / (mtf_count as f64)
                );

                let gs = prev.1;

                *prev = (prev.0 - a_freq, ge + 1);

                Some(
                    (0..alpha_size as isize)
                        .map(|i| {
                            if i >= gs && i <= ge {
                                Self::BZ_LESSER_ICOST
                            } else {
                                Self::BZ_GREATER_ICOST
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        let mut n_selectors = 0;
        let mut selector = [0; Self::BZ_MAX_SELECTORS];
        /*---
            Iterate up to BZ_N_ITERS times to improve the tables.
        ---*/
        for iter in 0..Self::BZ_N_ITERS {
            let mut rfreq = vec![vec![0; alpha_size]; group_num];
            let mut fave = vec![0; group_num];

            n_selectors = 0;
            let mut totc: u32 = 0;
            let mut gs = 0;
            while gs < mtf_count {
                /*--- Set group start & end marks. --*/
                let ge = cmp::min(gs + BZ_G_SIZE, mtf_count);

                /*--
                    Calculate the cost of this group as coded
                    by each of the coding tables.
                --*/
                /*--
                   Find the coding table which is best for this group,
                   and record its identity in the selector table.
                --*/
                let (bt, bc) = len.iter()
                    .rev()
                    .map(|li| {
                        self.mtf_buffer[gs..ge]
                            .iter()
                            .map(|&x| {
                                u16::from(*unsafe {
                                    li.get_unchecked(x as usize)
                                })
                            })
                            .sum::<u16>()
                    })
                    .enumerate()
                    .min_by(|x, y| x.1.cmp(&y.1))
                    .unwrap();

                totc += u32::from(bc);
                fave[bt] += 1;
                selector[n_selectors] = bt;
                n_selectors += 1;

                /*--
                   Increment the symbol frequencies for the selected table.
                 --*/
                for &i in &self.mtf_buffer[gs..ge] {
                    rfreq[bt][i as usize] += 1;
                }
                gs = ge;
            }

            if log_enabled!(Level::Debug) {
                let mut debug_str = String::new();

                let _ = fmt::write(
                    &mut debug_str,
                    format_args!(
                        "      pass {}: size is {}, grp uses are",
                        iter + 1,
                        totc / 8
                    ),
                );
                for f in fave {
                    let _ = fmt::write(&mut debug_str, format_args!(" {}", f));
                }
                debug!("{}", debug_str);
            }
            /*--
              Recompute the tables based on the accumulated frequencies.
            --*/
            /* maxLen was changed from 20 to 17 in bzip2-1.0.3.  See
                   comment in huffman.c for details. */
            len = rfreq
                .iter()
                .rev()
                .map(|r| Self::create_huffman(r, 17))
                .collect::<Vec<_>>();
        }

        /*--- Compute MTF values for the selectors. ---*/
        let mut selector_mtf_tab = MtfPosition::new(group_num);
        let selector_mtf = selector
            .iter()
            .take(n_selectors)
            .map(|&x| selector_mtf_tab.pop(x))
            .collect::<Vec<_>>();

        /*--- Assign actual codes for the tables. --*/
        let code = len.iter()
            .rev()
            .map(|j| HuffmanEncoder::<Left, u32>::new(j))
            .collect::<Vec<_>>();

        let mut debug_str = String::new();
        /*--- Transmit the mapping table. ---*/
        {
            let in_use16 = self.in_use
                .u16_iter()
                .map(|x| x != 0)
                .collect::<BitArray>();

            let n_bits = self.num_z;
            self.write_u16(
                queue,
                in_use16
                    .iter()
                    .fold(0, |x, y| (x << 1) + if y { 1 } else { 0 }),
            );

            for i in in_use16
                .iter()
                .enumerate()
                .filter_map(|(i, x)| if x { Some(i << 4) } else { None })
            {
                for j in (0..16).map(|x| x + i) {
                    let bv = SmallBitVec::new(
                        if self.in_use.get(j) { 1 } else { 0 },
                        1,
                    );
                    self.write(queue, bv)
                }
            }

            if log_enabled!(Level::Debug) {
                let _ = fmt::write(
                    &mut debug_str,
                    format_args!(
                        "      bits: mapping {}, ",
                        self.num_z - n_bits
                    ),
                );
            }
        }

        /*--- Now the selectors. ---*/
        let n_bits = self.num_z;
        self.write(queue, SmallBitVec::new(group_num as u32, 3));
        self.write(queue, SmallBitVec::new(n_selectors as u32, 15));

        for s in selector_mtf {
            self.write(
                queue,
                SmallBitVec::new((1 << (s + 1)) - 2, s + 1),
            );
        }

        if log_enabled!(Level::Debug) {
            let _ = fmt::write(
                &mut debug_str,
                format_args!("selectors {}, ", self.num_z - n_bits),
            );
        }

        /*--- Now the coding tables. ---*/
        let n_bits = self.num_z;
        for l in len.iter().rev() {
            let mut curr = l[0];
            self.write(queue, SmallBitVec::new(u32::from(curr), 5));
            for &li in l {
                while curr < li {
                    /* 10 */
                    self.write(queue, SmallBitVec::new(2, 2));
                    curr += 1;
                }
                while curr > li {
                    /* 11 */
                    self.write(queue, SmallBitVec::new(3, 2));
                    curr -= 1;
                }
                self.write(queue, SmallBitVec::new(0, 1));
            }
        }
        if log_enabled!(Level::Debug) {
            let _ = fmt::write(
                &mut debug_str,
                format_args!("code lengths {}, ", self.num_z - n_bits),
            );
        }

        /*--- And finally, the block data proper ---*/
        let n_bits = self.num_z;
        {
            let mut sel_ctr = 0;
            let mut gs = 0;
            while gs < mtf_count {
                let ge = cmp::min(gs + BZ_G_SIZE, mtf_count);
                let encoder = &code[selector[sel_ctr]];
                for i in gs..ge {
                    let b = self.mtf_buffer[i];
                    self.write(
                        queue,
                        try!(
                            encoder
                                .enc(&b)
                                .map_err(|_| CompressionError::Unexpected)
                        ),
                    );
                }
                gs = ge;
                sel_ctr += 1;
            }
        }
        if log_enabled!(Level::Debug) {
            let _ = fmt::write(
                &mut debug_str,
                format_args!("codes {}, ", self.num_z - n_bits),
            );
            debug!("{}", debug_str);
        }

        Ok(())
    }

    fn create_huffman(freq: &[usize], lim: usize) -> Vec<u8> {
        let weight = freq.iter()
            .map(|&x| cmp::max(1, x) << 8)
            .collect::<Vec<_>>();

        make_tab_with_fn(&weight, lim, |x, y| {
            ((x & 0xFFFF_FF00) + (y & 0xFFFF_FF00))
                | (1 + cmp::max(x & 0xFF, y & 0xFF))
        })
    }

    fn zle_write(
        &mut self,
        mut zero_count: usize,
        mtf_freq: &mut [u32],
        mtf_count: &mut usize,
    ) {
        if zero_count != 0 {
            zero_count += 1;
            while zero_count > 1 {
                let run = zero_count as u16 & 1;
                self.mtf_buffer[*mtf_count] = run;
                *mtf_count += 1;
                mtf_freq[run as usize] += 1;
                zero_count >>= 1;
            }
        }
    }

    fn next(
        &mut self,
        buf: u8,
        queue: &mut VecDeque<SmallBitVec<u32>>,
    ) -> Result<(), CompressionError> {
        if self.rle_count == 0 {
            self.rle_buffer = buf;
            self.rle_count = 1;
            return Ok(());
        }

        if self.rle_buffer == buf && self.rle_count < 255 {
            self.rle_count += 1;
            return Ok(());
        }

        self.write_rle();

        self.rle_count = 1;
        self.rle_buffer = buf;

        if self.block_buf.len() >= self.block_max_len {
            self.write_block(false, queue)
        } else {
            Ok(())
        }
    }

    fn write_rle(&mut self) {
        // RLE output
        (0..self.rle_count)
            .for_each(|_| self.block_crc.write_u8(self.rle_buffer));

        let ret_count = cmp::min(self.rle_count, 4);
        for _ in 0..ret_count {
            let v = self.rle_buffer;
            self.in_use.set(usize::from(v), true);
            self.block_buf.push(v);
        }

        if ret_count == 4 {
            let v = (self.rle_count - 4) as u8;
            self.in_use.set(usize::from(v), true);
            self.block_buf.push(v);
        }
    }

    fn flush(
        &mut self,
        queue: &mut VecDeque<SmallBitVec<u32>>,
    ) -> Result<(), CompressionError> {
        if !self.finished {
            self.write_block(false, queue)
        } else {
            Ok(())
        }
    }

    fn finish(
        &mut self,
        queue: &mut VecDeque<SmallBitVec<u32>>,
    ) -> Result<(), CompressionError> {
        if !self.finished {
            self.finished = true;
            self.write_block(true, queue)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {}
