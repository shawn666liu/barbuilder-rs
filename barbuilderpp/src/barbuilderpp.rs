// #![cfg_attr(debug_assertions, allow(dead_code, unused_imports, unused_variables))]

use anyhow::Result;
use chrono::{DateTime, Datelike, NaiveDate};
use std::collections::HashMap;

use barbuilder::Ticklike;
use barbuilder::data_impl::{BarImpl, TickImpl};

use crate::ffi::{CppBar, CppTick};

pub struct InstBarBuilderPP {
    instbb: barbuilder::InstBarBuilder,
}

/// barsz_sec: 单位秒，就是需要此builder创建哪些周期的K线，60表示1分钟K，300表示5分钟K，不支持日线86400  
/// session_minutes: 单位分钟，来自TradeSession的所有分钟合集
/// pre_bars: 未处理完的旧Bar
/// zero_vol_bar: 是否生成成交量为零的Bar
pub fn create_inst_barbuilder(
    inst: &str,
    barsz_sec: &Vec<u32>,
    session_minutes: Vec<u16>,
    zero_vol_bar: bool,
) -> Box<InstBarBuilderPP> {
    let instbb = barbuilder::InstBarBuilder::new2(inst, barsz_sec, session_minutes, zero_vol_bar);
    Box::new(InstBarBuilderPP { instbb })
}

#[cxx::bridge(namespace = "rustpp")]
mod ffi {

    #[derive(Clone, Debug, Default)]
    struct CppBar {
        /// days since epoch
        tradeday: i32,
        /// nanoseconds since epoch
        begin: i64,
        /// 内部使用
        internal_end: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: u64,
        turnover: f64,
        openint: u64,
        finished: bool,
        barsz_sec: u32,
        /// 是否因本tick触发而创建,上级on_tick调用时知道哪些bar是该tick新创建的
        created_this_tick: bool,
    }

    // 因为CppTick无法携带NaiveDateTime对象，
    // 所以无法直接实现Ticklike trait
    #[derive(Clone, Debug, Default)]
    struct CppTick {
        tradeday: i32,
        datetime: i64,
        last: f64,
        openint: u64,
        volume: u64,
        turnover: f64,
        vol_delta: u64,
        tnov_delta: f64,
    }

    extern "Rust" {
        type InstBarBuilderPP;

        /// barsz_sec: 单位秒，就是需要此builder创建哪些周期的K线，60表示1分钟K，300表示5分钟K，不支持日线86400  
        /// session_minutes: 单位分钟，来自TradeSession的所有分钟合集
        /// pre_bars: 未处理完的旧Bar, 必须是跟后续tick在同一交易日，否则应该过滤掉
        /// zero_vol_bar: 是否生成成交量为零的Bar
        fn create_inst_barbuilder(
            inst: &str,
            barsz_sec: &Vec<u32>,
            session_minutes: Vec<u16>,
            zero_vol_bar: bool,
        ) -> Box<InstBarBuilderPP>;

        /// 注意!!! 必须在on_tick调用之前，任何on_tick调用之后，都不能再设置prebar
        ///
        /// CppBar已自带barsize信息，
        fn set_pre_bars(&mut self, pre_bars: &Vec<CppBar>) -> Result<()>;

        /// 如果realtime_feed为空，则updated_this_tick不会被填充，
        /// 返回值，tick是否在此合约的tradesession之内，
        /// closed_this_tick: 本tick内关闭的bar，
        /// updated_this_tick: 收集Bar的实时变化信息，高开低收量，适用每tick推送的场景，
        /// 输出都是小周期的在前
        fn on_tick(
            self: &mut InstBarBuilderPP,
            closed_this_tick: &mut Vec<CppBar>,
            updated_this_tick: &mut Vec<CppBar>,
            tick: &CppTick,
            realtime_feed: bool,
        ) -> bool;
        /// 如果realtime_feed为空，则updated_this_tick不会被填充，
        /// 返回值，tick是否在此合约的tradesession之内，
        /// closed_this_tick: 本tick内关闭的bar，
        /// updated_this_tick: 收集Bar的实时变化信息，高开低收量，适用每tick推送的场景，
        /// 输出都是小周期的在前
        fn on_tick_detail(
            self: &mut InstBarBuilderPP,
            closed_this_tick: &mut Vec<CppBar>,
            updated_this_tick: &mut Vec<CppBar>,
            trade_day: i32,
            datetime: i64,
            last_price: f64,
            openint: u64,
            volume: u64,
            turnover: f64,
            vol_delta: u64,
            tnov_delta: f64,
            realtime_feed: bool,
        ) -> bool;

        /// 若last_bar的结束时间小于(now-threshold), 则关闭该bar，
        /// closed_bars: 关闭的bar，
        /// now: nanos_since_epoch，
        /// threhold: nanos_since_midnight，
        /// 输出都是小周期的在前
        fn on_timer(&mut self, now: i64, threshold: i64, closed_bars: &mut Vec<CppBar>);

        fn remove_barsize(&mut self, barsize: u32);
    }
}

impl Into<BarImpl> for CppBar {
    fn into(self) -> BarImpl {
        (&self).into()
    }
}
impl Into<BarImpl> for &CppBar {
    fn into(self) -> BarImpl {
        let days_from_ce = self.tradeday + 719163;
        let trade_day = NaiveDate::from_num_days_from_ce_opt(days_from_ce)
            .expect("from_num_days_from_ce_opt() failed");
        let begin = DateTime::from_timestamp_nanos(self.begin).naive_utc();
        let internal_end = DateTime::from_timestamp_nanos(self.internal_end).naive_utc();
        BarImpl::new(
            trade_day,
            begin,
            internal_end,
            self.open,
            self.high,
            self.low,
            self.close,
            self.volume,
            self.turnover,
            self.openint,
            self.finished,
            self.barsz_sec,
            self.created_this_tick,
        )
    }
}
impl From<&BarImpl> for CppBar {
    fn from(v: &BarImpl) -> Self {
        let days_since_epoch = v.tradeday.num_days_from_ce() - 719163;
        let nanos_since_epoch = v
            .begin
            .and_utc()
            .timestamp_nanos_opt()
            .expect("timestamp_nanos_opt() failed");
        let internal_end = v
            .internal_end
            .and_utc()
            .timestamp_nanos_opt()
            .expect("timestamp_nanos_opt() failed");
        CppBar {
            tradeday: days_since_epoch,
            begin: nanos_since_epoch,
            internal_end,
            open: v.open,
            high: v.high,
            low: v.low,
            close: v.close,
            volume: v.volume,
            turnover: v.turnover,
            openint: v.openint,
            finished: v.finished,
            barsz_sec: v.barsz_sec,
            created_this_tick: v.created_this_tick,
        }
    }
}
impl From<BarImpl> for CppBar {
    fn from(value: BarImpl) -> Self {
        Self::from(&value)
    }
}

// impl Into<TickImpl> for &CppTick {
//     fn into(self) -> TickImpl {
//         let days_from_ce = self.tradeday + 719163;
//         let date = NaiveDate::from_num_days_from_ce_opt(days_from_ce)
//             .expect("from_num_days_from_ce_opt() failed");
//         let time = DateTime::from_timestamp_nanos(self.datetime).naive_utc();
//         TickImpl::new(
//             date,
//             time,
//             self.last,
//             self.openint,
//             self.volume,
//             self.turnover,
//             self.vol_delta,
//             self.tnov_delta,
//         )
//     }
// }

// impl Into<TickImpl> for CppTick {
//     fn into(self) -> TickImpl {
//         (&self).into()
//     }
// }

impl Ticklike for CppTick {
    fn tradeday(&self) -> NaiveDate {
        let days_from_ce = self.tradeday + 719163;
        let date = NaiveDate::from_num_days_from_ce_opt(days_from_ce)
            .expect("from_num_days_from_ce_opt() failed");
        date
    }

    fn datetime(&self) -> chrono::NaiveDateTime {
        let time = DateTime::from_timestamp_nanos(self.datetime).naive_utc();
        time
    }

    fn last_price(&self) -> f64 {
        self.last
    }

    fn openint(&self) -> u64 {
        self.openint
    }

    fn volume(&self) -> u64 {
        self.volume
    }

    fn turnover(&self) -> f64 {
        self.turnover
    }

    fn vol_delta(&self) -> u64 {
        self.vol_delta
    }

    fn tnov_delta(&self) -> f64 {
        self.tnov_delta
    }
}

impl InstBarBuilderPP {
    pub fn set_pre_bars(&mut self, pre_bars: &Vec<ffi::CppBar>) -> Result<()> {
        if !pre_bars.is_empty() {
            let pre_bars: HashMap<u32, BarImpl> = pre_bars
                .iter()
                .map(|bar| (bar.barsz_sec, bar.into()))
                .collect();
            self.instbb.set_pre_bars(pre_bars)?;
        }
        Ok(())
    }

    pub fn on_tick(
        &mut self,
        closed_this_tick: &mut Vec<CppBar>,
        updated_this_tick: &mut Vec<CppBar>,
        tick: &CppTick,
        realtime_feed: bool,
    ) -> bool {
        if realtime_feed {
            self.instbb
                .on_tick(tick, closed_this_tick, Some(updated_this_tick))
        } else {
            self.instbb.on_tick(tick, closed_this_tick, None)
        }
    }
    pub fn on_tick_detail(
        &mut self,
        closed_this_tick: &mut Vec<CppBar>,
        updated_this_tick: &mut Vec<CppBar>,
        trade_day: i32,
        datetime: i64,
        last_price: f64,
        openint: u64,
        volume: u64,
        turnover: f64,
        vol_delta: u64,
        tnov_delta: f64,
        realtime_feed: bool,
    ) -> bool {
        let days_from_ce = trade_day + 719163;
        let date = NaiveDate::from_num_days_from_ce_opt(days_from_ce)
            .expect("from_num_days_from_ce_opt() failed");
        let time = DateTime::from_timestamp_nanos(datetime).naive_utc();
        let tick = TickImpl::new(
            date, time, last_price, openint, volume, turnover, vol_delta, tnov_delta,
        );
        if realtime_feed {
            self.instbb
                .on_tick(&tick, closed_this_tick, Some(updated_this_tick))
        } else {
            self.instbb.on_tick(&tick, closed_this_tick, None)
        }
    }
    pub fn on_timer(&mut self, now: i64, threshold: i64, closed_bars: &mut Vec<CppBar>) {
        let now = DateTime::from_timestamp_nanos(now).naive_utc();
        let threshold = chrono::Duration::nanoseconds(threshold);
        self.instbb.on_timer(&now, threshold, closed_bars);
    }
    pub fn remove_barsize(&mut self, barsize: u32) {
        self.instbb.remove_barsize(barsize);
    }
}
