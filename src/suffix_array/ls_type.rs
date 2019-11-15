use crate::bitset::BitArray;

#[derive(Debug)]
pub(crate) struct LSTypeArray {
    bitmap: BitArray,
    is_lms: BitArray,
}

impl LSTypeArray {
    pub(crate) fn with_shift<T: PartialEq<T> + PartialOrd<T>>(
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

    pub(crate) fn get(&self, idx: usize) -> bool {
        self.bitmap.get(idx)
    }

    pub(crate) fn is_lms(&self, idx: usize) -> bool {
        self.is_lms.get(idx)
    }

    pub(crate) fn len(&self) -> usize {
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

        let ls_value = &[
            false, false, true, true, false, false, true, true, false, false,
            true, true, false, false, false, false,
        ];
        let lms_value = &[
            false, false, true, false, false, false, true, false, false, false,
            true, false, false, false, false, false,
        ];

        for i in 0..type_arr.len() {
            assert_eq!(type_arr.get(i), ls_value[i]);
            assert_eq!(type_arr.is_lms(i), lms_value[i]);
        }
    }

    #[test]
    fn is_lms2() {
        let test_str = b"The quick brown fox jumps over the black lazy dog";
        //               SLLSSLLSLSSLSLLSSSLSSLSSLSSLSLSLLLSSLSSLSLSLLSSLLS
        //                  @   @ @  @  @   @  @  @  @ @   @  @  @ @  @   @
        //               01234567890123456789012345678901234567890123456789
        let type_arr = LSTypeArray::with_shift(test_str, 0);
        let ls_value = &[
            true, false, false, true, true, false, false, true, false, true,
            true, false, true, false, false, true, true, true, false, true,
            true, false, true, true, false, true, true, false, true, false,
            true, false, false, false, true, true, false, true, true, false,
            true, false, true, false, false, true, true, false, false,
        ];
        let lms_value = &[
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
