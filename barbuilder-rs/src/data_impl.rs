use chrono::{NaiveDate, NaiveDateTime};
use serde::Deserialize;

use crate::data_traits::*;

// 自定义 NaiveDateTime 反序列化函数
pub fn deserialize_datetime_with_millis<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    // 支持两种格式：带毫秒和不带毫秒
    if let Ok(dt) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(dt);
    }
    // 尝试不带毫秒的格式
    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BarData {
    pub tradeday: NaiveDate,
    pub begin: NaiveDateTime,
    /// 内部使用，提前算好，空间换时间，
    /// 需要包含with_slice_end信息，
    pub internal_end: NaiveDateTime,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
    #[serde(default)]
    pub turnover: f64, // 期货才有，让它可选
    #[serde(default)]
    pub openint: u64, // 期货才有，让它可选
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub barsz_sec: u32,
}

impl BarData {
    pub fn new(
        tradeday: NaiveDate,
        begin: NaiveDateTime,
        internal_end: NaiveDateTime,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: u64,
        turnover: f64,
        openint: u64,
        finished: bool,
        barsz_sec: u32,
    ) -> Self {
        Self {
            tradeday,
            begin,
            internal_end,
            open,
            high,
            low,
            close,
            volume,
            turnover,
            openint,
            finished,
            barsz_sec,
        }
    }
}

impl From<&BarData> for BarData {
    fn from(value: &BarData) -> Self {
        value.clone()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TickData {
    pub tradeday: NaiveDate,
    #[serde(deserialize_with = "deserialize_datetime_with_millis")]
    pub datetime: NaiveDateTime,
    pub last: f64,
    pub openint: u64,
    pub volume: u64,
    pub turnover: f64,
    #[serde(default)]
    pub vol_delta: u64,
    #[serde(default)]
    pub tnov_delta: f64,
}

impl TickData {
    pub fn new(
        trade_day: NaiveDate,
        datetime: NaiveDateTime,
        last_price: f64,
        openint: u64,
        volume: u64,
        turnover: f64,
        vol_delta: u64,
        tnov_delta: f64,
    ) -> Self {
        Self {
            tradeday: trade_day,
            datetime,
            last: last_price,
            openint,
            volume,
            turnover,
            vol_delta,
            tnov_delta,
        }
    }
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
