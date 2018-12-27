#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::mem;
use core::slice;
use core::usize;
use suffix_array::bucket::BucketBuilder;
use suffix_array::ls_type::LSTypeArray;

fn array_rotate_for_non_sentinel_bwt(
    array: &[u8],
    sarray: &mut [usize],
    bucket_max: usize,
) -> usize {
    // 初期値の投入
    let mut n1 = 0;
    let mut val = bucket_max + 1;
    let mut prev_pos = 0;
    let count = array.len();
    for (i, &a) in array.iter().enumerate() {
        let j = usize::from(a);
        if val > j {
            sarray[0] = i;
            val = j;
            n1 = 1;
            prev_pos = i;
        } else if val == j {
            prev_pos += 1;
            if prev_pos != i {
                sarray[n1] = i;
                n1 += 1;
            }
        }
    }

    for i in 0..count {
        let mut n2 = 0;
        val = bucket_max + 1;
        for j in 0..n1 {
            let mut k = sarray[j] + 1;
            if k >= count {
                k -= count;
            }

            let l = usize::from(array[k]);

            if val == l {
                sarray[n2] = k;
                n2 += 1;
            } else if val > l {
                sarray[0] = k;
                val = l;
                n2 = 1;
            }
        }
        if n2 == 1 {
            return if sarray[0] <= i {
                sarray[0] + count - i - 1
            } else {
                sarray[0] - i - 1
            };
        }
        n1 = n2;
    }
    sarray[0]
}

fn fill<T>(array: &mut [usize], offset: usize, count: usize, value: usize) {
    for a in array.iter_mut().skip(offset).take(count) {
        *a = value
    }
}

fn induce_sa<T: Copy>(
    bucket_builder: &BucketBuilder<T>,
    type_array: &LSTypeArray,
    suffix_array: &mut [usize],
    shift: usize,
) where
    usize: From<T>,
{
    // compute SAl
    {
        let mut bucket = bucket_builder.build(false);

        // compute sentinel
        let k = if shift == 0 {
            type_array.len()
        } else {
            shift
        } - 1;
        let bk = bucket[k];
        suffix_array[bk] = k;
        bucket[k] = bk + 1;

        for i in 0..type_array.len() {
            let mut j = suffix_array[i];
            if j < usize::max_value() && j != shift {
                j = if j == 0 {
                    type_array.len()
                } else {
                    j
                } - 1;
                if !type_array.get(j) {
                    let bj = bucket[j];
                    suffix_array[bj] = j;
                    bucket[j] = bj + 1;
                }
            }
        }
    }
    // compute SAs
    {
        let mut bucket = bucket_builder.build(true);
        for i in (0..type_array.len()).rev() {
            let mut j = suffix_array[i];
            if j < usize::max_value() && j != shift {
                j = if j == 0 {
                    type_array.len()
                } else {
                    j
                } - 1;
                if type_array.get(j) {
                    let bj = bucket[j] - 1;
                    bucket[j] = bj;
                    suffix_array[bj] = j;
                }
            }
        }
    }
}

// find the suffix array SA of s[0..n-1] in {1..K}ˆn
// require s[n-1]=0 (the sentinel!), n>=2
// use a working space (excluding s and SA) of
// at most 2.25n+O(1) for a constant alphabet
fn sa_is<T: Copy + PartialEq<T> + PartialOrd<T>>(
    array: &[T],
    suffix_array: &mut [usize],
    bucket_min: usize,
    bucket_max: usize,
    shift: usize,
) where
    usize: From<T>,
{
    let count = array.len();
    let type_array = LSTypeArray::with_shift(array, shift);

    // stage 1: reduce the problem by at least 1/2
    // sort all the S-substrings
    // bucket array
    let bucket_builder = BucketBuilder::new(array, bucket_min, bucket_max);
    let mut bucket = bucket_builder.build(true);

    // find ends of buckets
    fill::<usize>(suffix_array, 0, count, usize::max_value());

    for i in ((shift + 1)..count).chain(0..shift) {
        if type_array.is_lms(i) {
            let bi = bucket[i] - 1;
            bucket[i] = bi;
            suffix_array[bi] = i;
        }
    }
    induce_sa(
        &bucket_builder,
        &type_array,
        suffix_array,
        shift,
    );

    // compact all the sorted substrings into
    // the first n1 items of SA
    // 2*n1 must be not larger than n (proveable)
    let mut n1 = 0;
    for i in 0..count {
        if type_array.is_lms(suffix_array[i]) {
            suffix_array[n1] = suffix_array[i];
            n1 += 1;
        }
    }

    // find the lexicographic names of substrings
    // init the name array buffer
    fill::<usize>(suffix_array, n1, count - n1, usize::max_value());
    let mut name = 0;
    let mut prev_store = usize::max_value();

    for i in 0..n1 {
        let mut prev = prev_store;
        let mut pos = suffix_array[i];
        let mut now = pos;
        let mut diff = false;
        loop {
            if prev == usize::max_value() || now == shift || prev == shift
                || array[now] != array[prev]
                || type_array.get(now) != type_array.get(prev)
            {
                diff = true;
                break;
            } else if now != pos
                && (type_array.is_lms(now) || type_array.is_lms(prev))
            {
                break;
            }

            now = if now == count - 1 {
                0
            } else {
                now + 1
            };
            prev = if prev == count - 1 {
                0
            } else {
                prev + 1
            };
        }
        if diff {
            name += 1;
            prev_store = pos;
        }
        pos = (if pos > shift {
            pos - shift
        } else {
            pos + count - shift
        }) >> 1;
        suffix_array[n1 + pos] = name - 1;
    }
    {
        let mut j = count - 1;
        for i in (n1..=j).rev() {
            if suffix_array[i] < usize::max_value() {
                suffix_array[j] = suffix_array[i];
                j -= 1;
            }
        }
    }

    // stage 2: solve the reduced problem
    // recurse if names are not yet unique
    let s1 = unsafe {
        slice::from_raw_parts_mut(
            suffix_array
                .as_mut_ptr()
                .add(count - n1),
            n1,
        )
    };
    if name < n1 {
        sa_is::<usize>(s1, suffix_array, 0, name - 1, 0);
    } else {
        // generate the suffix array of s1 directly;
        for (i, &s) in s1.iter().enumerate() {
            suffix_array[s] = i;
        }
    }

    // stage 3: induce the result for
    // the original problem
    // bucket array
    // put all the LMS characters into their buckets
    // find ends of buckets
    let mut bucket2 = bucket_builder.build(true);
    {
        let mut j = 0;
        for i in ((shift + 1)..count).chain(0..shift) {
            if type_array.is_lms(i) {
                s1[j] = i; // get p1
                j += 1;
            }
        }
    }

    // get index in s
    for sa in suffix_array.iter_mut().take(n1) {
        *sa = s1[*sa];
    }

    // init SA[n1..n-1]
    fill::<usize>(suffix_array, n1, count - n1, usize::max_value());

    for i in (0..n1).rev() {
        let j = mem::replace(&mut suffix_array[i], usize::max_value());
        let b2j = bucket2[j] - 1;
        bucket2[j] = b2j;
        suffix_array[b2j] = j;
    }

    induce_sa(
        &bucket_builder,
        &type_array,
        suffix_array,
        shift,
    );
}

pub fn bwt(array: &[u8], max_value: usize) -> Vec<usize> {
    let mut suffix_array = vec![0_usize; array.len()];
    let shift =
        array_rotate_for_non_sentinel_bwt(array, &mut suffix_array, max_value);
    sa_is(array, &mut suffix_array, 0, max_value, shift);
    suffix_array
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::u8;

    fn test_bwt(src: &[u8], bwtstr: &[u8]) {
        let ret = bwt(src, u8::max_value() as usize);
        let mut bwt_ret = vec![0_u8; src.len()];
        for i in 0..bwt_ret.len() {
            let j = if ret[i] == 0 {
                bwt_ret.len()
            } else {
                ret[i]
            } - 1;
            bwt_ret[i] = src[j];
        }
        assert_eq!(bwt_ret, bwtstr);
    }

    fn test_bwtpos(src: &[u8], bwtpos: &[usize]) {
        let ret = bwt(src, u8::max_value() as usize);
        assert_eq!(ret, bwtpos);
    }

    #[test]
    fn test_bwt1() {
        test_bwt(
            b"The quick brown fox jumps over the black lazy dog",
            b"ekynxksergll  ia hhv otTu ccb uwd rfm ebp qjoooza",
            //e black lazy dog
            //k brown fox jumps over the black lazy dog
            //y dog
            //n fox jumps over the black lazy dog
            //x jumps over the black lazy dog
            //k lazy dog
            //s over the black lazy dog
            //e quick brown fox jumps over the black lazy dog
            //r the black lazy dog
            //g
            //lack lazy dog
            //lazy dog
            // brown fox jumps over the black lazy dog
            // black lazy dog"
            //ick brown fox jumps over the black lazy dog
            //ack lazy dog
            // dog
            //he black lazy dog
            //he quick brown fox jumps over the black lazy dog
            //ver the black lazy dog
        );
    }

    #[test]
    fn test_bwt2() {
        test_bwt(
            b"abracadabra0AbRa4Cad14abra",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
        test_bwt(
            b"bracadabra0AbRa4Cad14abraa",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
        test_bwt(
            b"racadabra0AbRa4Cad14abraab",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
        test_bwt(
            b"acadabra0AbRa4Cad14abraabr",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
        test_bwt(
            b"cadabra0AbRa4Cad14abraabra",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
        test_bwt(
            b"adabra0AbRa4Cad14abraabrac",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
        test_bwt(
            b"dabra0AbRa4Cad14abraabraca",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
        test_bwt(
            b"abra0AbRa4Cad14abraabracad",
            b"ada104brRrd4arCcAaaaaaabbb",
        );
    }

    #[test]
    fn test_bwt3() {
        test_bwt(b"mmiissiissiippii", b"pssmiiiimipissii");
    }

    #[test]
    fn test_bwt4() {
        test_bwt(
            b"mmiissiissiippiimmiissiissiippii",
            b"ppssssmmiiiiiiiimmiippiissssiiii",
        );
    }

    #[test]
    fn test_bwt5() {
        // aiipibipi 7
        // bipiaiipi 3
        // iaiipibip 6
        // ibipiaiip 2
        // iipibipia 8
        // ipiaiipib 4
        // ipibipiai 0
        // piaiipibi 5
        // pibipiaii 1

        test_bwt(b"ipibipiai", b"iippabiii");
        test_bwtpos(b"ipibipiai", &[7, 3, 6, 2, 8, 4, 0, 5, 1]);
        test_bwtpos(b"pibipiaii", &[6, 2, 5, 1, 7, 3, 8, 4, 0]);
        test_bwtpos(b"ibipiaiip", &[5, 1, 4, 0, 6, 2, 7, 3, 8]);
        test_bwtpos(b"bipiaiipi", &[4, 0, 3, 8, 5, 1, 6, 2, 7]);
        test_bwtpos(b"ipiaiipib", &[3, 8, 2, 7, 4, 0, 5, 1, 6]);
        test_bwtpos(b"piaiipibi", &[2, 7, 1, 6, 3, 8, 4, 0, 5]);
        test_bwtpos(b"iaiipibip", &[1, 6, 0, 5, 2, 7, 3, 8, 4]);
        test_bwtpos(b"aiipibipi", &[0, 5, 8, 4, 1, 6, 2, 7, 3]);
        test_bwtpos(b"iipibipia", &[8, 4, 7, 3, 0, 5, 1, 6, 2]);
    }

    #[test]
    fn test_bwt6() {
        // abacaiaiaiai 4
        // acaiaiaiaiab 6
        // aiabacaiaiai 2
        // aiaiabacaiai 0
        // aiaiaiabacai 10
        // aiaiaiaiabac 8
        // bacaiaiaiaia 5
        // caiaiaiaiaba 7
        // iabacaiaiaia 3
        // iaiabacaiaia 1
        // iaiaiabacaia 11
        // iaiaiaiabaca 9
        test_bwt(b"aiaiabacaiai", b"ibiiicaaaaaa");
        test_bwtpos(
            b"aiaiabacaiai",
            &[4, 6, 2, 0, 10, 8, 5, 7, 3, 1, 11, 9],
        );
        test_bwtpos(
            b"iaiabacaiaia",
            &[3, 5, 1, 11, 9, 7, 4, 6, 2, 0, 10, 8],
        );
        test_bwtpos(
            b"aiabacaiaiai",
            &[2, 4, 0, 10, 8, 6, 3, 5, 1, 11, 9, 7],
        );
        test_bwtpos(
            b"iabacaiaiaia",
            &[1, 3, 11, 9, 7, 5, 2, 4, 0, 10, 8, 6],
        );
        test_bwtpos(
            b"abacaiaiaiai",
            &[0, 2, 10, 8, 6, 4, 1, 3, 11, 9, 7, 5],
        );
        test_bwtpos(
            b"bacaiaiaiaia",
            &[11, 1, 9, 7, 5, 3, 0, 2, 10, 8, 6, 4],
        );
        test_bwtpos(
            b"acaiaiaiaiab",
            &[10, 0, 8, 6, 4, 2, 11, 1, 9, 7, 5, 3],
        );
        test_bwtpos(
            b"caiaiaiaiaba",
            &[9, 11, 7, 5, 3, 1, 10, 0, 8, 6, 4, 2],
        );
        test_bwtpos(
            b"aiaiaiaiabac",
            &[8, 10, 6, 4, 2, 0, 9, 11, 7, 5, 3, 1],
        );
        test_bwtpos(
            b"iaiaiaiabaca",
            &[7, 9, 5, 3, 1, 11, 8, 10, 6, 4, 2, 0],
        );
        test_bwtpos(
            b"aiaiaiabacai",
            &[6, 8, 4, 2, 0, 10, 7, 9, 5, 3, 1, 11],
        );
        test_bwtpos(
            b"iaiaiabacaia",
            &[5, 7, 3, 1, 11, 9, 6, 8, 4, 2, 0, 10],
        );
    }

    #[test]
    fn test_bwt7() {
        // aibiaibiaiciaici 13
        // aibiaiciaiciaibi 1
        // aiciaibiaibiaici 9
        // aiciaiciaibiaibi 5
        // biaibiaiciaiciai 15
        // biaiciaiciaibiai 3
        // ciaibiaibiaiciai 11
        // ciaiciaibiaibiai 7
        // iaibiaibiaiciaic 12
        // iaibiaiciaiciaib 0
        // iaiciaibiaibiaic 8
        // iaiciaiciaibiaib 4
        // ibiaibiaiciaicia 14
        // ibiaiciaiciaibia 2
        // iciaibiaibiaicia 10
        // iciaiciaibiaibia 6
        test_bwt(b"iaibiaiciaiciaib", b"iiiiiiiicbcbaaaa");
        test_bwtpos(
            b"iaibiaiciaiciaib",
            &[
                13, 1, 9, 5, 15, 3, 11, 7, 12, 0, 8, 4, 14, 2, 10, 6
            ],
        );
    }

    #[test]
    fn test_bwt8() {
        // 15 acacbdafacacbdag
        //  7 acacbdagacacbdaf
        //  1 acbdafacacbdagac
        //  9 acbdagacacbdafac
        //  5 afacacbdagacacbd
        // 13 agacacbdafacacbd
        //  3 bdafacacbdagacac
        // 11 bdagacacbdafacac
        //  0 cacbdafacacbdaga
        //  8 cacbdagacacbdafa
        //  2 cbdafacacbdagaca
        // 10 cbdagacacbdafaca
        //  4 dafacacbdagacacb
        // 12 dagacacbdafacacb
        //  6 facacbdagacacbda
        // 14 gacacbdafacacbda
        test_bwt(b"cacbdafacacbdaga", b"gfccddccaaaabbaa");
        test_bwtpos(
            b"cacbdafacacbdaga",
            &[
                15, 7, 1, 9, 5, 13, 3, 11, 0, 8, 2, 10, 4, 12, 6, 14
            ],
        );
        test_bwtpos(
            b"acbdafacacbdagac",
            &[
                14, 6, 0, 8, 4, 12, 2, 10, 15, 7, 1, 9, 3, 11, 5, 13
            ],
        );
        test_bwtpos(
            b"cbdafacacbdagaca",
            &[
                13, 5, 15, 7, 3, 11, 1, 9, 14, 6, 0, 8, 2, 10, 4, 12
            ],
        );
        test_bwtpos(
            b"bdafacacbdagacac",
            &[
                12, 4, 14, 6, 2, 10, 0, 8, 13, 5, 15, 7, 1, 9, 3, 11
            ],
        );
        test_bwtpos(
            b"dafacacbdagacacb",
            &[
                11, 3, 13, 5, 1, 9, 15, 7, 12, 4, 14, 6, 0, 8, 2, 10
            ],
        );
        test_bwtpos(
            b"afacacbdagacacbd",
            &[
                10, 2, 12, 4, 0, 8, 14, 6, 11, 3, 13, 5, 15, 7, 1, 9
            ],
        );
        test_bwtpos(
            b"facacbdagacacbda",
            &[
                9, 1, 11, 3, 15, 7, 13, 5, 10, 2, 12, 4, 14, 6, 0, 8
            ],
        );
        test_bwtpos(
            b"acacbdagacacbdaf",
            &[
                8, 0, 10, 2, 14, 6, 12, 4, 9, 1, 11, 3, 13, 5, 15, 7
            ],
        );
        test_bwtpos(
            b"cacbdagacacbdafa",
            &[
                7, 15, 9, 1, 13, 5, 11, 3, 8, 0, 10, 2, 12, 4, 14, 6
            ],
        );
        test_bwtpos(
            b"acbdagacacbdafac",
            &[
                6, 14, 8, 0, 12, 4, 10, 2, 7, 15, 9, 1, 11, 3, 13, 5
            ],
        );
        test_bwtpos(
            b"cbdagacacbdafaca",
            &[
                5, 13, 7, 15, 11, 3, 9, 1, 6, 14, 8, 0, 10, 2, 12, 4
            ],
        );
        test_bwtpos(
            b"bdagacacbdafacac",
            &[
                4, 12, 6, 14, 10, 2, 8, 0, 5, 13, 7, 15, 9, 1, 11, 3
            ],
        );
        test_bwtpos(
            b"dagacacbdafacacb",
            &[
                3, 11, 5, 13, 9, 1, 7, 15, 4, 12, 6, 14, 8, 0, 10, 2
            ],
        );
        test_bwtpos(
            b"agacacbdafacacbd",
            &[
                2, 10, 4, 12, 8, 0, 6, 14, 3, 11, 5, 13, 7, 15, 9, 1
            ],
        );
        test_bwtpos(
            b"gacacbdafacacbda",
            &[
                1, 9, 3, 11, 7, 15, 5, 13, 2, 10, 4, 12, 6, 14, 8, 0
            ],
        );
        test_bwtpos(
            b"acacbdafacacbdag",
            &[
                0, 8, 2, 10, 6, 14, 4, 12, 1, 9, 3, 11, 5, 13, 7, 15
            ],
        );
    }

    #[test]
    fn test_bwt9() {
        test_bwtpos(&[0, 0, 1], &[0, 1, 2]);
    }

    #[test]
    fn test_bwt10() {
        test_bwtpos(&[1, 0, 0, 0], &[1, 2, 3, 0]);
    }

    #[test]
    fn test_bwt11() {
        //  0 2324122044142414
        //           @
        //    SLSLSLLSLLSLSLSL
        //    @ @ @     @ @ @
        //    0123456789012345

        //  7 0441424142324122
        //  4 1220441424142324
        // 14 1423241220441424
        // 10 1424142324122044
        //  6 2044142414232412
        //  5 2204414241423241
        //  0 2324122044142414
        //  2 2412204414241423
        // 12 2414232412204414
        //  1 3241220441424142
        //  3 4122044142414232
        // 13 4142324122044142
        //  9 4142414232412204
        // 15 4232412204414241
        // 11 4241423241220441
        //  8 4414241423241220
        test_bwtpos(
            &[2, 3, 2, 4, 1, 2, 2, 0, 4, 4, 1, 4, 2, 4, 1, 4],
            &[
                7, 4, 14, 10, 6, 5, 0, 2, 12, 1, 3, 13, 9, 15, 11, 8
            ],
        );
    }

    #[test]
    fn test_bwt12() {
        test_bwtpos(
            b"2324122044142414",
            &[
                7, 4, 14, 10, 6, 5, 0, 2, 12, 1, 3, 13, 9, 15, 11, 8
            ],
        );
    }
}
