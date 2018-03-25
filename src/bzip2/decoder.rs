//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use bitio::direction::left::Left;
use bitio::reader::BitRead;
use bitset::BitArray;
use bzip2::{HEADER_0, HEADER_h, BZ_G_SIZE, HEADER_B, HEADER_Z};
use bzip2::error::BZip2Error;
use bzip2::mtf::MtfPositionDecoder;
use core::hash::{BuildHasher, Hasher};
use crc32::{BuiltinDigest, IEEE_NORMAL};
use huffman::decoder::HuffmanDecoder;
use traits::decoder::Decoder;

const BZ2_R_NUMS: [usize; 512] = [
    619, 720, 127, 481, 931, 816, 813, 233, 566, 247, 985, 724, 205, 454, 863,
    491, 741, 242, 949, 214, 733, 859, 335, 708, 621, 574, 73, 654, 730, 472,
    419, 436, 278, 496, 867, 210, 399, 680, 480, 51, 878, 465, 811, 169, 869,
    675, 611, 697, 867, 561, 862, 687, 507, 283, 482, 129, 807, 591, 733, 623,
    150, 238, 59, 379, 684, 877, 625, 169, 643, 105, 170, 607, 520, 932, 727,
    476, 693, 425, 174, 647, 73, 122, 335, 530, 442, 853, 695, 249, 445, 515,
    909, 545, 703, 919, 874, 474, 882, 500, 594, 612, 641, 801, 220, 162, 819,
    984, 589, 513, 495, 799, 161, 604, 958, 533, 221, 400, 386, 867, 600, 782,
    382, 596, 414, 171, 516, 375, 682, 485, 911, 276, 98, 553, 163, 354, 666,
    933, 424, 341, 533, 870, 227, 730, 475, 186, 263, 647, 537, 686, 600, 224,
    469, 68, 770, 919, 190, 373, 294, 822, 808, 206, 184, 943, 795, 384, 383,
    461, 404, 758, 839, 887, 715, 67, 618, 276, 204, 918, 873, 777, 604, 560,
    951, 160, 578, 722, 79, 804, 96, 409, 713, 940, 652, 934, 970, 447, 318,
    353, 859, 672, 112, 785, 645, 863, 803, 350, 139, 93, 354, 99, 820, 908,
    609, 772, 154, 274, 580, 184, 79, 626, 630, 742, 653, 282, 762, 623, 680,
    81, 927, 626, 789, 125, 411, 521, 938, 300, 821, 78, 343, 175, 128, 250,
    170, 774, 972, 275, 999, 639, 495, 78, 352, 126, 857, 956, 358, 619, 580,
    124, 737, 594, 701, 612, 669, 112, 134, 694, 363, 992, 809, 743, 168, 974,
    944, 375, 748, 52, 600, 747, 642, 182, 862, 81, 344, 805, 988, 739, 511,
    655, 814, 334, 249, 515, 897, 955, 664, 981, 649, 113, 974, 459, 893, 228,
    433, 837, 553, 268, 926, 240, 102, 654, 459, 51, 686, 754, 806, 760, 493,
    403, 415, 394, 687, 700, 946, 670, 656, 610, 738, 392, 760, 799, 887, 653,
    978, 321, 576, 617, 626, 502, 894, 679, 243, 440, 680, 879, 194, 572, 640,
    724, 926, 56, 204, 700, 707, 151, 457, 449, 797, 195, 791, 558, 945, 679,
    297, 59, 87, 824, 713, 663, 412, 693, 342, 606, 134, 108, 571, 364, 631,
    212, 174, 643, 304, 329, 343, 97, 430, 751, 497, 314, 983, 374, 822, 928,
    140, 206, 73, 263, 980, 736, 876, 478, 430, 305, 170, 514, 364, 692, 829,
    82, 855, 953, 676, 246, 369, 970, 294, 750, 807, 827, 150, 790, 288, 923,
    804, 378, 215, 828, 592, 281, 565, 555, 710, 82, 896, 831, 547, 261, 524,
    462, 293, 465, 502, 56, 661, 821, 976, 991, 658, 869, 905, 758, 745, 193,
    768, 550, 608, 933, 378, 286, 215, 979, 792, 961, 61, 688, 793, 644, 986,
    403, 106, 366, 905, 644, 372, 567, 466, 434, 645, 210, 389, 550, 919, 135,
    780, 773, 635, 389, 707, 100, 626, 958, 165, 504, 920, 176, 193, 713, 857,
    265, 203, 50, 668, 108, 645, 990, 626, 197, 510, 357, 358, 850, 858, 364,
    936, 638,
];

struct BlockRandomise {
    n2go: usize,
    t_pos: usize,
}

impl BlockRandomise {
    pub fn new() -> Self {
        Self { n2go: 0, t_pos: 0 }
    }

    pub fn reset(&mut self) {
        self.n2go = 0;
        self.t_pos = 0;
    }

    pub fn next(&mut self) -> bool {
        if self.n2go == 0 {
            self.n2go = BZ2_R_NUMS[self.t_pos];
            self.t_pos += 1;
            if self.t_pos == 512 {
                self.t_pos = 0;
            }
        }
        self.n2go -= 1;
        self.n2go == 1
    }
}

pub struct BZip2Decoder {
    block_no: usize,
    block_size_100k: usize,
    combined_crc: u32,
    block_crc: u32,
    block_crc_digest: BuiltinDigest,
    tt: Vec<u32>,
    n_block_used: usize,
    t_pos: u32,
    block_randomise: BlockRandomise,
    block_randomised: bool,
    result_count: usize,
    result_wrote_count: usize,
    result_charactor: u8,
    stream_no: usize,
}

impl Default for BZip2Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl BZip2Decoder {
    const RUN_A: u16 = 0;
    const RUN_B: u16 = 1;

    pub fn new() -> Self {
        Self {
            block_no: 0,
            block_size_100k: 0,
            combined_crc: 0,
            block_crc: 0,
            block_crc_digest: IEEE_NORMAL.build_hasher(),
            tt: Vec::new(),
            n_block_used: 0,
            t_pos: 0,
            block_randomise: BlockRandomise::new(),
            block_randomised: false,
            result_count: 0,
            result_wrote_count: 0,
            result_charactor: 0,
            stream_no: 1,
        }
    }

    fn read_u8<R: BitRead<Left>>(reader: &mut R) -> Result<u8, String> {
        reader.read_bits(8).map(|x| x.data())
    }

    fn read_u32<R: BitRead<Left>>(reader: &mut R) -> Result<u32, String> {
        reader.read_bits(32).map(|x| x.data())
    }

    fn check_u8<R: BitRead<Left>>(
        reader: &mut R,
        value: u8,
    ) -> Result<bool, String> {
        Self::read_u8(reader).map(|x| x == value)
    }

    fn init_block<R: BitRead<Left>>(
        &mut self,
        reader: &mut R,
    ) -> Result<bool, BZip2Error> {
        loop {
            if self.block_no == 0 {
                let magic_err = if self.stream_no == 1 {
                    BZip2Error::DataErrorMagicFirst
                } else {
                    BZip2Error::DataErrorMagic
                };
                try!(Self::check_u8(reader, HEADER_B).map_err(|_| magic_err));
                try!(Self::check_u8(reader, HEADER_Z).map_err(|_| magic_err));
                try!(Self::check_u8(reader, HEADER_h).map_err(|_| magic_err));
                self.block_size_100k = {
                    let b = try!(
                        Self::read_u8(reader)
                            .map_err(|_| BZip2Error::UnexpectedEof)
                    );
                    if b < 1 + HEADER_0 || b > 9 + HEADER_0 {
                        return Err(magic_err);
                    }
                    usize::from(b - HEADER_0)
                };
            } else {
                let data_block_crc = self.block_crc_digest.finish() as u32;
                debug!(
                    " {{0x{:08x}, 0x{:08x}}}]",
                    self.block_crc, data_block_crc
                );
                if data_block_crc != self.block_crc {
                    return Err(BZip2Error::DataError);
                }
                self.combined_crc = ((self.combined_crc << 1)
                    | (self.combined_crc >> 31))
                    ^ self.block_crc;
                self.block_crc_digest = IEEE_NORMAL.build_hasher();
            }

            let block_head_byte = try!(
                Self::read_u8(reader).map_err(|_| BZip2Error::UnexpectedEof)
            );

            if block_head_byte == 0x31 {
                try!(
                    Self::check_u8(reader, 0x41)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x59)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x26)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x53)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x59)
                        .map_err(|_| BZip2Error::DataError)
                );
                self.block_no += 1;
                debug!("    [{}: huff+mtf ", self.block_no);

                self.block_crc = try!(
                    Self::read_u32(reader)
                        .map_err(|_| BZip2Error::UnexpectedEof)
                );
                self.block_randomised = try!(
                    reader
                        .read_bits::<u8>(1)
                        .map_err(|_| BZip2Error::UnexpectedEof)
                ).data() == 1;

                let orig_pos = try!(
                    reader
                        .read_bits::<u32>(24)
                        .map_err(|_| BZip2Error::UnexpectedEof)
                ).data() as usize;

                if orig_pos > 10 + 100_000 * self.block_size_100k {
                    return Err(BZip2Error::DataError);
                }

                /*--- Receive the mapping table ---*/
                let seq2unseq = {
                    let mut in_use16 = BitArray::new(16);
                    for i in 0..16 {
                        in_use16.set(
                            i,
                            try!(
                                reader
                                    .read_bits::<u8>(1)
                                    .map_err(|_| BZip2Error::UnexpectedEof)
                            ).data() == 1,
                        );
                    }

                    let mut ret = Vec::with_capacity(256);
                    for (i, _) in
                        in_use16.iter().enumerate().filter(|&(_, x)| x)
                    {
                        for j in 0..16 {
                            if try!(
                                reader
                                    .read_bits::<u8>(1)
                                    .map_err(|_| BZip2Error::UnexpectedEof)
                            ).data() == 1
                            {
                                ret.push(i * 16 + j)
                            }
                        }
                    }
                    ret
                };

                if seq2unseq.is_empty() {
                    return Err(BZip2Error::DataError);
                }

                let alpha_size = seq2unseq.len() + 2;

                /*--- Now the selectors ---*/
                let n_groups = try!(
                    reader.read_bits(3).map_err(|_| BZip2Error::UnexpectedEof)
                ).data();
                if n_groups < 2 || n_groups > 6 {
                    return Err(BZip2Error::DataError);
                }
                let n_selectors = try!(
                    reader.read_bits(15).map_err(|_| BZip2Error::UnexpectedEof)
                ).data();
                if n_selectors < 1 {
                    return Err(BZip2Error::DataError);
                }

                let mut selector = Vec::with_capacity(n_selectors);
                {
                    let mut selector_mtf_dec =
                        MtfPositionDecoder::new(n_groups);
                    for _ in 0..n_selectors {
                        let mut j = 0;
                        while try!(
                            reader
                                .read_bits::<u8>(1)
                                .map_err(|_| BZip2Error::UnexpectedEof)
                        ).data() != 0
                        {
                            j += 1;
                            if j >= n_groups {
                                return Err(BZip2Error::DataError);
                            }
                        }
                        /*--- Undo the MTF values for the selectors. ---*/
                        selector.push(selector_mtf_dec.pop(j));
                    }
                }

                let mut len = vec![vec![0; alpha_size]; n_groups];
                /*--- Now the coding tables ---*/
                for t in &mut len {
                    let mut curr = try!(
                        reader
                            .read_bits::<u8>(5)
                            .map_err(|_| BZip2Error::UnexpectedEof)
                    ).data();
                    for i in t.iter_mut() {
                        while try!(
                            reader
                                .read_bits::<u8>(1)
                                .map_err(|_| BZip2Error::UnexpectedEof)
                        ).data() != 0
                        {
                            if curr < 1 || curr > 20 {
                                return Err(BZip2Error::DataError);
                            }
                            if try!(
                                reader
                                    .read_bits::<u8>(1)
                                    .map_err(|_| BZip2Error::UnexpectedEof)
                            ).data() == 0
                            {
                                curr += 1;
                            } else {
                                curr -= 1;
                            }
                        }
                        *i = curr;
                    }
                }

                /*--- Create the Huffman decoding tables ---*/
                let mut code = Vec::with_capacity(n_groups);
                for l in &len {
                    code.push(try!(
                        HuffmanDecoder::<Left>::new(l, 12)
                            .map_err(|_| BZip2Error::DataError)
                    ));
                }

                /*--- Now the MTF values ---*/
                let eob = alpha_size as u16 - 1;
                let nblock_max = 100_000 * self.block_size_100k;

                let mut unzftab = vec![0; 257]; // LF-mapping Table
                self.tt.clear();
                self.tt.reserve_exact(nblock_max);

                {
                    let mut group_no = 0;
                    let mut group_pos = 0;
                    let mut n = 1;
                    let mut es = 0;

                    let mut mtf_decoder =
                        MtfPositionDecoder::new(seq2unseq.len());

                    loop {
                        if group_pos == 0 {
                            group_no += 1;
                            if group_no > n_selectors {
                                return Err(BZip2Error::DataError);
                            }
                            group_pos = BZ_G_SIZE;
                        }
                        group_pos -= 1;
                        let next_sym = try!(
                            try!(
                                code[selector[group_no - 1]]
                                    .dec(reader)
                                    .map_err(|_| BZip2Error::DataError)
                            ).ok_or_else(|| BZip2Error::DataError)
                        );

                        if es > 0 && next_sym != Self::RUN_A
                            && next_sym != Self::RUN_B
                        {
                            let uc = seq2unseq[mtf_decoder.pop(0)];
                            unzftab[uc + 1] += es;
                            for _ in 0..es {
                                self.tt.push(uc as u32);
                            }
                            if self.tt.len() >= nblock_max {
                                return Err(BZip2Error::DataError);
                            }
                            n = 1;
                            es = 0;
                        }

                        if next_sym == eob {
                            break;
                        }

                        /* Check that N doesn't get too big, so that es
                        doesn't go negative.  The maximum value that can
                        be RUNA/RUNB encoded is equal to the block size
                        (post the initial RLE), viz, 900k, so bounding N
                        at 2 million should guard against overflow
                        without rejecting any legitimate inputs. */
                        if n >= 2 * 1024 * 1024 {
                            return Err(BZip2Error::DataError);
                        }

                        if next_sym == Self::RUN_A {
                            es += n;
                            n <<= 1;
                        } else if next_sym == Self::RUN_B {
                            n <<= 1;
                            es += n;
                        } else {
                            if self.tt.len() >= nblock_max {
                                return Err(BZip2Error::DataError);
                            }

                            let uc = seq2unseq
                                [mtf_decoder.pop(next_sym as usize - 1)];
                            unzftab[uc + 1] += 1;
                            self.tt.push(uc as u32);
                        }
                    }
                }

                /* Now we know what nblock is, we can do a better sanity
                check on s->origPtr. */
                if orig_pos >= self.tt.len() {
                    return Err(BZip2Error::DataError);
                }

                /*-- Set up cftab to facilitate generation of T^(-1) --*/
                /* Actually generate cftab. */
                if unzftab[0] != 0 {
                    return Err(BZip2Error::DataError);
                }

                for i in 1..unzftab.len() {
                    // /* Check: unzftab entries in range. */
                    // if (unzftab[i] < 0 || unzftab[i] > nblock)
                    //     throw new InvalidDataException();
                    unzftab[i] += unzftab[i - 1];
                    /* Check: cftab entries non-descending. */
                    if unzftab[i - 1] > unzftab[i] {
                        return Err(BZip2Error::DataError);
                    }
                }
                /* Check: cftab entries in range. */
                if unzftab[unzftab.len() - 1] != self.tt.len() {
                    return Err(BZip2Error::DataError);
                }

                debug!("rt+rld");

                /*-- compute the T^(-1) vector --*/
                for i in 0..self.tt.len() {
                    let uc = (self.tt[i] & 0xFF) as usize;
                    self.tt[unzftab[uc]] |= (i as u32) << 8;
                    unzftab[uc] += 1;
                }

                self.t_pos = self.tt[orig_pos] >> 8;
                self.n_block_used = 0;

                if self.block_randomised {
                    self.block_randomise.reset();
                }

                self.result_count = 0;
                self.result_wrote_count = 0;

                return Ok(true);
            } else if block_head_byte == 0x17 {
                try!(
                    Self::check_u8(reader, 0x72)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x45)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x38)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x50)
                        .map_err(|_| BZip2Error::DataError)
                );
                try!(
                    Self::check_u8(reader, 0x90)
                        .map_err(|_| BZip2Error::DataError)
                );
                let stored_combind_crc = try!(
                    Self::read_u32(reader)
                        .map_err(|_| BZip2Error::UnexpectedEof)
                );
                debug!(
                    "    combined CRCs: stored = 0x{:08x}, computed = 0x{:08x}",
                    stored_combind_crc, self.combined_crc
                );
                if stored_combind_crc != self.combined_crc {
                    return Err(BZip2Error::DataError);
                }
                reader.skip_to_next_byte();
                let next = try!(
                    reader
                        .peek_bits::<usize>(8)
                        .map_err(|_| BZip2Error::Unexpected)
                );
                if next.len() == 8 {
                    self.block_no = 0;
                    self.combined_crc = 0;
                    self.stream_no += 1;
                } else {
                    return Ok(false);
                }
            } else {
                return Err(BZip2Error::DataError);
            }
        }
    }

    fn get_next_lfm(&mut self) -> Result<u8, BZip2Error> {
        let mut position = self.t_pos;
        /* c_tPos is unsigned, hence test < 0 is pointless. */
        if position >= 100_000 * self.block_size_100k as u32 {
            return Err(BZip2Error::DataError);
        }
        position = self.tt[position as usize];
        let mut k0 = position as u8;
        self.t_pos = position >> 8;
        self.n_block_used += 1;
        if self.block_randomised {
            k0 ^= if self.block_randomise.next() { 1 } else { 0 };
        }

        Ok(k0)
    }
}

impl<R> Decoder<R> for BZip2Decoder
where
    R: BitRead<Left>,
{
    type Error = BZip2Error;
    type Output = u8;

    fn next(&mut self, iter: &mut R) -> Result<Option<u8>, Self::Error> {
        if self.result_count == self.result_wrote_count {
            if self.n_block_used == self.tt.len()
                && !try!(self.init_block(iter))
            {
                return Ok(None);
            }

            let buffer = try!(self.get_next_lfm());
            if buffer == self.result_charactor {
                self.result_count += 1;
                self.result_wrote_count += 1;
            } else {
                self.result_charactor = buffer;
                self.result_count = 1;
                self.result_wrote_count = 1;
            }

            if self.result_count == 4 {
                self.result_count += usize::from(try!(self.get_next_lfm()));
            }
        } else {
            self.result_wrote_count += 1;
        }
        self.block_crc_digest.write_u8(self.result_charactor);
        Ok(Some(self.result_charactor))
    }
}
