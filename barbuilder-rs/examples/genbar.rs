use anyhow::Result;
use barbuilder::{
    InstBarBuilder,
    data_impl::{BarData, TickData},
};
use chrono::{Duration, NaiveDateTime};
use csv;
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

fn main() -> Result<()> {
    let bar_size_vec = vec![900, 3600];

    let ts = TradeSession::new_commodity_session_night();
    let mut bb = InstBarBuilder::new1("ag2510", &bar_size_vec, &ts, None, true);

    let tickcsv = include_str!("../../tick_ag2510_partial.csv");
    let mut rdr = csv::Reader::from_reader(tickcsv.as_bytes());

    let mut prev_volume = 0;
    let mut prev_turnover = 0.0;
    let mut closed_this_tick: Vec<BarData> = vec![];
    let mut updated_this_tick: Vec<BarData> = vec![];
    let mut tick_index = 0;
    let mut tick_time = NaiveDateTime::default();
    for result in rdr.deserialize() {
        tick_index += 1;
        let mut tick: TickData = result?;
        tick.vol_delta = tick.volume.saturating_sub(prev_volume);
        tick.tnov_delta = tick.turnover - prev_turnover;

        // 更新前值
        prev_volume = tick.volume;
        prev_turnover = tick.turnover;

        println!(
            "\n\n{:<5} tick: {:<23}, px {}, v {}",
            tick_index, tick.datetime, tick.last, tick.vol_delta
        );

        if tick_index == 124 {
            let x = 0;
            println!("debug here, {}", x);
        }

        tick_time = tick.datetime;
        let _insession = bb.on_tick(&tick, &mut closed_this_tick, Some(&mut updated_this_tick));
        // println!("in? {}", _insession);
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
    }

    tick_time += Duration::seconds(6);
    println!("\n\nontimer: {}", tick_time);
    bb.on_timer(&tick_time, Duration::seconds(5), &mut closed_this_tick);

    if !closed_this_tick.is_empty() {
        println!("ontimer closed bar:");
        for bar in closed_this_tick.iter() {
            print_bar(bar);
        }
        closed_this_tick.clear();
    }
    Ok(())
}
