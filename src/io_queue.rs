//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use std::cell::RefCell;
use std::cmp::min;
use std::io::{Read, Result, Write};
use std::ptr;
use std::rc::Rc;

#[derive(Debug)]
pub struct IOQueue {
    buf: Vec<u8>,
    pos: usize,
}

impl IOQueue {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(8192),
            pos: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len() - self.pos
    }
}

impl Read for IOQueue {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let rlen = min(self.len(), buf.len());
        unsafe {
            ptr::copy_nonoverlapping(
                self.buf.as_ptr().offset(self.pos as isize),
                buf.as_mut_ptr(),
                rlen,
            );
        }
        self.pos += rlen;
        Ok(rlen)
    }
}

impl Write for IOQueue {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.len() + self.len() > self.buf.capacity() {
            let reserve_size = buf.len() + self.len();
            self.buf.reserve(reserve_size);
        }
        let wlen = min(self.buf.capacity() - self.len(), buf.len());
        if wlen > self.buf.capacity() - self.buf.len() {
            let l = self.len();
            for i in 0..l {
                self.buf[i] = self.buf[self.pos + i];
            }
            self.pos = 0;
        }
        let slen = self.buf.len();

        unsafe {
            ptr::copy_nonoverlapping(
                buf.as_ptr(),
                self.buf.as_mut_ptr().offset((self.pos + slen) as isize),
                wlen,
            );
            self.buf.set_len(slen + wlen);
        }
        Ok(wlen)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RcIOQueue {
    inner: Rc<RefCell<IOQueue>>,
}

impl RcIOQueue {
    pub fn new() -> Self {
        Self { inner: Rc::new(RefCell::new(IOQueue::new())) }
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }
}

impl Read for RcIOQueue {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.inner.borrow_mut().read(buf)
    }
}

impl Write for RcIOQueue {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.inner.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.borrow_mut().flush()
    }
}
