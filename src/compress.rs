//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use std::io::Result as ioResult;
use std::slice;

pub trait Compress {
    fn total_in(&self) -> u64;
    fn total_out(&self) -> u64;
    fn compress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        action: Action,
    ) -> ioResult<(usize, usize)>;

    fn compress_vec(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        action: Action,
    ) -> ioResult<(usize, usize)> {
        let len = output.len();
        let out = unsafe {
            slice::from_raw_parts_mut(
                output.as_mut_ptr().offset(len as isize),
                output.capacity() - len,
            )
        };
        let iolen = try!(self.compress(input, out, action));
        let nlen = (iolen.0, iolen.1 + len);
        unsafe {
            output.set_len(nlen.1);
        }
        Ok(nlen)
    }
}

pub enum Action {
    Run,
    Flush,
    Finish,
}
