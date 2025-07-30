#![allow(unused_assignments)]

use std::cmp::Ordering;

/// 简单的二分法查找, list必须已经从小到大排序
/// 返回(left, mid, right) 三个索引, 负数为无效值  
/// 如果有连续相同值的话，返回第一个
pub fn simple_bisect_ord<T: Ord>(list: &[T], target: &T) -> (isize, isize, isize) {
    return simple_bisect(list, target, &|t1, t2| -> Ordering { t1.cmp(t2) });
}

/// 简单的二分法查找, list必须已经从小到大排序
/// 返回(left, mid, right) 三个索引, 负数为无效值  
/// 如果有连续相同值的话，返回第一个
pub fn simple_bisect<T>(
    list: &[T],
    target: &T,
    cmp_fn: &dyn Fn(&T, &T) -> Ordering,
) -> (isize, isize, isize) {
    if list.is_empty() {
        return (-1, -1, -1);
    }

    let mut found = false;
    let mut first = 0_usize;
    let mut mid = 0_usize;
    let mut len = list.len();
    let mut half = 0_usize;
    while len > 0 {
        half = len >> 1;
        mid = first + half;
        let mid_item = &list[mid as usize];
        match cmp_fn(mid_item, target) {
            Ordering::Less => {
                first = mid + 1;
                len = len - half - 1;
            }
            Ordering::Equal => {
                len = half;
                found = true;
            }
            _ => len = half,
        }
    }
    let mut _mid = first as isize;
    let mut _left = _mid - 1;
    let mut _right = -1_isize;
    match found {
        true => {
            if _mid < list.len() as isize - 1 {
                _right = _mid + 1;
            }
        }
        false => {
            if _mid < list.len() as isize {
                _right = _mid;
            }
            _mid = -1;
        }
    }
    return (_left, _mid, _right);
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn bisect_ord() {
        let v = [-8, -6, -4, 0, 7, 7, 7, 7, 7, 9, 23];
        let (l, m, r) = simple_bisect_ord(&v, &-9);
        assert!(l == -1 && m == -1 && r == 0);

        let (l, m, r) = simple_bisect_ord(&v, &-4);
        assert!(l == 1 && m == 2 && r == 3);

        let (l, m, r) = simple_bisect_ord(&v, &28);
        assert!(l == 10 && m == -1 && r == -1);

        let (l, m, r) = simple_bisect_ord(&v, &-5);
        assert!(l == 1 && m == -1 && r == 2);

        let (l, m, r) = simple_bisect_ord(&v, &7);
        assert!(l == 3 && m == 4 && r == 5);
    }
}
