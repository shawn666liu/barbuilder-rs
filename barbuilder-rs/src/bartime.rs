use chrono::{Duration, NaiveTime};
use std::{collections::BTreeMap, fmt::Display};

use tradesession::{SessionSlice, ShiftedTime, TradeSession};

/// 多加4小时的解释: 按照常规计算，夜里23:59:59.500最大，对应的index也是最大，零点开始begin最小，index为零，
/// 但实际情况，跨日就不好处理了，不是连续递增的，所以我们将夜盘20:00:00开始，都增加4小时，进行比较的时候，就连续了
/// (virtual_beginc, virtual_end] 左开右闭区间，即边界时间点属于前一个bar,
/// 比如5分K线，9:05:00属于前一个bar，9:05:00.500则属于后一个bar，注意后一种情况，
/// 直接取num_seconds_from_midnight()会少1秒而属于前一个Bar，必须加上999毫秒再取num_seconds_from_midnight()

/// 具体bar的开始结束时间（秒）及其他属性
/// 虽然BarTime里面没有保存barsize属性，但实际每一个BarTime必然已经知道barsize了
#[derive(Clone)]
pub struct BarTime {
    /// 虚拟的开始时间(进行了增加4小时的处理)，比如早盘第一个5分K含集合竞价，开始时间为8:59:00+4hours
    ///
    /// 如果是Session的开始时间，提前1秒，比如下午开始时间，13:29:29+4hours
    pub virtual_begin: ShiftedTime,

    /// 虚拟的结束时间(进行了增加4小时的处理)
    ///
    /// 对于30分钟K线，10:00~10:30，本该结束于10:30，但实际为10:15，因为盘中休息,
    /// 如果是Session的结束时间，延后1秒，比如下午结束时间，15:00:01+4hours
    pub virtual_end: ShiftedTime,

    /// 名义开始时间(NaiveTime, 没有增加4小时)，不会超过一天,(因为bar不会从24:00:00开始，只会从0:00:00开始)
    pub nominal_begin: NaiveTime,

    /// 从名义开始时间到实际结束的时间长度,作用，已知bar_begin,计算实际的bar_end,
    /// 对于30分钟K线，10:00到10:30，本该结束于10:30，但实际为10:15，因为盘中休息,
    /// 所以nominal_begin为10:00的30分K，duration是15分钟，而不是30分钟，
    /// 国债15:00到15:30的30分K，15:00到16:00的小时K，实际都是15分钟
    pub duration: chrono::Duration,

    /// 以下三个变量暂未使用，还未考虑好

    /// 开始时间是集合竞价
    pub with_auction: bool,

    /// 开始时间是否是一个slice的开始, 比如9:00, 10:30, 13:00, 21:00
    pub with_slice_begin: bool,

    /// 结束时间是否是一个slice的结束, 比如10:15, 11:30, 15:00, 15:15
    pub with_slice_end: bool,
}

impl Default for BarTime {
    fn default() -> Self {
        let tmp = NaiveTime::from_hms_opt(21, 0, 0).expect("no fail");
        Self {
            virtual_begin: ShiftedTime::from(&tmp),
            virtual_end: ShiftedTime::from(&tmp),
            nominal_begin: tmp,
            duration: Duration::seconds(0),
            with_auction: false,
            with_slice_begin: false,
            with_slice_end: false,
        }
    }
}

impl Display for BarTime {
    /// "  虚拟始末时间,  实际时间,         名义开始时间,集合竞价,片段开始,片段结束"
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let t1: NaiveTime = self.virtual_begin.into();
        let t2: NaiveTime = self.virtual_end.into();
        write!(
            f,
            "({}~{}), ({}~{}), {}, {:>5}, {:>5}, {:>5}",
            self.virtual_begin.0,
            self.virtual_end.0,
            t1,
            t2,
            self.nominal_begin,
            self.with_auction,
            self.with_slice_begin,
            self.with_slice_end,
        )
    }
}

impl BarTime {
    /// 内部调用，session_begin，session_end，day_begin都加了4小时，
    /// 由于目前的barsz有1、3、5、10、15、30、60，对于夜盘21开始后的3小时，都是可以整除的，
    /// 从21:00（虚拟的是1:00）与从零点开始计数是一样的，不会有不整除的地方，
    fn build_from_slice(index: usize, slice: &SessionSlice, barsz_sec: u32) -> Vec<BarTime> {
        // 为什么使用nominal时间作为起点? 时间段
        // 比如09:00~10:15, 10:30~11:30
        // 比如21:00~23:00, 21:00~02:30
        let session_begin = slice.begin().seconds();
        let session_end = slice.end().seconds();

        // 对于小时K线来说, 如果session_begin是10:30, 那么aligned_start是10:00
        let ratio = session_begin / barsz_sec;
        let aligned_start = ratio * barsz_sec;
        debug_assert!(aligned_start <= session_begin);

        let ratio = session_end / barsz_sec;
        let mut aligned_end = ratio * barsz_sec;
        debug_assert!(aligned_end <= session_end);
        if aligned_end < session_end {
            // session_end还未全部被aligned_end包含，多加一个周期
            aligned_end += barsz_sec;
            // 多加一个周期后，有可能超出session_end，
            // 比如小时K在11:00~11:30，而不是加满1小时成为11:00~12:00
            // 后面会做判断限制
        }

        let last_idx = ((aligned_end - aligned_start) / barsz_sec - 1) as usize;

        let mut result = vec![];
        // 不包含aligned_end
        for (idx, shifted_begin) in (aligned_start..aligned_end)
            .step_by(barsz_sec as usize)
            .into_iter()
            .enumerate()
        {
            // 名义时间不进行加一天的判断和处理
            let mut start = shifted_begin;
            let mut end = start + barsz_sec;
            let mut with_slice_begin = false;
            let mut with_slice_end = false;
            let mut with_auction = false;

            if idx == 0 {
                with_slice_begin = true;
                // 比如股指期货，9:15开始，如果是小时K，则start为9:00, 做如下调整
                start = std::cmp::max(start, session_begin);
                // 如果是集合竞价时间，则回退1分钟零1秒，以便可以包括集合竞价
                // 对于夜盘商品21:00竞价，非夜盘9:00竞价，
                // 即使该时段没有集合竞价，包括这个61秒也没有问题，因为没有数据推送
                // 所以我们把21:00和9:00都加入，9:00 shift后(9+4)*3600 = 46800
                if index == 0 || session_begin == 3600 || session_begin == 46800 {
                    // index==0 可以覆盖金融期货9:30的集合竞价
                    start -= 61;
                    with_auction = true;
                } else {
                    // 每个Session的开始,我们提前获取1秒钟, 以防有数据丢失
                    start -= 1;
                }
            }
            if idx == last_idx {
                with_slice_end = true;
                // 限制end不能超过session_end点
                // 比如小时K,11:00~12:00点, 但实际时间为11:00~11:30, 此时上午交易已全部结束
                end = std::cmp::min(end, session_end);

                // 每个Session的结束,我们多获取1s,
                // 经观察，上海品种，会在结束后500ms, 重复推一个vol为零的tick，可用于触发bar结束，
                // 比如23:00:500, 15:00:500等时间点
                // 因为需要计算duration, 所以最后去加
                // end += 1;

                // 金融交易所，在15:15:00.400, 有zero_vol推送
                // 金融交易所，在11:30:00.400, 有非零vol推送?

                // 由于这是此slice的最后一个bar, 即使立即推送，实盘也无法进行交易了，所以延迟1秒可以接受
            }
            // duration里面没有加1秒或者减1/61秒的操作
            let duration = Duration::seconds(end as i64 - shifted_begin as i64);
            let nominal = ShiftedTime(shifted_begin);
            if with_slice_end {
                // session结束，加1秒
                end += 1;
            }
            let btm = BarTime {
                virtual_begin: ShiftedTime(start),
                virtual_end: ShiftedTime(end),
                nominal_begin: nominal.into(),
                duration,
                with_slice_begin,
                with_slice_end,
                with_auction,
            };
            result.push(btm);
        }
        result
    }

    /// 根据TradeSession和barsz_sec生成对应的BarTime列表
    pub fn vec_from_session(session: &TradeSession, barsz_sec: u32) -> Vec<BarTime> {
        debug_assert!(barsz_sec > 0 && barsz_sec < 86400);

        // key是BarTime.nominal_begin, 用来捕获可能重复的大周期开始时间
        let mut temp_map: BTreeMap<NaiveTime, BarTime> = BTreeMap::new();

        // 注意： 所有数值比实际时间多4小时, 零点对应时间实际为20:00:00点
        // 如果以20:00:00为零开始进行分割，一些非标准barsz比如7分钟等，可能无法对齐到实际零点
        // 所以还原到实际零点处理后再恢复,(并没有实现)
        let slices = session.get_slices();
        for (index, slice) in slices.iter().enumerate() {
            // println!("--------------- {} ---", slice);

            let tempvec = BarTime::build_from_slice(index, slice, barsz_sec);
            // 这里采用BTree去重，解决一个小问题，
            // 比如一小时的K,会在两段Session都生成，开始时间相同
            // 1) (10:00:00~10:15:00) -> begin 10:00:00
            // 2) (10:30:00~11:00:00) -> begin 10:00:00
            // 正确的做法是将两段合并，取最大的部分
            // 注意: duration的计算方式
            for bt in tempvec {
                if let Some(old) = temp_map.get(&bt.nominal_begin) {
                    // (时间靠前的一个, 时间靠后的一个)
                    let (small, big) = if old.virtual_begin < bt.virtual_begin {
                        (old, &bt)
                    } else {
                        (&bt, old)
                    };
                    let item = BarTime {
                        virtual_begin: small.virtual_begin,
                        virtual_end: big.virtual_end,
                        nominal_begin: bt.nominal_begin,
                        duration: big.nominal_begin - small.nominal_begin + big.duration,
                        with_slice_begin: small.with_slice_begin,
                        with_slice_end: big.with_slice_end,
                        with_auction: small.with_auction,
                    };
                    temp_map.insert(bt.nominal_begin, item);
                } else {
                    temp_map.insert(bt.nominal_begin, bt);
                }
            }
        }

        let mut vec: Vec<BarTime> = temp_map.into_iter().map(|(_, v)| v).collect();
        // 重新排序，必须使用real_begin_sec从小到大
        vec.sort_by(|t1, t2| t1.virtual_begin.cmp(&t2.virtual_begin));
        return vec;
    }
    ///
    pub fn header_for_print() -> &'static str {
        return "       虚拟始末时间,     实际时间,     名义开始时间, 竞价?, 开始?, 结束?";
    }
}
