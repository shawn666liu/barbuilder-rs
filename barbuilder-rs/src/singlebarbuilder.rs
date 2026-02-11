use anyhow::{Result, anyhow};
use chrono::Duration;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use std::ops::Add;
use std::ops::Sub;

use crate::BarTime;
use crate::Ticklike;
use crate::data_impl::BarData;
use crate::util::simple_bisect;
use tradesession::ShiftedTime;

pub const FMT_TICK_TIME: &str = "%Y-%m-%d %H:%M:%S%.3f";

/// 最新一个Bar的重要缓存, Option<last_bar>为空时也要保持
#[derive(Clone, Default, Debug)]
struct LastBarCache {
    /// 最近一个bar的结束时间，
    ///
    /// 注意：对于30分钟bar，名义结束时间为10:30, 在10:15时实际已结束，
    /// 那么，这里的last_bar_end的值必须是10:15,
    ///
    /// 因为此时还有1、3、5、15分钟的K线同时完成，必须使用相同的bar_end
    ///
    /// 仅在创建新Bar时修改,否则保持前值，无需每tick更新
    ///
    /// 包括with_slice_end信息
    pub last_bar_end: NaiveDateTime,

    /// last_bar_end在BarTime队列中的索引，
    ///
    /// 仅在创建新Bar时修改,否则保持前值，无需每tick更新
    pub last_bar_index: i32,

    /// 当前bar关闭之前，记录这个数值，如果后续创建zero_vol的bar, 需要这个收盘价
    pub last_bar_close: f64,

    /// 用于生成zero_vol_bar
    pub last_tradeday: NaiveDate,
}

// 注意: 非daily bar
// 仅针对某合约的某一种Barsize进行处理，主要目的是利用空间换时间，
// 提前将一些相关数据缓存，减少tick到来后的实时计算量
pub struct SingleBarBuilder {
    inst: String,

    /// 单位为秒
    barsz_sec: u32,

    /// 参见BarTime各字段说明
    bar_time_vec: Vec<BarTime>,

    /// 最新tick应该存在于此索引(也可能为-1)，需每tick更新
    pub(crate) tick_idx: i32,

    /// 最近的非zero_vol_bar, 一旦关闭，就被take()拿走
    pub(crate) last_bar: Option<BarData>,

    /// 在本次on_tick事件中，关闭的Bar，
    /// 为什么使用Vec而不是单个Barlik，因为中途可能有补充的 Zero Volume Bar ？
    /// 是否可以确保，每一个tick到来之后，最多close一个bar？ 存疑
    pub(crate) opt_closed_this_tick: Option<BarData>,

    /// 如果本周期需要创建zero_vol_bar,放在这里
    pub(crate) zerovol_bar_vec: Vec<BarData>,

    /// 临时变量，用于搜索，避免反复构造销毁
    to_cmp: BarTime,

    /// 是否生成vol为零的bar
    zero_vol_bar: bool,

    last_cache: LastBarCache,
}

impl SingleBarBuilder {
    pub fn new(inst: &str, barsz_sec: u32, bar_time_vec: Vec<BarTime>, zero_vol_bar: bool) -> Self {
        let mut last_cache = LastBarCache::default();
        last_cache.last_bar_index = -1;

        Self {
            inst: inst.to_owned(),
            barsz_sec,
            bar_time_vec,
            tick_idx: -1,
            last_bar: None,
            opt_closed_this_tick: None,
            zerovol_bar_vec: Vec::default(),
            to_cmp: BarTime::default(),
            zero_vol_bar,
            last_cache,
        }
    }

    pub fn barsz_sec(&self) -> u32 {
        self.barsz_sec
    }

    pub fn inst(&self) -> &str {
        &self.inst
    }

    pub fn last_bar_end(&self) -> &NaiveDateTime {
        &self.last_cache.last_bar_end
    }

    pub fn bar_time_vec(&self) -> &Vec<BarTime> {
        &self.bar_time_vec
    }

    /// 注意!!! 必须在on_tick调用之前，任何on_tick调用之后，都不能再设置prebar
    ///
    /// 有prebar的情况，比如实盘中在极短后重启软件时，有保存加载机制，加载了旧Bar,
    /// 而且这个bar的周期比较长，比如30分K,或者小时K,尚未走完，则可以利用上这个prebar
    pub fn set_pre_bar(&mut self, mut bar: BarData) -> Result<()> {
        if self.tick_idx >= 0 || self.last_cache.last_bar_index >= 0 {
            return Err(anyhow!("set_pre_bar() must called before any on_tick()"));
        }

        // 这个virtual_end并不准确，比如10:00开始的30分k线，其实duration只有15分钟(后15分钟休市了)，
        // 而不是30分钟， 所以我们先search end, 不行再search begin
        let begin = bar.begin.time();
        self.to_cmp.virtual_end =
            ShiftedTime::from(begin + Duration::seconds(self.barsz_sec.into()));
        let mut bar_index = Self::search_end_time_index(&self.to_cmp, &self.bar_time_vec);
        if bar_index == -1 {
            self.to_cmp.virtual_begin = ShiftedTime::from(begin);
            bar_index = Self::search_begin_time_index(&self.to_cmp, &self.bar_time_vec);
        }

        // 无法保证外部传入的prebar的internal_end是准确的，重算
        if bar_index >= 0 {
            let bt = &self.bar_time_vec[bar_index as usize];
            let mut bar_end = bar.begin + bt.duration;
            if bt.with_slice_end {
                bar_end += Duration::seconds(1);
            }
            bar.internal_end = bar_end;
            bar.created_this_tick = false;
            self.last_cache.last_bar_end = bar_end;
            self.last_cache.last_bar_close = bar.close;
            self.last_cache.last_tradeday = bar.tradeday;
            self.last_cache.last_bar_index = bar_index;

            // 注意： 由于这个模块可用于历史数据回放，不应该调用now()函数，
            // 所以，finished这个判断，丢给on_tick()或者on_timer()

            // let now = Local::now().naive_local();
            // bar.finished = now >= bar_end;

            log::trace!(
                "SingleBarBuilder::set_pre_bar(), bar_index {:>3}, barsz {:>4}, bar {:?}",
                bar_index,
                self.barsz_sec,
                bar
            );

            // last_bar 与 bar_index 必须是一致的
            self.last_bar = Some(bar);
        }
        Ok(())
    }

    /// 返回值，是否需要生成新Bar
    pub fn check_exists_bar(&mut self, tick: &dyn Ticklike) -> bool {
        // 这里要注意，2025-07-23 00:00:00和2025-07-23 00:00:00.500,
        // 仅相差500ms，但计算出来的ShiftedTime秒数是不一样的，
        // 因为，前者是上一个bar的结束，后者是新一个bar的开始
        self.to_cmp.virtual_end = ShiftedTime::from(tick.datetime().time());
        self.tick_idx = Self::search_end_time_index(&self.to_cmp, &self.bar_time_vec);

        #[cfg(debug_assertions)]
        println!("bs {:>4}, tick_idx {}", self.barsz_sec, self.tick_idx);

        if self.tick_idx < 0 {
            // 下一周期tick数据无效，不在Bar的范围，无需创建新bar
            if let Some(bar) = self.last_bar.take() {
                // 需要关闭当前Bar
                self.emit_bar_close(bar, false, "idx_invalid", Some(tick));
            }
            return false;
        } else {
            // tick_idx >= 0
            if self.last_bar.is_some() {
                return self.check_bar_detail(tick);
            } else {
                // 旧bar已经推送，需要生成新Bar，
                // 判断中间是否需要创建zero_vol_bar
                if self.tick_idx > self.last_cache.last_bar_index {
                    self.tick_advanced(tick);
                }
                return true;
            }
        }
    }

    /// 搜索input.virtual_end(结束时间)在 bar_time_vec中的索引，有可能等于-1，比如午休时间
    /// 这里input.virtual_end实际并不是bar_end, 实际上是最新tick的datetime,
    pub fn search_end_time_index(input: &BarTime, time_vec: &Vec<BarTime>) -> i32 {
        // 二分法优化, 如果落在后边界的话, 算在end里面,所以使用virtual_end进行判断
        // 例如, 10:00:00秒，这个时间戳，是算在前一个bar里面的，后一个bar从10:00:00.500开始计算
        let (_, mid, right) = simple_bisect(time_vec, input, &|t1, t2| {
            return t1.virtual_end.cmp(&t2.virtual_end);
        });
        if mid >= 0 {
            // 时间戳恰好在virtual_end上面的情况
            return mid as i32;
        }
        let tm_sec = &input.virtual_end;
        if right >= 0 {
            // begin,end 左开右闭
            // 有两种情况, 第二种无效
            let bt = &time_vec[right as usize];
            if tm_sec > &bt.virtual_begin {
                // (1)
                //         tm_sec->|  |<-(this is the right)
                //                 V  V
                // (b,e](b,e]__(b,{ },e]
                return right as i32;
            } else {
                // (2)
                //     tm_sec->|     | <-(this is the right)
                //             V     V
                // (b,e](b,e]_{ }_(b,e]
            }
        }
        return -1;
    }

    /// 搜索input.virtual_begin(开始时间)在 bar_time_vec中的索引，有可能等于-1，比如午休时间，
    /// 比如30分K,在早上10:15结束，通过barsz_sec仅能获得10:30这个点，
    /// 所以有时我们必须通过virtual_begin来获取index;
    /// 这里input.virtual_begin一定是bar_begin, 而跟tick的datetime无关
    pub fn search_begin_time_index(input: &BarTime, time_vec: &Vec<BarTime>) -> i32 {
        let (_, mid, _) = simple_bisect(time_vec, input, &|t1, t2| {
            return t1.virtual_begin.cmp(&t2.virtual_begin);
        });
        if mid == 0 {
            // 时间戳恰好在virtual_begin上面
            return mid as i32;
        }
        return -1;
    }

    pub(crate) fn emit_bar_close(
        &mut self,
        mut bar: BarData,
        is_force_close: bool,
        debug_str: &str,
        tick: Option<&dyn Ticklike>,
    ) {
        log::trace!(
            "emit_bar_close: {}({:>4}), begin {}, tick {}, force? {}, {}",
            self.inst,
            self.barsz_sec,
            bar.begin,
            match tick.as_ref() {
                Some(t) => format!("{}", t.datetime().format(FMT_TICK_TIME)),
                None => "".to_owned(),
            },
            is_force_close,
            debug_str,
        );
        bar.finished = true;
        // 记录最后价格，创建zero_vol_bar时需要
        self.last_cache.last_bar_close = bar.close;
        self.opt_closed_this_tick = Some(bar);
    }

    /// 返回值，是否需要生成新Bar  
    /// 单独处理check_exists_bar()中，bar和tick_idx都有效的情况
    fn check_bar_detail(&mut self, tick: &dyn Ticklike) -> bool {
        debug_assert!(self.tick_idx >= 0);
        debug_assert!(self.last_bar.is_some());

        #[cfg(debug_assertions)]
        println!(
            "bs {:>4}, tick_idx {}, bar_idx {}",
            self.barsz_sec, self.tick_idx, self.last_cache.last_bar_index
        );

        if self.tick_idx == self.last_cache.last_bar_index {
            // 情况一： 两者在同一个槽子里面
            // tick:   ... |tick_idx |
            // bar :   ... |bar_index|
            return self.same_slot(tick);
        } else if self.tick_idx > self.last_cache.last_bar_index {
            // 情况二： tick跑到了前面，说明中间部分没有推tick，需要关闭当前bar，并补充中间volume为零的bar
            // tick:   ... | ... ... | ..(1).. | ..(2).. |tick_idx|
            // bar :   ... |bar_index|
            return self.tick_advanced(tick);
        } else {
            //  if (tick_idx < bar_index)

            // 情况三： bar跑到了tick前面，这种情况似乎不应该出现，也不是很好处理，只能等待tick追上来，什么也不做
            // tick:   ... |tick_idx|
            // bar :   ... | ...... | ... | ... |bar_index|
            return false;
        }
    }

    /// 情况一： 两者在同一个槽子里面，利用tick更新bar
    /// tick:   ... |tick_idx|
    /// bar :   ... |bar_index|
    fn same_slot(&mut self, tick: &dyn Ticklike) -> bool {
        debug_assert!(self.last_cache.last_bar_index == self.tick_idx);
        debug_assert!(self.last_bar.is_some());

        let bar = self.last_bar.as_mut().expect("no fail");

        if bar.finished {
            // 确保这不是最后一个bar, 最后一个bar的话，不可能再生成新Bar
            if self.tick_idx as usize >= self.bar_time_vec.len() - 1 {
                return false;
            }
            // 当前处理方式，如果该tick有量，则放入到下一个bar，创建新bar，
            // 如果没有量，则直接丢弃
            if tick.vol_delta() > 0 {
                // bar已经关闭，这里极有可能的情况是，当前bar因超时，已被force_close()函数强制关闭，但后续还有这个bar内的tick推送来
                log::warn!(
                    "tick_recvd_for_finished_bar. {}({}), tick {}, bar begin {}",
                    self.inst,
                    self.barsz_sec,
                    tick.datetime(),
                    bar.begin,
                );
                return true;
            }
            return false;
        }

        bar.high = f64::max(bar.high, tick.last_price());
        bar.low = f64::min(bar.low, tick.last_price());
        bar.close = tick.last_price();
        bar.volume += tick.vol_delta();
        bar.turnover += tick.tnov_delta();
        bar.openint = tick.openint();

        bar.finished = tick.datetime() >= &bar.internal_end;

        if bar.finished {
            let last = self.last_bar.take().expect("no fail");
            self.emit_bar_close(last, false, "idx_equal", Some(tick));
        }
        // 由于当前tick已经用掉了，所以不能再创建 新Bar,必须等下一个tick， 所以 返回false
        return false;
    }

    /// 情况二： tick跑到了前面，说明中间部分没有推tick，需要关闭当前bar，并补充中间volume为零的bar
    /// 或者bar已经推送，tick跑到bar_index前面了，需要补充中间volume为零的bar
    /// tick:   ... | ... ... | ..(1).. | ..(2).. |tick_idx|
    /// bar :   ... |bar_index|
    fn tick_advanced(&mut self, tick: &dyn Ticklike) -> bool {
        debug_assert!(self.tick_idx > self.last_cache.last_bar_index);

        if let Some(bar) = self.last_bar.take() {
            self.emit_bar_close(bar, false, "tick_advanced", Some(tick));
        }

        // 创建volume为零的bar，OHLC则为上一个bar的close， 如上面图示的情况，就需要创建两个bar
        if self.zero_vol_bar {
            let start = self.last_cache.last_bar_index + 1;
            let end_exclude = self.tick_idx;

            // bugfix: 由于比较tick时间时左开右闭(]的操作，对于跨零点的tick,
            // 比如，前一tick 2021-11-08 23:59:59.500, 后一tick 2021-11-09 00:00:00,
            // 它们是在同一个slot里面的，如果newbar.begin直接通过tick.datetime().date()获取，
            // 则后者多了一天，导致bug
            // 解决办法，减去1毫秒之后，再取日期
            let realday = tick.datetime().sub(Duration::milliseconds(1)).date();

            // 不含end_exclude
            self.create_zero_vol_bar_batch(start, end_exclude, &realday, tick.tradeday());
        }

        // 当前tick并未用掉, 返回true
        return true;
    }

    pub fn create_new_bar(&mut self, tick: &dyn Ticklike) {
        debug_assert!(self.tick_idx >= 0);
        // 需要判断前一个bar被强制关闭，但tick属于前一个bar的情况，
        // 这个在same_slot()调用后，由于vol非零，返回值为true

        // 理论上来说，这个tick属于上一个bar, 该bar应该处理完这个tick才关闭
        // 由于可能使用了超时关闭当前bar的操作force_close_2_1_1(), 这个情况是可以出现的
        // 这个force_close的目的，是用于截面策略，需要在某个时间点上的所有bar数据，比如我们希望在10:00之后10秒内，
        // 该品种所有合约比如ru2509,ru2601,...的所有1分钟，3分钟，5分钟，30分钟，60分钟的bar,全部完成关闭
        // 但是由于某个合约推送特别慢，导致在10秒之后才推送到达，就会出现这种情况，
        // bar已经关闭，tick才接收到，
        // 目前的处理，tick_idx+=1,把该tick的价量，计算到下一个bar
        if let Some(bar) = &mut self.last_bar {
            if bar.finished && self.tick_idx == self.last_cache.last_bar_index {
                // 由于上一个bar已经关闭，而此tick对应的时间为上一个bar的时间槽，所以必须移到下一个bar
                self.tick_idx += 1;
                if self.tick_idx as usize >= self.bar_time_vec.len() {
                    self.tick_idx = -1;
                    return;
                }
                log::warn!(
                    "create_new_after_bar_closed. {}, tick {}, bar {}({})",
                    self.inst,
                    tick.datetime(),
                    bar.begin,
                    self.barsz_sec
                );
            }
        }

        let bt = &self.bar_time_vec[self.tick_idx as usize];
        let mut newbar = BarData::new_empty();

        // bugfix: 由于比较tick时间时左开右闭(]的操作，对于跨零点的tick,
        // 比如，前一tick 2021-11-08 23:59:59.500, 后一tick 2021-11-09 00:00:00,
        // 它们是在同一个slot里面的，如果newbar.begin直接通过tick.datetime().date()获取，
        // 则后者多了一天，导致bug
        // 解决办法，减去1毫秒之后，再取日期

        // 时间部分，取bar_time_vec里面的nominal_begin作为开始时间，对于上面的后者，
        // begin time为0点整向前推一个时间周期

        let begin_ = tick
            .datetime()
            .sub(Duration::milliseconds(1))
            .date()
            .and_time(bt.nominal_begin);
        newbar.begin = begin_;
        // 检查一下，这里是否有bug
        #[cfg(debug_assertions)]
        {
            let compare = tick
                .tradeday()
                .add(Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .expect("no fail");
            if begin_ >= compare {
                log::error!(
                    "\n\n\n\n\nnewbar.begin {} is far more than expected!!!!!\n\n\n\n\n",
                    newbar.begin
                );
                panic!(
                    "newbar.begin {} is far more than expected! {}",
                    newbar.begin, compare
                );
            }
        }
        newbar.internal_end = newbar.begin + bt.duration;
        if bt.with_slice_end {
            newbar.internal_end += Duration::seconds(1);
        }
        newbar.open = tick.last_price();
        newbar.high = newbar.open;
        newbar.low = newbar.open;
        newbar.close = newbar.open;
        newbar.openint = tick.openint();
        newbar.tradeday = *tick.tradeday();
        newbar.volume = tick.vol_delta();
        newbar.turnover = tick.tnov_delta();
        // 注意： 缺省是true, 这里需要重置为false
        newbar.finished = false;
        newbar.barsz_sec = self.barsz_sec;

        log::trace!(
            "BarCreated1 {}({:>4}), begin {}; tick {}",
            self.inst,
            self.barsz_sec,
            newbar.begin,
            tick.datetime().format(FMT_TICK_TIME),
        );

        self.last_cache.last_bar_end = newbar.internal_end;
        self.last_cache.last_bar_index = self.tick_idx;
        self.last_cache.last_tradeday = newbar.tradeday;
        self.last_bar = Some(newbar);
    }

    // bar_end_time参数尚未包含with_slice_end信息
    fn create_zero_vol_bar(
        &mut self,
        bar_idx: i32,
        bar_end_time: &NaiveDateTime,
        tradeday: &NaiveDate,
    ) -> BarData {
        let bt: &BarTime = &self.bar_time_vec[bar_idx as usize];
        let mut bar = BarData::new_empty();

        bar.tradeday = *tradeday;
        bar.begin = *bar_end_time - bt.duration;
        bar.internal_end = *bar_end_time;
        if bt.with_slice_end {
            bar.internal_end += Duration::seconds(1);
        }
        bar.finished = true;
        bar.close = self.last_cache.last_bar_close;
        bar.open = bar.close;
        bar.high = bar.close;
        bar.low = bar.close;
        bar.volume = 0;
        bar.turnover = 0.0;
        bar.barsz_sec = self.barsz_sec();

        log::trace!(
            "BarCreated_0_vol: {}({:>4}), end {}, close {}",
            self.inst,
            self.barsz_sec,
            bar_end_time,
            bar.close,
        );

        self.last_cache.last_bar_end = bar.internal_end;
        self.last_cache.last_bar_index = bar_idx;
        self.last_cache.last_tradeday = bar.tradeday;
        return bar;
    }

    /// 批量创建满足条件的zero vol bar, 不含end_idx_exclude
    fn create_zero_vol_bar_batch(
        &mut self,
        start_idx: i32,
        end_idx_exclude: i32,
        realday: &NaiveDate,
        tradeday: &NaiveDate,
    ) {
        for idx in start_idx..end_idx_exclude {
            if let Some(bt) = self.bar_time_vec().get(idx as usize) {
                log::warn!(
                    "need_create_zero_vol_bar, {}, begin {}",
                    self.inst,
                    bt.nominal_begin
                );

                // bugfix: 由于比较tick时间时左开右闭(]的操作，对于跨零点的tick,
                // 比如，前一tick 2021-11-08 23:59:59.500, 后一tick 2021-11-09 00:00:00,
                // 它们是在同一个slot里面的，如果newbar.begin直接通过tick.datetime().date()获取，
                // 则后者多了一天，导致bug
                // 解决办法，减去1毫秒之后，再取日期
                let bar_endtime = realday.and_time(bt.nominal_begin).add(bt.duration);
                let zerovol = self.create_zero_vol_bar(idx, &bar_endtime, tradeday);
                self.zerovol_bar_vec.push(zerovol);
            }
        }
    }

    /// 完成bar结束时间等于force_bar_end的
    pub fn force_finish(&mut self, force_bar_end: &NaiveDateTime) {
        self.on_timer(force_bar_end, Duration::zero());
    }

    /// 若last_bar的结束时间小于(now-threshold), 则关闭该bar,
    /// 这里不调用zerovol_bar_vec.clear和last_bar.take(), 上层调用需要清理这些项  
    /// 非交易日不能调用
    pub fn on_timer(&mut self, now: &NaiveDateTime, threshold: chrono::TimeDelta) {
        let compare_time = *now - threshold;
        if let Some(bar) = &mut self.last_bar {
            if compare_time >= bar.internal_end {
                if let Some(last) = self.last_bar.take() {
                    self.emit_bar_close(last, true, "by_timer", None);
                }
            }
        }

        // 如果last_bar_index==-1,表示当天没有bar也没有tick,则不用处理了
        if self.zero_vol_bar && self.last_cache.last_bar_index >= 0 {
            // 如果last_bar尚未关闭，说明其后不会有zero_vol_bar
            if self.last_bar.is_some() {
                return;
            }

            // 分3种情况
            // 1) timer时间落在某个bar的session内部，
            // 2) timer时间落在非bar时间段，但其后还有bar会生成
            // 3) timer时间落在非bar时间段，其后全天结束，不再有任何bar

            // 1)
            // bar:   ... |last_bar_index| ..(bar1).. | ..(bar2).. |..(bar3).. <-(on_timer here)| ..(bar4).. |
            //              virtual_end->|    start   |            | vend_time->         index->|
            // 这种情况，直接可以算出index非-1,且index>=start, 等于start则表示start所在的这个bar还未走完，什么也不做，
            // 大于start, 则创建 start..index之间的空bar

            // 2) 比如落在午休时间
            // bar:   ... |last_bar_index| ..(bar1).. | ..(bar2).. | ..(lunchtime).. <-(on_timer here)| ..(bar3).. |
            //              virtual_end->|    start   |            |       vend_time->         index->|
            // 这种情况，index=-1

            // 3) 比如落在收盘之后
            // bar:   ... |last_bar_index| ..(bar1).. | ..(bar2).. | ..(mkt closed).. <-(on_timer here)|
            //              virtual_end->|    start   |            |        vend_time->         index->|
            // 这种情况，index=-1

            // 对于2,3的情况，我们并不能识别出来，只知道index=-1, 所以处理办法是一样的，
            // 就是vend_time时间按照逐个bar时长回溯到非空session所在的bar_end, (如上图，就是回溯到bar2)
            // 把中间的zero_vol_bar补上就行了，

            // 如果已经处理完上面图3的bar2，说明全天都结束了，(图2的bar2没有全天结束，下午还有bar)
            if (self.last_cache.last_bar_index + 1) as usize == self.bar_time_vec.len() {
                // 全天结束
                return;
            }

            let mut vend_time = ShiftedTime::from(compare_time.time());
            let bt = &self.bar_time_vec[self.last_cache.last_bar_index as usize];
            let start = self.last_cache.last_bar_index + 1;
            let mut result = -1;
            let mut more_than_last = 0;
            while vend_time > bt.virtual_end {
                self.to_cmp.virtual_end = vend_time;
                let index = Self::search_end_time_index(&self.to_cmp, &self.bar_time_vec);
                if index >= 0 {
                    // found
                    result = index;
                    break;
                } else {
                    // 每次减少一个bar的时间
                    let adj = self.barsz_sec() as i32;
                    vend_time.adjust(-adj);
                    more_than_last = 1;
                }
            }
            println!("result {}, start {}", result, start);
            if result + more_than_last > start {
                // 由于比较tick时间时左开右闭(]的操作，对于跨零点的tick,
                // 比如，前一tick 2021-11-08 23:59:59.500, 后一tick 2021-11-09 00:00:00,
                // 它们是在同一个slot里面的，如果newbar.begin直接通过tick.datetime().date()获取，
                // 则后者多了一天，导致bug
                // 解决办法，减去1毫秒之后，再取日期
                let realday = compare_time.sub(Duration::milliseconds(1)).date();
                let tradeday = self.last_cache.last_tradeday;
                self.create_zero_vol_bar_batch(start, result + more_than_last, &realday, &tradeday);
            }
        }
    }
}
