use anyhow::Result;
use chrono::{Duration, NaiveDateTime};
use std::collections::HashMap;
use tradesession::TradeSession;

use crate::data_impl::BarImpl;
use crate::{BarTime, SingleBarBuilder, Ticklike};

/// 针对某个合约所有时间周期Bar的生成器
pub struct InstBarBuilder {
    inst: String,

    /// 不同的barsize，需要不同的BarBuilder
    /// barsize从小到大排列
    single_bb_map: Vec<SingleBarBuilder>,

    /// 临时变量，本次on_tick中，这些barsize的builder需要检查并创建新bar，每tick清空
    need_create_barsz: Vec<u32>,

    // 最近时间戳,on_tick()里面tick的时间
    // recent_time: NaiveDateTime,
    /// 最近一个bar的结束时间
    bar_end_time: NaiveDateTime,

    /// 使用Vec比HashMap性能更高，BarData自带barsz_sec
    // closed_this_tick: Vec<BarData>,
    // updated_thistick: Vec<&'a BarData>,

    /// 是否生成vol为零的bar, 在这里还需要过滤，如果该品种TradeSession未正确设置
    /// 上层调用方在创建时，如果其TradeSession是因为找不到而给的full_session替代,
    /// 则zero_vol_bar不应该启用
    zero_vol_bar: bool,
}

impl InstBarBuilder {
    /// 从Vec<BarTime> map创建
    pub fn new(
        inst: &str,
        mut barsz_vs_bartime_map: HashMap<u32, Vec<BarTime>>,
        zero_vol_bar: bool,
    ) -> Self {
        // 丢弃barsize超过86400的
        let bad: HashMap<u32, Vec<BarTime>> = barsz_vs_bartime_map
            .extract_if(|&sz, _| sz >= 86400)
            .collect();
        if !bad.is_empty() {
            log::error!(
                "InstBarBuilder::new, barsz_vs_bartime_map barsize should not big than 86400"
            )
        }
        if barsz_vs_bartime_map.is_empty() {
            log::error!("InstBarBuilder::new, barsz_vs_bartime_map should not be empty")
        }
        let mut builders: Vec<SingleBarBuilder> = barsz_vs_bartime_map
            .into_iter()
            .map(|(barsz, vec)| SingleBarBuilder::new(inst, barsz, vec, zero_vol_bar))
            .collect();

        builders.sort_by(|a, b| a.barsz_sec().cmp(&b.barsz_sec()));

        Self {
            inst: inst.to_string(),
            single_bb_map: builders,
            bar_end_time: NaiveDateTime::default(),
            need_create_barsz: vec![],
            zero_vol_bar,
        }
    }

    /// 从TradeSession对象创建, barsz_sec不能大于等于86400秒(日线)
    pub fn new1(
        inst: &str,
        barsz_sec: &Vec<u32>,
        session: &TradeSession,
        // opt_pre_bars: Option<HashMap<u32, BarData>>,
        zero_vol_bar: bool,
    ) -> Self {
        if barsz_sec.is_empty() {
            log::error!("InstBarBuilder::new1, barsz_sec should not be empty")
        }
        let barsz_vs_bartime_map: HashMap<u32, Vec<BarTime>> = barsz_sec
            .iter()
            .map(|sz| (*sz, BarTime::vec_from_session(&session, *sz)))
            .collect();
        Self::new(inst, barsz_vs_bartime_map, zero_vol_bar)
    }

    /// 从session_minutes创建
    pub fn new2(
        inst: &str,
        barsz_sec: &Vec<u32>,
        session_minutes: Vec<u16>,
        zero_vol_bar: bool,
    ) -> Self {
        let session = TradeSession::new_from_minutes(session_minutes);
        Self::new1(inst, barsz_sec, &session, zero_vol_bar)
    }

    /// 注意!!! 必须在on_tick调用之前，任何on_tick调用之后，都不能再设置prebar
    pub fn set_pre_bars(&mut self, mut pre_bars: HashMap<u32, BarImpl>) -> Result<()> {
        for bb in self.single_bb_map.iter_mut() {
            if let Some(pre_bar) = pre_bars.remove(&bb.barsz_sec()) {
                bb.set_pre_bar(pre_bar)?;
            }
        }
        Ok(())
    }

    pub fn inst(&self) -> &str {
        &self.inst
    }

    /// 如果不再处理某个barsize了，则可以移除
    pub fn remove_barsize(&mut self, barsize: u32) {
        self.single_bb_map.retain(|bb| bb.barsz_sec() != barsize);
    }

    /// 返回值，tick是否在此合约的tradesession之内，
    /// closed_this_tick: 本tick内关闭的bar，
    /// updated_this_tick: 收集Bar的实时变化信息，高开低收量，适用每tick推送的场景，  
    /// 输出都是小周期的在前
    pub fn on_tick<T, B>(
        &mut self,
        tick: &T,
        closed_this_tick: &mut Vec<B>,
        updated_this_tick: Option<&mut Vec<B>>,
    ) -> bool
    where
        T: Ticklike,
        B: for<'a> From<&'a BarImpl>,
    {
        self.need_create_barsz.clear();

        // log::trace!("on_tick, {}", tick);
        let mut tick_time_in_session = true;

        // 是否已经在循环中设置了bar_end_time
        let mut bar_end_time_assigned = false;
        // 遍历所有的barsize，处理OnBar/OnBarClose事件，这次循环里不创建新Bar
        for bb in self.single_bb_map.iter_mut() {
            let barsize = bb.barsz_sec();
            // 利用tick_idx来计算函数的返回值：
            // 各bb的tick_idx仅在check_exists_bar()中计算一次，
            // 在调用create_new_bar()之前，其他地方是不会改变的，
            // 所以，如果tick_idx为负数，则不在tradesession，
            // 理论上，一种barsize的tick_idx为负数，其他所有的都应为负数
            if bb.check_exists_bar(tick) {
                self.need_create_barsz.push(barsize);
            }

            if bb.tick_idx < 0 {
                // 首个或者多个不同barsize的bb，此函数的返回值
                tick_time_in_session = false;
            } else if tick_time_in_session == false {
                // 测试代码，对于同一个合约来说，随便1，5，10分钟等，tick time in session必须是一致的
                // 理论上不应该执行到这里，否则就是逻辑错误
                log::error!(
                    "tick_idx一致性检查失败, {}, 应该全部为负数, 但barsz {} 的值({})不符 ",
                    tick.datetime(),
                    barsize,
                    bb.tick_idx
                );
            }

            if let Some(bar) = bb.opt_closed_this_tick.take() {
                // bar_end_time取较小的，
                // 比如10:15:00,此时1，3，5，15bar完成，但30分钟的bar其实也完成了，因为10:15~10:30商品休市无行情推送，
                // end_time只能取10:15:00, 而不能取10:30:00，否则上层逻辑会有错，
                if !bar_end_time_assigned {
                    self.bar_end_time = *bb.last_bar_end();
                    bar_end_time_assigned = true;
                } else {
                    #[cfg(debug_assertions)]
                    {
                        // 测试代码，bar_end_time必须一致
                        if &self.bar_end_time != bb.last_bar_end() {
                            log::error!(
                                "last_bar_end_not_match {}, pre {} vs single {}",
                                self.inst,
                                self.bar_end_time,
                                bb.last_bar_end()
                            );
                        }
                    }
                    self.bar_end_time = self.bar_end_time.min(*bb.last_bar_end());
                }
                if bar.volume > 0 || self.zero_vol_bar {
                    closed_this_tick.push(B::from(&bar));
                }
            }
        }

        // 所有旧Bar推送完毕后，看看是否需要创建新Bar
        // 注意: create_new_bar()有可能修改tick_idx值, 返回值计算需在此前操作
        for &idx in self.need_create_barsz.iter() {
            if let Some(bb) = self.single_bb_map.iter_mut().find(|b| b.barsz_sec() == idx) {
                bb.create_new_bar(tick);
            }
        }

        if self.zero_vol_bar {
            for bb in self.single_bb_map.iter_mut() {
                closed_this_tick.extend(bb.zerovol_bar_vec.iter().map(|b| B::from(b)));
                bb.zerovol_bar_vec.clear();
            }
        }

        // 实时推送
        if let Some(updvec) = updated_this_tick {
            updvec.extend(
                self.single_bb_map
                    .iter()
                    .filter_map(|bb| bb.last_bar.as_ref().map(Into::into)),
            );
        }

        // 本tick已结束，重置created_this_tick
        for bb in self.single_bb_map.iter_mut() {
            if let Some(bar) = bb.last_bar.as_mut() {
                bar.created_this_tick = false;
            }
        }

        return tick_time_in_session;
    }

    /// 若last_bar的结束时间小于(now-threshold), 则关闭该bar，
    /// closed_bars: 关闭的bar，输出都是小周期的在前
    pub fn on_timer<T>(
        &mut self,
        now: &NaiveDateTime,
        threshold: chrono::TimeDelta,
        closed_bars: &mut Vec<T>,
    ) where
        T: for<'a> From<&'a BarImpl>,
    {
        // 因为这里只有closed_bars，且被take, 所以无需处理created_this_tick
        for bb in self.single_bb_map.iter_mut() {
            bb.on_timer(now, threshold);
            if let Some(bar) = bb.opt_closed_this_tick.take() {
                closed_bars.push(T::from(&bar));
            }
            if !bb.zerovol_bar_vec.is_empty() {
                closed_bars.extend(bb.zerovol_bar_vec.iter().map(|b| T::from(b)));
                bb.zerovol_bar_vec.clear();
            }
        }
    }

    /// 强制完成在force_end_time点上未结束的bar，并收集这些Bar，输出都是小周期的在前
    pub fn force_finish<T>(&mut self, force_end_time: &NaiveDateTime, force_closed: &mut Vec<T>)
    where
        T: for<'a> From<&'a BarImpl>,
    {
        self.on_timer(force_end_time, Duration::zero(), force_closed);
    }
}
