use bitset::BitArray;

#[derive(Debug)]
pub(crate) struct LSTypeArray {
    bitmap: BitArray,
    is_lms: BitArray,
}

impl LSTypeArray {
    pub fn with_shift<T: PartialEq<T> + PartialOrd<T>>(
        array: &[T],
        shift: usize,
    ) -> Self {
        let count = array.len();
        let mut bitmap = BitArray::new(count);
        let start = if shift == 0 { count } else { shift };

        // classify the type of each character
        // the sentinel must be in s1, important!!!
        // bitmap.set(start - 1, false);
        for i in (1..start).rev() {
            let b = bitmap.get(i);
            bitmap.set(
                i - 1,
                if array[i] == array[i - 1] {
                    b
                } else {
                    array[i - 1] < array[i]
                },
            )
        }

        if shift != 0 {
            let b = bitmap.get(0);
            bitmap.set(
                count - 1,
                if array[0] == array[count - 1] {
                    b
                } else {
                    array[count - 1] < array[0]
                },
            );
            for i in ((shift + 1)..count).rev() {
                let b = bitmap.get(i);
                bitmap.set(
                    i - 1,
                    if array[i] == array[i - 1] {
                        b
                    } else {
                        array[i - 1] < array[i]
                    },
                )
            }
        }

        let is_lms = if shift == 0 {
            bitmap
                .iter()
                .scan(true, |old, b| {
                    let ret = Some(b && !*old);
                    *old = b;
                    ret
                })
                .collect::<BitArray>()
        } else {
            let last = bitmap.get(count - 1);
            bitmap
                .iter()
                .enumerate()
                .scan(last, |old, (i, b)| {
                    let ret = Some(i != shift && b && !*old);
                    *old = b;
                    ret
                })
                .collect::<BitArray>()
        };

        Self { bitmap, is_lms }
    }

    pub fn get(&self, idx: usize) -> bool {
        self.bitmap.get(idx)
    }

    pub fn is_lms(&self, idx: usize) -> bool {
        self.is_lms.get(idx)
    }

    pub fn len(&self) -> usize {
        self.bitmap.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_lms() {
        //               LLSSLLSSLLSSLLLLS
        let test_str = b"mmiissiissiippii";
        let type_arr = LSTypeArray::with_shift(test_str, 0);
        assert!(!type_arr.get(0));
        assert!(!type_arr.get(1));
        assert!(type_arr.get(2));
        assert!(type_arr.get(3));
        assert!(!type_arr.get(4));
        assert!(!type_arr.get(5));
        assert!(type_arr.get(6));
        assert!(type_arr.get(7));
        assert!(!type_arr.get(8));
        assert!(!type_arr.get(9));
        assert!(type_arr.get(10));
        assert!(type_arr.get(11));
        assert!(!type_arr.get(12));
        assert!(!type_arr.get(13));
        assert!(!type_arr.get(14));
        assert!(!type_arr.get(15));

        assert!(!type_arr.is_lms(0));
        assert!(!type_arr.is_lms(1));
        assert!(type_arr.is_lms(2));
        assert!(!type_arr.is_lms(3));
        assert!(!type_arr.is_lms(4));
        assert!(!type_arr.is_lms(5));
        assert!(type_arr.is_lms(6));
        assert!(!type_arr.is_lms(7));
        assert!(!type_arr.is_lms(8));
        assert!(!type_arr.is_lms(9));
        assert!(type_arr.is_lms(10));
        assert!(!type_arr.is_lms(11));
        assert!(!type_arr.is_lms(12));
        assert!(!type_arr.is_lms(13));
        assert!(!type_arr.is_lms(14));
        assert!(!type_arr.is_lms(15));
    }

    #[test]
    fn is_lms2() {
        let test_str = b"The quick brown fox jumps over the black lazy dog";
        //               SLLSSLLSLSSLSLLSSSLSSLSSLSSLSLSLLLSSLSSLSLSLLSSLLS
        //                  @   @ @  @  @   @  @  @  @ @   @  @  @ @  @   @
        //               01234567890123456789012345678901234567890123456789
        let type_arr = LSTypeArray::with_shift(test_str, 0);
        let ls_value = &[
            //  0      1      2      3      4
            true, false, false, true, true, false, false, true, false, true,
            true, false, true, false, false, true, true, true, false, true,
            true, false, true, true, false, true, true, false, true, false,
            true, false, false, false, true, true, false, true, true, false,
            true, false, true, false, false, true, true, false, false,
        ];
        let lms_value = &[
            //  0      1      2      3      4
            false, false, false, true, false, false, false, true, false, true,
            false, false, true, false, false, true, false, false, false, true,
            false, false, true, false, false, true, false, false, true, false,
            true, false, false, false, true, false, false, true, false, false,
            true, false, true, false, false, true, false, false, false,
        ];

        for i in 0..type_arr.len() {
            assert_eq!(type_arr.get(i), ls_value[i]);
            assert_eq!(type_arr.is_lms(i), lms_value[i]);
        }
    }

}
