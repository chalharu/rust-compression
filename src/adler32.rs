//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.
#![cfg(feature = "zlib")]

use crate::core::hash::Hasher;

#[derive(Debug)]
pub(crate) struct Adler32 {
    a: u32,
    b: u32,
    t: u16,
}

impl Default for Adler32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Adler32 {
    const LOOP_SIZE: u16 = 5549;
    const MOD_ADLER: u32 = 0xFFF1;

    pub(crate) fn new() -> Adler32 {
        Self {
            a: 1,
            b: 0,
            t: Self::LOOP_SIZE,
        }
    }
}

impl Hasher for Adler32 {
    fn write_u8(&mut self, byte: u8) {
        let byte = u32::from(byte);
        self.a += byte;
        self.b += self.a;
        if self.t == 0 {
            self.t = Self::LOOP_SIZE;
            self.a %= Self::MOD_ADLER;
            self.b %= Self::MOD_ADLER;
        } else {
            self.t -= 1;
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u8(*b);
        }
    }

    fn finish(&self) -> u64 {
        u64::from(
            ((self.b % Self::MOD_ADLER) << 16) | (self.a % Self::MOD_ADLER),
        )
    }
}
