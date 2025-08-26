// #![cfg_attr(debug_assertions, allow(dead_code, unused_imports, unused_variables))]

mod bartime;
pub mod data_impl;
mod data_traits;
mod instbarbuilder;
mod singlebarbuilder;
pub mod util;

pub use bartime::*;
pub use data_traits::*;
pub use instbarbuilder::*;
pub use singlebarbuilder::*;

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr};

    use super::*;
    use crate::data_impl::{BarData, TickData};
    use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
    use tradesession::TradeSession;

    fn print_bar(bar: &BarData) {
        println!(
            "   {}, {} ~ {}, ohlc({}, {}, {}, {}), v {}",
            bar.barsz_sec / 60,
            bar.begin,
            bar.internal_end.time(),
            bar.open,
            bar.high,
            bar.low,
            bar.close,
            bar.volume
        );
    }

    // 测试有prebar及ontime
    #[test]
    fn prebar_ontimer() -> anyhow::Result<()> {
        let barsize_sec = 900;
        let minutes = if barsize_sec == 300 {
            5
        } else if barsize_sec == 900 {
            15
        } else if barsize_sec == 1800 {
            30
        } else {
            0
        };

        let ts = TradeSession::new_commodity_session_night();
        let mut bar = BarData::default();
        let date = NaiveDate::from_ymd_opt(2025, 7, 23).expect("");
        let time = NaiveTime::from_hms_opt(9, minutes, 0).expect("");
        bar.begin = NaiveDateTime::new(date, time);
        bar.barsz_sec = barsize_sec;
        bar.tradeday = date;
        bar.open = 9468.0;
        bar.high = 9471.0;
        bar.low = 9459.0;
        bar.close = 9463.0;
        bar.volume = 100;
        bar.openint = 482770;
        bar.turnover = 94017413385.0;
        let mut prebars: HashMap<u32, BarData> = HashMap::new();
        prebars.insert(barsize_sec, bar);
        let mut bb = InstBarBuilder::new1("ag2510", &vec![barsize_sec], &ts, true);
        bb.set_pre_bars(prebars)?;
        let mut closed_this_tick: Vec<BarData> = vec![];
        let mut updated_this_tick: Vec<BarData> = vec![];
        let mut tick = TickData::default();
        tick.datetime = date.and_hms_milli_opt(9, 16, 10, 500).expect("");
        tick.tradeday = date;
        tick.last = 9478.0;
        tick.vol_delta = 50;
        tick.volume = 760184;
        tick.turnover = 93975804915.0;
        tick.tnov_delta = 68900.0;

        println!(
            "\n\nontick: {:<23}, px {}, v {}",
            tick.datetime, tick.last, tick.vol_delta
        );

        bb.on_tick(&tick, &mut closed_this_tick, Some(&mut updated_this_tick));

        if !closed_this_tick.is_empty() {
            println!("closed bar:");
            for bar in closed_this_tick.iter() {
                print_bar(bar);
            }
            closed_this_tick.clear();
        }

        if !updated_this_tick.is_empty() {
            println!("updated bar:");
            for bar in updated_this_tick.iter() {
                print_bar(bar);
            }
            updated_this_tick.clear();
        }
        let now = NaiveDateTime::from_str("2025-07-23T10:20:00")?;
        println!("\n\nontimer: {}", now);
        bb.on_timer(&now, Duration::seconds(10), &mut closed_this_tick);
        if !closed_this_tick.is_empty() {
            println!("ontimer1 closed bar:");
            for bar in closed_this_tick.iter() {
                print_bar(bar);
            }
            closed_this_tick.clear();
        }

        tick.datetime = tick.datetime.date().and_hms_opt(14, 51, 13).expect("");

        println!(
            "\n\ntick: {:<23}, px {}, v {}",
            tick.datetime, tick.last, tick.vol_delta
        );
        bb.on_tick(&tick, &mut closed_this_tick, Some(&mut updated_this_tick));

        if !closed_this_tick.is_empty() {
            println!("closed bar:");
            for bar in closed_this_tick.iter() {
                print_bar(bar);
            }
            closed_this_tick.clear();
        }
        if !updated_this_tick.is_empty() {
            println!("updated bar:");
            for bar in updated_this_tick.iter() {
                print_bar(bar);
            }
            updated_this_tick.clear();
        }

        let now = NaiveDateTime::from_str("2025-07-23T15:03:00")?;
        println!("\n\nontimer: {}", now);
        bb.on_timer(&now, Duration::seconds(5), &mut closed_this_tick);
        if !closed_this_tick.is_empty() {
            println!("ontimer2 closed bar:");
            for bar in closed_this_tick.iter() {
                print_bar(bar);
            }
            closed_this_tick.clear();
        }

        Ok(())
    }
}
