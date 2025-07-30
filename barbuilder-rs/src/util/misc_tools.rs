use std::borrow::Cow;
use std::error::Error;

/// 移除首个出现的数字及其后所有字符,如果首字母是数字则从其后首个非数字之后开始移除  
///
/// IF1603 => IF,  
///
/// y1605 => y,  
///
/// 黄大豆1号1501 => 黄大豆,  
///
/// 10年国债2003 => 10年国债,  
///
/// 1605 => 1605,
///
/// IF => IF
///
/// "" => ""
pub fn trim_num_and_after(input: &str) -> &str {
    let mut any_non_number = false;
    let buf = input.as_bytes();
    for (i, c) in buf.iter().enumerate() {
        if !c.is_ascii_digit() {
            any_non_number = true;
            continue;
        }
        if i > 0 && any_non_number {
            return &input[0..i];
        }
    }
    return input;
}

/// 获取尾部的数字
///
/// IF1905 => 1905
///
/// IF1905A => ""
///
/// 黄大豆1号1501 => 1501
pub fn get_tail_numbers(input: &str) -> &str {
    let buf = input.as_bytes();
    for (i, c) in buf.iter().rev().enumerate() {
        if !c.is_ascii_digit() {
            if i == 0 {
                return "";
            }
            return &input[buf.len() - i..];
        }
    }
    return input;
}

/// 根据合约名及最后交易日期所在的年份, 获取该合约名代表的月份(日期为1号)
/// expire_year在这里的作用是提供年份指引, 因为合约名没有年份的前两位数字
/// 注意, 这里要求instrument已经是4位数字, 如果不是请先调用fix_czce_inst()
/// 返回值为(year, month)
pub fn get_inst_month(instrument: &str, expire_year: i16) -> (i16, i16) {
    // 一般情况下,合约月份与最后交易日在同一个月, 比如sc2205的交割月为22年5月;
    // 但是因元旦春节等影响, 也有例外, 比如 sc2002的最后交易日为20年1月16日,并不在2月份,

    // 那么会不会有合约名为3001, 而最后交易日在2029-12-xx的情况呢?
    // 这种情况直接取2029的前两位补充到3001, 成为2030-01是没有问题的

    // 但是0001最后交易日在1999-12-xx, 简单补充则为1900-01-01

    // sc0001 => 0001
    let inst_no_prd: i16 = get_tail_numbers(instrument).parse().unwrap();
    let month = (inst_no_prd % 100 + 11) % 12 + 1;

    // 若expire_year = 1999, 则year = 1900
    let mut year = expire_year / 100 * 100 + inst_no_prd / 100;
    if expire_year > year {
        year += 100;
    }
    return (year, month);
}

/// 将郑州合约转为4位数字  
///
/// CF001 => CF2001  
///
/// tick_cf001 => tick_cf2001  
pub fn fix_czce_inst(czce_inst: &str, this_year: i16) -> Cow<str> {
    // 计算末尾的连续数字
    let mut count = 0;
    for c in czce_inst.chars().rev() {
        match c.is_ascii_digit() {
            true => count += 1,
            _ => break,
        }
    }
    // 如果末尾已经有4位数字，或者不足3位数字，则什么也不做
    if count >= 4 || count < 3 {
        return Cow::Borrowed(czce_inst);
    }

    let chars = czce_inst.chars().collect::<Vec<char>>();

    // 提取倒数第三位数字
    let len = chars.len();
    let _y = chars[len - 3];
    let _y = match _y.to_digit(10) {
        Some(d) => d as i16,
        None => unreachable!("{} to digiit", _y),
    };

    // 2019 -> 2019
    // 2020 -> 2029, 跟this_year有关
    let mut aa = this_year / 10 * 10 + _y;
    let diff = aa - this_year;
    if diff >= 5 {
        aa -= 10;
    } else if diff <= -5 {
        aa += 10;
    }
    // 取年份的十位数字，比如2020 -> 2
    let aa = aa / 10 % 10;
    let c10 = match std::char::from_digit(aa as u32, 10) {
        Some(c) => c,
        None => unreachable!("from_digit {}", aa),
    };
    let result = [&chars[0..len - 3], &[c10], &chars[len - 3..]].concat();
    return Cow::Owned(result.iter().collect::<String>());
}

/// 还原原始的郑州合约，只有三个数字
///
/// TA2109 或者 TA1109 -> TA109
pub fn restore_czce_inst(czce_inst: &str) -> Cow<str> {
    // 计算末尾的连续数字
    let mut count = 0;
    for c in czce_inst.chars().rev() {
        match c.is_ascii_digit() {
            true => count += 1,
            _ => break,
        }
    }
    // 如果末尾不足4位数字，则什么也不做
    if count < 4 {
        return Cow::Borrowed(czce_inst);
    }

    let mut s = czce_inst.to_string();
    s.remove(s.len() - 4);
    return Cow::Owned(s);
}

/// 将text的内容填充到buffer,超过的长度被截断
pub fn set_cstr_from_str_truncate(buffer: &mut [u8], text: &str) {
    for (place, data) in buffer
        .split_last_mut()
        .expect("buffer len 0 in set_cstr_from_str_truncate")
        .1
        .iter_mut()
        .zip(text.as_bytes().iter())
    {
        *place = *data;
    }
    unsafe {
        *buffer.get_unchecked_mut(text.len()) = 0u8;
    }
}

/// 根据price_tick计算小数点后的位数,得到精度
/// 0.02->2, 0.2->1, 5->0
pub fn calc_precision(price_tick: f64) -> u32 {
    let stick = format!("{price_tick}");
    match stick.find('.') {
        Some(pos) => (stick.len() - 1 - pos) as u32,
        None => 0_u32,
    }
}

/// 提取错误的详情
pub fn err_detail(err: &dyn Error) -> String {
    match err.source() {
        Some(e) => format!("{}, {}", err_detail(&e), err),
        None => format!("{}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn _trim_num_and_after() {
        let prd = trim_num_and_after("IF1603");
        println!("{}", prd);
        assert_eq!(prd, "IF");

        let prd = trim_num_and_after("黄大豆1号1501");
        println!("{}", prd);
        assert_eq!(prd, "黄大豆");

        let prd = trim_num_and_after("10年国债2003");
        println!("{}", prd);
        assert_eq!(prd, "10年国债");

        assert_eq!(trim_num_and_after("1605"), "1605");

        assert_eq!(trim_num_and_after("IF"), "IF");

        assert_eq!(trim_num_and_after(""), "");
    }

    #[test]
    fn _get_tail_numbers() {
        let res = get_tail_numbers("IF1603");
        println!("{}", res);
        assert_eq!(res, "1603");

        let res = get_tail_numbers("黄大豆1号1501");
        println!("{}", res);
        assert_eq!(res, "1501");

        let res = get_tail_numbers("IF1905A");
        println!("{}", res);
        assert_eq!(res, "");

        let res = get_tail_numbers("12345");
        println!("{}", res);
        assert_eq!(res, "12345");

        assert_eq!(get_tail_numbers(""), "");
    }

    #[test]
    fn czce_inst() {
        assert_eq!(fix_czce_inst("tick_cf001", 2020), "tick_cf2001");
        assert_eq!(fix_czce_inst("CF001", 2020), "CF2001");
        assert_eq!(fix_czce_inst("CF001", 2024), "CF2001");
        assert_eq!(fix_czce_inst("CF001", 2025), "CF3001");
        assert_eq!(fix_czce_inst("CF2001", 2029), "CF2001");
        assert_eq!(fix_czce_inst("CF901", 2010), "CF0901");
        assert_eq!(fix_czce_inst("CF01", 2020), "CF01");
        assert_eq!(fix_czce_inst("大豆", 1999), "大豆");
        assert_eq!(fix_czce_inst("", 2020), "");

        assert_eq!(restore_czce_inst("CF2001"), "CF001");
        assert_eq!(restore_czce_inst("CF001"), "CF001");
        assert_eq!(restore_czce_inst("1001"), "001"); // bad input
    }
}
