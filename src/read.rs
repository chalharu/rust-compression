//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use std::{cmp, fmt, mem, ptr};
use std::io::{Error, ErrorKind, Result};

const DEFAULT_BUF_SIZE: usize = 8 * 1024;

struct Guard<'a, T: 'a> {
    buf: &'a mut Vec<T>,
    len: usize,
}

impl<'a, T> Drop for Guard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            self.buf.set_len(self.len);
        }
    }
}

fn read_to_end<T: Default, R: Read<T> + ?Sized>(
    r: &mut R,
    buf: &mut Vec<T>,
) -> Result<usize> {
    let start_len = buf.len();
    let mut g = Guard {
        len: buf.len(),
        buf: buf,
    };
    let mut new_write_size = 16;
    let ret;
    loop {
        if g.len == g.buf.len() {
            if new_write_size < DEFAULT_BUF_SIZE / mem::size_of::<T>() {
                new_write_size *= 2;
            }
            unsafe {
                g.buf.reserve(new_write_size);
                g.buf.set_len(g.len + new_write_size);
                r.initializer().initialize(&mut g.buf[g.len..]);
            }
        }

        match r.read(&mut g.buf[g.len..]) {
            Ok(0) => {
                ret = Ok(g.len - start_len);
                break;
            }
            Ok(n) => g.len += n,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                ret = Err(e);
                break;
            }
        }
    }

    ret
}

pub struct Chain<T, U> {
    first: T,
    second: U,
    done_first: bool,
}

impl<T, U> Chain<T, U> {
    pub fn into_inner(self) -> (T, U) {
        (self.first, self.second)
    }

    pub fn get_ref(&self) -> (&T, &U) {
        (&self.first, &self.second)
    }

    pub fn get_mut(&mut self) -> (&mut T, &mut U) {
        (&mut self.first, &mut self.second)
    }
}

impl<T: fmt::Debug, U: fmt::Debug> fmt::Debug for Chain<T, U> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Chain")
            .field("t", &self.first)
            .field("u", &self.second)
            .finish()
    }
}

impl<I: Default, T: Read<I>, U: Read<I>> Read<I> for Chain<T, U> {
    fn read(&mut self, buf: &mut [I]) -> Result<usize> {
        if !self.done_first {
            match self.first.read(buf)? {
                0 if buf.len() != 0 => {
                    self.done_first = true;
                }
                n => return Ok(n),
            }
        }
        self.second.read(buf)
    }

    unsafe fn initializer(&self) -> Initializer {
        let initializer = self.first.initializer();
        if initializer.should_initialize() {
            initializer
        } else {
            self.second.initializer()
        }
    }
}

pub trait Read<T: Default> {
    fn read(&mut self, buf: &mut [T]) -> Result<usize>;

    #[inline]
    unsafe fn initializer(&self) -> Initializer {
        Initializer::zeroing()
    }

    fn read_to_end(&mut self, buf: &mut Vec<T>) -> Result<usize> {
        read_to_end(self, buf)
    }

    fn read_exact(&mut self, mut buf: &mut [T]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(Error::new(
                ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ))
        } else {
            Ok(())
        }
    }
    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self
    }

    fn chain<R: Read<T>>(self, next: R) -> Chain<Self, R>
    where
        Self: Sized,
    {
        Chain {
            first: self,
            second: next,
            done_first: false,
        }
    }

    fn take(self, limit: u64) -> Take<Self>
    where
        Self: Sized,
    {
        Take {
            inner: self,
            limit: limit,
        }
    }
}

#[derive(Debug)]
pub struct Take<T> {
    inner: T,
    limit: u64,
}

impl<T> Take<T> {
    pub fn limit(&self) -> u64 {
        self.limit
    }
    pub fn set_limit(&mut self, limit: u64) {
        self.limit = limit;
    }
    pub fn into_inner(self) -> T {
        self.inner
    }
    pub fn get_ref(&self) -> &T {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<I: Default, T: Read<I>> Read<I> for Take<T> {
    fn read(&mut self, buf: &mut [I]) -> Result<usize> {
        // Don't call into inner reader at all at EOF because it may still block
        if self.limit == 0 {
            return Ok(0);
        }

        let max = cmp::min(buf.len() as u64, self.limit) as usize;
        let n = self.inner.read(&mut buf[..max])?;
        self.limit -= n as u64;
        Ok(n)
    }

    unsafe fn initializer(&self) -> Initializer {
        self.inner.initializer()
    }
}


#[derive(Debug)]
pub struct Initializer(bool);

impl Initializer {
    #[inline]
    pub fn zeroing() -> Initializer {
        Initializer(true)
    }

    #[inline]
    pub unsafe fn nop() -> Initializer {
        Initializer(false)
    }

    #[inline]
    pub fn should_initialize(&self) -> bool {
        self.0
    }

    #[inline]
    pub fn initialize<T: Default>(&self, buf: &mut [T]) {
        if self.should_initialize() {
            for i in 0..buf.len() as isize {
                unsafe {
                    ptr::write(buf.as_mut_ptr().offset(i), Default::default());
                }
            }
        }
    }
}

impl<'a, T: Clone + Default> Read<T> for &'a [T] {
    #[inline]
    fn read(&mut self, buf: &mut [T]) -> Result<usize> {
        let amt = cmp::min(buf.len(), self.len());
        let (a, b) = self.split_at(amt);

        // First check if the amount of bytes we want to read is small:
        // `copy_from_slice` will generally expand to a call to `memcpy`, and
        // for a single byte the overhead is significant.
        if amt == 1 {
            buf[0] = a[0].clone();
        } else {
            buf[..amt].clone_from_slice(a);
        }

        *self = b;
        Ok(amt)
    }
}

impl<T: Clone + Default> Read<T> for Vec<T> {
    #[inline]
    fn read(&mut self, buf: &mut [T]) -> Result<usize> {
        let amt = cmp::min(buf.len(), self.len());
        let c = self.clone();
        let (a, b) = c.split_at(amt);

        // First check if the amount of bytes we want to read is small:
        // `copy_from_slice` will generally expand to a call to `memcpy`, and
        // for a single byte the overhead is significant.
        if amt == 1 {
            buf[0] = a[0].clone();
        } else {
            buf[..amt].clone_from_slice(a);
        }

        *self = b.to_vec();
        Ok(amt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_to_end() {
        let mut c: &[u32] = &[];
        let mut v = Vec::new();
        assert_eq!(c.read_to_end(&mut v).unwrap(), 0);
        assert_eq!(v, []);

        let mut c: &[u32] = &[1];
        let mut v = Vec::new();
        assert_eq!(c.read_to_end(&mut v).unwrap(), 1);
        assert_eq!(v, [1]);

        let cap = 1024 * 1024;
        let data = (0..cap).map(|i| (i / 3) as u32).collect::<Vec<_>>();
        let mut v = Vec::new();
        let (a, b) = data.split_at(data.len() / 2);
        assert_eq!(a.clone().read_to_end(&mut v).unwrap(), a.len());
        assert_eq!(b.clone().read_to_end(&mut v).unwrap(), b.len());
        assert_eq!(v, data);
    }

    #[test]
    fn read_exact() {
        let mut buf = [0_u32; 4];

        let mut c: &[u32] = &[];
        assert_eq!(
            c.read_exact(&mut buf).unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );

        let c1: &[u32] = &[1, 2, 3];
        let c2: &[u32] = &[4, 5, 6, 7, 8, 9];
        let mut c = c1.chain(c2);
        c.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, &[1, 2, 3, 4]);
        c.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, &[5, 6, 7, 8]);
        assert_eq!(
            c.read_exact(&mut buf).unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn read_exact_slice() {
        let mut buf = [0; 4];

        let mut c = &b""[..];
        assert_eq!(
            c.read_exact(&mut buf).unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );

        let mut c = &b"123"[..];
        assert_eq!(
            c.read_exact(&mut buf).unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );

        let mut c = &b"1234"[..];
        c.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"1234");

        let mut c = &b"56789"[..];
        c.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"5678");
        assert_eq!(c, b"9");
    }

    #[test]
    fn take_eof() {
        struct R;

        impl Read<u32> for R {
            fn read(&mut self, _: &mut [u32]) -> Result<usize> {
                Err(Error::new(ErrorKind::Other, ""))
            }
        }

        let mut buf = [0; 1];
        assert_eq!(0, R.take(0).read(&mut buf).unwrap());
    }
}
