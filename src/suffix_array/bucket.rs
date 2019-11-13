use crate::core::ops::{Index, IndexMut};
#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::vec;

pub(crate) struct BucketBuilder<'a, T> {
    data: Box<[usize]>, // want to use RawVec but that is unstable
    array: &'a [T],     // want to use RawVec but that is unstable
    min: usize,
}

impl<'a, T: Copy> BucketBuilder<'a, T>
where
    usize: From<T>,
{
    pub(crate) fn new(array: &'a [T], min: usize, max: usize) -> Self {
        let mut data = vec![0; max - min + 2].into_boxed_slice();

        for v in array {
            let v = usize::from(*v);
            if v > max {
                panic!("out of range: max");
            }
            if v < min {
                panic!("out of range: min");
            }
            data[v - min] += 1;
        }

        let mut sum = 0;
        for d in data.iter_mut() {
            let val = *d;
            *d = sum;
            sum += val;
        }
        Self { array, data, min }
    }

    pub(crate) fn build(&self, has_end: bool) -> Bucket<'a, T> {
        let mut data = vec![0; self.data.len() - 1].into_boxed_slice();
        if has_end {
            for d in self.data.iter().skip(1).zip(data.iter_mut()) {
                *d.1 = *d.0;
            }
        } else {
            for d in self.data.iter().zip(data.iter_mut()) {
                *d.1 = *d.0;
            }
        }
        Bucket::new(self.array, data, self.min)
    }
}

pub(crate) struct Bucket<'a, T> {
    data: Box<[usize]>, // want to use RawVec but that is unstable
    array: &'a [T],     // want to use RawVec but that is unstable
    min: usize,
}

impl<'a, T> Bucket<'a, T> {
    pub(crate) fn new(array: &'a [T], data: Box<[usize]>, min: usize) -> Self {
        Self { array, data, min }
    }
}

impl<T: Copy> Index<usize> for Bucket<'_, T>
where
    usize: From<T>,
{
    type Output = usize;
    fn index(&self, idx: usize) -> &usize {
        &self.data[usize::from(self.array[idx]) - self.min]
    }
}

impl<T: Copy> IndexMut<usize> for Bucket<'_, T>
where
    usize: From<T>,
{
    fn index_mut(&mut self, idx: usize) -> &mut usize {
        &mut self.data[usize::from(self.array[idx]) - self.min]
    }
}
