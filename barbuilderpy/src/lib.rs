// #![cfg_attr(debug_assertions, allow(dead_code, unused_imports, unused_variables))]

use chrono::{NaiveDate, NaiveDateTime};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use std::collections::HashMap;
use std::vec;

use barbuilder::Ticklike;

fn to_pyerr(err: anyhow::Error) -> PyErr {
    PyErr::new::<PyException, _>(err.to_string())
}

#[gen_stub_pyclass]
#[pyclass]
pub struct InstBarBuilder {
    instbb: barbuilder::InstBarBuilder,
}

#[gen_stub_pyclass]
#[pyclass(get_all, set_all)]
#[derive(Clone, Default, Debug)]
pub struct BarData {
    pub tradeday: NaiveDate,
    pub begin: NaiveDateTime,
    /// 内部使用
    pub internal_end: NaiveDateTime,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
    pub turnover: f64,
    pub openint: u64,
    pub finished: bool,
    pub barsz_sec: u32,
    /// 是否因本tick触发而创建,上级on_tick调用时知道哪些bar是该tick新创建的
    pub created_this_tick: bool,
}
impl Into<barbuilder::data_impl::BarData> for BarData {
    fn into(self) -> barbuilder::data_impl::BarData {
        (&self).into()
    }
}

impl Into<barbuilder::data_impl::BarData> for &BarData {
    fn into(self) -> barbuilder::data_impl::BarData {
        barbuilder::data_impl::BarData::new(
            self.tradeday,
            self.begin,
            self.internal_end,
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

impl From<&barbuilder::data_impl::BarData> for BarData {
    fn from(v: &barbuilder::data_impl::BarData) -> Self {
        BarData {
            tradeday: v.tradeday,
            begin: v.begin,
            internal_end: v.internal_end,
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
impl From<barbuilder::data_impl::BarData> for BarData {
    fn from(value: barbuilder::data_impl::BarData) -> Self {
        Self::from(&value)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl BarData {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }
    #[pyo3(name = "__repr__")]
    pub fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

#[gen_stub_pyclass]
#[pyclass(get_all, set_all)]
#[derive(Clone, Debug, Default)]
pub struct TickData {
    pub tradeday: NaiveDate,
    pub datetime: NaiveDateTime,
    pub last: f64,
    pub openint: u64,
    pub volume: u64,
    pub turnover: f64,
    pub vol_delta: u64,
    pub tnov_delta: f64,
}

impl Ticklike for &TickData {
    fn tradeday(&self) -> &chrono::NaiveDate {
        &self.tradeday
    }
    fn datetime(&self) -> &chrono::NaiveDateTime {
        &self.datetime
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

impl Ticklike for TickData {
    fn tradeday(&self) -> &chrono::NaiveDate {
        &self.tradeday
    }
    fn datetime(&self) -> &chrono::NaiveDateTime {
        &self.datetime
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

#[gen_stub_pymethods]
#[pymethods]
impl TickData {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    #[pyo3(name = "__repr__")]
    pub fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl InstBarBuilder {
    #[new]
    pub fn new(
        inst: &str,
        barsz_sec: Vec<u32>,
        session_minutes: Vec<u16>,

        zero_vol_bar: bool,
    ) -> Self {
        let instbb =
            barbuilder::InstBarBuilder::new2(inst, &barsz_sec, session_minutes, zero_vol_bar);
        InstBarBuilder { instbb }
    }

    /// 注意!!! 必须在on_tick调用之前，任何on_tick调用之后，都不能再设置prebar
    pub fn set_pre_bars(&mut self, pre_bar_map: HashMap<u32, BarData>) -> PyResult<()> {
        if !pre_bar_map.is_empty() {
            let pre_bars: HashMap<u32, barbuilder::data_impl::BarData> = pre_bar_map
                .iter()
                .map(|(sz, bar)| (*sz, bar.into()))
                .collect();
            self.instbb.set_pre_bars(pre_bars).map_err(to_pyerr)?;
        }
        Ok(())
    }

    pub fn to_string(&self) -> String {
        format!("InstBarbuilder, {}", self.instbb.inst())
    }

    /// 返回值，tick是否在此合约的tradesession之内，
    /// closed_this_tick: 本tick内关闭的bar，
    /// updated_this_tick: 收集Bar的实时变化信息，高开低收量，适用每tick推送的场景，
    /// 输出都是小周期的在前
    pub fn on_tick(
        &mut self,
        tick: &TickData,
        realtime_feed: bool,
    ) -> (bool, Vec<BarData>, Option<Vec<BarData>>) {
        let mut closed_this_tick = vec![];

        if realtime_feed {
            let mut updated_this_tick = vec![];
            let insession =
                self.instbb
                    .on_tick(&tick, &mut closed_this_tick, Some(&mut updated_this_tick));
            return (insession, closed_this_tick, Some(updated_this_tick));
        } else {
            let insession = self.instbb.on_tick(&tick, &mut closed_this_tick, None);
            return (insession, closed_this_tick, None);
        }
    }
    /// 返回值，tick是否在此合约的tradesession之内，
    /// closed_this_tick: 本tick内关闭的bar，
    /// updated_this_tick: 收集Bar的实时变化信息，高开低收量，适用每tick推送的场景，
    /// 输出都是小周期的在前
    pub fn on_tick_detail(
        &mut self,
        tradeday: NaiveDate,
        datetime: NaiveDateTime,
        last: f64,
        openint: u64,
        volume: u64,
        turnover: f64,
        vol_delta: u64,
        tnov_delta: f64,
        realtime_feed: bool,
    ) -> (bool, Vec<BarData>, Option<Vec<BarData>>) {
        let tick = barbuilder::data_impl::TickData {
            tradeday,
            datetime,
            last,
            openint,
            volume,
            turnover,
            vol_delta,
            tnov_delta,
        };
        let mut closed_this_tick = vec![];

        if realtime_feed {
            let mut updated_this_tick = vec![];
            let insession =
                self.instbb
                    .on_tick(&tick, &mut closed_this_tick, Some(&mut updated_this_tick));
            return (insession, closed_this_tick, Some(updated_this_tick));
        } else {
            let insession = self.instbb.on_tick(&tick, &mut closed_this_tick, None);
            return (insession, closed_this_tick, None);
        }
    }

    /// 若last_bar的结束时间小于(now-threshold), 则关闭该bar，
    /// 返回关闭的bar，输出都是小周期的在前
    pub fn on_timer(&mut self, now: NaiveDateTime, threshold: chrono::TimeDelta) -> Vec<BarData> {
        let mut closed_this_tick = vec![];
        self.instbb.on_timer(&now, threshold, &mut closed_this_tick);
        closed_this_tick
    }

    pub fn remove_barsize(&mut self, barsize: u32) {
        self.instbb.remove_barsize(barsize);
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn barbuilderpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<BarData>()?;
    m.add_class::<TickData>()?;
    m.add_class::<InstBarBuilder>()?;
    Ok(())
}

// Define a function to gather stub information.
define_stub_info_gatherer!(stub_info);
