use chrono::{NaiveDate, NaiveDateTime};

pub trait Ticklike {
    fn tradeday(&self) -> &NaiveDate;
    fn datetime(&self) -> &NaiveDateTime;
    fn last_price(&self) -> f64;
    fn openint(&self) -> u64;
    fn volume(&self) -> u64;
    fn turnover(&self) -> f64;

    fn vol_delta(&self) -> u64;
    fn tnov_delta(&self) -> f64;
}
