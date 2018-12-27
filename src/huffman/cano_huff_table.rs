//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

fn down_heap(buf: &mut [usize], mut n: usize, len: usize) {
    let tmp = buf[n];
    let mut leaf = (n << 1) + 1;

    while leaf < len {
        if leaf + 1 < len && buf[buf[leaf]] > buf[buf[leaf + 1]] {
            leaf += 1;
        }

        if buf[tmp] < buf[buf[leaf]] {
            break;
        }
        buf[n] = buf[leaf];
        n = leaf;
        leaf = (n << 1) + 1;
    }
    buf[n] = tmp;
}

fn create_heap(buf: &mut [usize]) {
    let s = buf.len() >> 1;
    for i in (0..(s >> 1)).rev() {
        down_heap(buf, i, s);
    }
}

fn take_package(
    ty: &mut [Vec<usize>],
    len: &mut [usize],
    cur: &mut [usize],
    i: usize,
) {
    let x = ty[i][cur[i]];
    if x == len.len() {
        take_package(ty, len, cur, i + 1);
        take_package(ty, len, cur, i + 1);
    } else {
        len[x] -= 1;
    }

    cur[i] += 1;
}

/// Reverse package merge
fn gen_code_lm<F: Fn(usize, usize) -> usize>(
    freq: &[usize],
    lim: usize,
    weight_add_fn: F,
) -> Vec<u8> {
    let len = freq.len();
    let mut freqmap = freq.iter()
        .enumerate()
        .map(|(i, &f)| (i, f))
        .collect::<Vec<_>>();
    freqmap.sort_by(|x, y| y.1.cmp(&x.1));
    let (map, sfreq): (Vec<_>, Vec<_>) = freqmap.into_iter().unzip();

    let mut max_elem = vec![0; lim];
    let mut b = vec![0; lim];

    let mut excess = (1 << lim) - len;
    let half = 1 << (lim - 1);
    max_elem[lim - 1] = len;

    for j in 0..lim {
        if excess >= half {
            b[j] = 1;
            excess -= half;
        }
        excess <<= 1;
        if lim >= 2 + j {
            max_elem[lim - 2 - j] = max_elem[lim - 1 - j] / 2 + len;
        }
    }

    max_elem[0] = b[0];
    for j in 1..lim {
        if max_elem[j] > 2 * max_elem[j - 1] + b[j] {
            max_elem[j] = 2 * max_elem[j - 1] + b[j];
        }
    }

    let mut val = (0..lim)
        .map(|i| vec![0; max_elem[i]])
        .collect::<Vec<_>>();
    let mut ty = (0..lim)
        .map(|i| vec![0; max_elem[i]])
        .collect::<Vec<_>>();
    let mut c = vec![lim; len];

    for (t, &s) in sfreq.iter().enumerate().take(max_elem[lim - 1]) {
        val[lim - 1][t] = s;
        ty[lim - 1][t] = t;
    }

    let mut cur = vec![0; lim];
    if b[lim - 1] == 1 {
        c[0] -= 1;
        cur[lim - 1] += 1;
    }

    let mut j = lim - 1;
    while j > 0 {
        let mut i = 0;
        let mut next = cur[j];

        for t in 0..max_elem[j - 1] {
            let weight = if next + 1 < max_elem[j] {
                weight_add_fn(val[j][next], val[j][next + 1])
            } else {
                0
            };
            if weight > sfreq[i] {
                val[j - 1][t] = weight;
                ty[j - 1][t] = len;
                next += 2;
            } else {
                val[j - 1][t] = sfreq[i];
                ty[j - 1][t] = i;
                i += 1;
                if i >= len {
                    break;
                }
            }
        }

        j -= 1;
        cur[j] = 0;
        if b[j] == 1 {
            take_package(&mut ty, &mut c, &mut cur, j);
        }
    }

    let mut r = c.iter()
        .zip(map)
        .map(|(&x, i)| (x as u8, i))
        .collect::<Vec<_>>();
    r.sort_unstable_by_key(|v| v.1);
    r.into_iter()
        .map(move |v| v.0)
        .collect::<Vec<_>>()
}

fn gen_code<F: Fn(usize, usize) -> usize>(
    freq: &[usize],
    lim: usize,
    weight_add_fn: F,
) -> Vec<u8> {
    if freq.len() == 1 {
        vec![1]
    } else {
        let mut buf = (freq.len()..(freq.len() << 1))
            .chain(freq.iter().cloned())
            .collect::<Vec<_>>();

        create_heap(&mut buf);

        // Generate Huffman Tree
        for i in (1..freq.len()).rev() {
            let m1 = buf[0];
            buf[0] = buf[i];
            down_heap(&mut buf, 0, i);
            let m2 = buf[0];
            buf[i] = weight_add_fn(buf[m1], buf[m2]);
            buf[0] = i;
            buf[m1] = i;
            buf[m2] = i;
            down_heap(&mut buf, 0, i);
        }

        // Counting
        buf[1] = 0;
        for i in 2..freq.len() {
            buf[i] = buf[buf[i]] + 1;
        }

        let ret = (0..freq.len())
            .map(|i| (buf[buf[i + freq.len()]] + 1) as u8)
            .collect::<Vec<_>>();

        if ret.iter().any(|l| *l as usize > lim) {
            gen_code_lm(freq, lim, weight_add_fn)
        } else {
            ret
        }
    }
}

pub fn make_tab_with_fn<F: Fn(usize, usize) -> usize>(
    freq: &[usize],
    lim: usize,
    weight_add_fn: F,
) -> Vec<u8> {
    if freq.is_empty() {
        Vec::new()
    } else {
        let (s, l): (Vec<_>, Vec<_>) = freq.iter()
            .enumerate()
            .filter_map(|(i, &t)| if t != 0 { Some((i, t)) } else { None })
            .unzip();
        if s.is_empty() {
            Vec::new()
        } else {
            s.into_iter()
                .zip(gen_code(&l, lim, weight_add_fn))
                .scan(0, move |c, (s, v)| {
                    let r = vec![0; s - *c].into_iter().chain(vec![v]);
                    *c = s + 1;
                    Some(r)
                })
                .flat_map(move |v| v)
                .collect()
        }
    }
}

#[cfg(any(feature = "deflate", feature = "lzhuf", test))]
pub fn make_table(freq: &[usize], lim: usize) -> Vec<u8> {
    make_tab_with_fn(freq, lim, |x, y| x + y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp;

    #[test]
    fn create_haffman_tab() {
        let freq = vec![0, 1, 1, 2, 2, 4, 4, 8, 8];
        let tab = make_table(&freq, 12);

        assert_eq!(
            tab.iter()
                .zip(freq)
                .map(|(x, y)| *x as usize * y)
                .sum::<usize>(),
            80
        );
    }

    #[test]
    fn create_haffman_tab_with_fn() {
        let symb_len = vec![0_u8, 4, 4, 4, 4, 3, 3, 2, 2];
        let freq = vec![0_usize, 1, 1, 2, 2, 4, 4, 8, 8];
        let tab = make_tab_with_fn(
            &freq.iter().map(|i| i << 8).collect::<Vec<_>>(),
            12,
            |x, y| {
                ((x & !0xFF) + (y & !0xFF)) | (cmp::max(x & 0xFF, y & 0xFF) + 1)
            },
        );

        assert_eq!(tab, symb_len);
    }

    #[test]
    fn create_haffman_tab_with_fn_lim_len() {
        let freq = (0..63).collect::<Vec<_>>();
        let tab = make_tab_with_fn(
            &freq.iter().map(|i| i << 8).collect::<Vec<_>>(),
            8,
            |x, y| {
                ((x & !0xFF) + (y & !0xFF)) | (cmp::max(x & 0xFF, y & 0xFF) + 1)
            },
        );

        assert!(
            tab.iter()
                .zip(freq.clone())
                .map(|(x, y)| *x as usize * y)
                .sum::<usize>() < freq.iter().sum::<usize>() * 6
        );

        assert!(*tab.iter().max().unwrap() <= 8);
    }

    #[test]
    fn create_haffman_tab_unit() {
        let freq = vec![0, 1];
        let tab = make_table(&freq, 12);

        assert_eq!(tab, vec![0, 1]);
    }
}
