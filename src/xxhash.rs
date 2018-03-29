//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(feature = "lz4")]

use core::hash::Hasher;
use core::ptr;

pub struct XXH32 {
    total_len: u64,
    v1: u32,
    v2: u32,
    v3: u32,
    v4: u32,
    mem: [u8; 16],
    memsize: usize,
    seed: u32,
}

impl Default for XXH32 {
    fn default() -> Self {
        Self::new(0)
    }
}

impl XXH32 {
    const PRIME32_1: u32 = 2654435761;
    const PRIME32_2: u32 = 2246822519;
    const PRIME32_3: u32 = 3266489917;
    const PRIME32_4: u32 = 668265263;
    const PRIME32_5: u32 = 374761393;

    pub fn new(seed: u32) -> XXH32 {
        Self {
            v1: seed.wrapping_add(Self::PRIME32_1)
                .wrapping_add(Self::PRIME32_2),
            v2: seed.wrapping_add(Self::PRIME32_2),
            v3: seed,
            v4: seed.wrapping_sub(Self::PRIME32_1),
            total_len: 0,
            mem: [0; 16],
            memsize: 0,
            seed,
        }
    }

    #[inline]
    fn xxh_rotl32(x: u32, r: u32) -> u32 {
        ((x << r) | (x >> (32 - r)))
    }

    #[inline]
    fn xxh32_round(seed: u32, input: u32) -> u32 {
        Self::xxh_rotl32(
            seed.wrapping_add(input.wrapping_mul(Self::PRIME32_2)),
            13,
        ).wrapping_mul(Self::PRIME32_1)
    }

    #[cfg(target_endian = "little")]
    #[inline]
    unsafe fn xxh_get32bits(ptr: *const u8) -> u32 {
        *(ptr as *const u32)
    }

    #[cfg(target_endian = "big")]
    #[inline]
    unsafe fn xxh_get32bits(ptr: *const u8) -> u32 {
        u32::from(*ptr) | u32::from(*ptr.offset(1)) << 8
            | u32::from(*ptr.offset(2)) << 16
            | u32::from(*ptr.offset(3)) << 32
    }

    #[inline]
    fn round(&mut self, memptr: *const u8) {
        unsafe {
            self.v1 = Self::xxh32_round(self.v1, Self::xxh_get32bits(memptr));
            self.v2 = Self::xxh32_round(
                self.v2,
                Self::xxh_get32bits(memptr.offset(4)),
            );
            self.v3 = Self::xxh32_round(
                self.v3,
                Self::xxh_get32bits(memptr.offset(8)),
            );
            self.v4 = Self::xxh32_round(
                self.v4,
                Self::xxh_get32bits(memptr.offset(12)),
            );
        }
    }
}

impl Hasher for XXH32 {
    fn write(&mut self, bytes: &[u8]) {
        let mut srcptr = bytes.as_ptr();
        let mut srclen = bytes.len();
        self.total_len += srclen as u64;
        let memptr = self.mem.as_mut_ptr();

        if self.memsize + srclen < 16 {
            unsafe {
                ptr::copy_nonoverlapping(
                    srcptr,
                    memptr.offset(self.memsize as isize),
                    srclen,
                );
            }
            self.memsize += srclen;
            return;
        }

        if self.memsize > 0 {
            let wlen = 16 - self.memsize as usize;
            unsafe {
                ptr::copy_nonoverlapping(
                    srcptr,
                    memptr.offset(self.memsize as isize),
                    wlen,
                );
            }
            srclen -= wlen;
            unsafe {
                srcptr = srcptr.offset(wlen as isize);
            }
            self.round(memptr);
        }

        while srclen >= 16 {
            unsafe {
                ptr::copy_nonoverlapping(srcptr, memptr, 16);
            }
            srclen -= 16;
            unsafe {
                srcptr = srcptr.offset(16);
            }

            self.round(memptr);
        }

        if srclen > 0 {
            unsafe {
                ptr::copy_nonoverlapping(srcptr, memptr, srclen);
            }
        }
        self.memsize = srclen;
    }

    fn write_u8(&mut self, value: u8) {
        self.total_len += 1 as u64;
        let memptr = self.mem.as_mut_ptr();

        unsafe {
            *memptr.offset(self.memsize as isize) = value;
        }

        if self.memsize == 15 {
            self.round(memptr);
            self.memsize = 0;
        } else {
            self.memsize += 1;
        }
    }

    fn finish(&self) -> u64 {
        unsafe {
            let mut h32 = if self.total_len >= 16 {
                Self::xxh_rotl32(self.v1, 1)
                    .wrapping_add(Self::xxh_rotl32(self.v2, 7))
                    .wrapping_add(Self::xxh_rotl32(self.v3, 12))
                    .wrapping_add(Self::xxh_rotl32(self.v4, 18))
            } else {
                self.seed.wrapping_add(Self::PRIME32_5)
            } + self.total_len as u32;

            let mut memptr = self.mem.as_ptr();

            for _ in 0..(self.memsize >> 2) {
                h32 = Self::xxh_rotl32(
                    h32.wrapping_add(
                        Self::xxh_get32bits(memptr)
                            .wrapping_mul(Self::PRIME32_3),
                    ),
                    17,
                ).wrapping_mul(Self::PRIME32_4);
                memptr = memptr.offset(4);
            }

            for _ in 0..(self.memsize & 3) {
                h32 = Self::xxh_rotl32(
                    h32.wrapping_add(
                        u32::from(*memptr).wrapping_mul(Self::PRIME32_5),
                    ),
                    11,
                ).wrapping_mul(Self::PRIME32_1);
                memptr = memptr.offset(1);
            }

            h32 ^= h32 >> 15;
            h32 = h32.wrapping_mul(Self::PRIME32_2);
            h32 ^= h32 >> 13;
            h32 = h32.wrapping_mul(Self::PRIME32_3);
            h32 ^= h32 >> 16;
            u64::from(h32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xxh32(seed: u32, input: &[u8]) -> u64 {
        let mut digest = XXH32::new(seed);
        digest.write(input);
        digest.finish()
    }

    fn xxh32_check(seed: u32, input: &[u8], result: u32) {
        assert_eq!(xxh32(seed, input), u64::from(result));
    }

    #[test]
    fn test_xxh32() {
        xxh32_check(12345, b"test", 3834992036);
        xxh32_check(0, b"a", 1426945110);
        xxh32_check(1, b"a", 4111757423);
        xxh32_check(0xFFFF_FFFF, b"a", 3443684653);
    }
    #[test]
    fn xxh32_update() {
        let mut digest = XXH32::default();
        digest.write_u8(b'a');
        assert_eq!(digest.finish(), xxh32(0, b"a"));
        digest.write_u8(b'b');
        assert_eq!(digest.finish(), xxh32(0, b"ab"));
        digest.write_u8(b'c');
        assert_eq!(digest.finish(), xxh32(0, b"abc"));
        digest.write_u8(b'd');
        assert_eq!(digest.finish(), xxh32(0, b"abcd"));
        digest.write_u8(b'e');
        assert_eq!(digest.finish(), xxh32(0, b"abcde"));
        digest.write_u8(b'f');
        assert_eq!(digest.finish(), xxh32(0, b"abcdef"));
        digest.write_u8(b'g');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefg"));
        digest.write_u8(b'h');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefgh"));
        digest.write_u8(b'i');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefghi"));
        digest.write_u8(b'j');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefghij"));
        digest.write_u8(b'k');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefghijk"));
        digest.write_u8(b'l');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefghijkl"));
        digest.write_u8(b'm');
        digest.write_u8(b'n');
        digest.write_u8(b'o');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefghijklmno"));
        digest.write_u8(b'p');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefghijklmnop"));
        digest.write_u8(b'q');
        assert_eq!(digest.finish(), xxh32(0, b"abcdefghijklmnopq"));
        digest.write_u8(b'r');
        digest.write_u8(b's');
        digest.write_u8(b't');
        assert_eq!(
            digest.finish(),
            xxh32(0, b"abcdefghijklmnopqrst")
        );
    }
}
