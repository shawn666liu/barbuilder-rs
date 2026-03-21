use anyhow::Result;
use barbuilder::{
    InstBarBuilder,
    data_impl::{BarImpl, TickImpl},
};
use chrono::{Duration, Local, NaiveDateTime};
use csv;
use tradesession::TradeSession;

fn print_bar(bar: &BarImpl) {
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

    let mut closed_this_tick: Vec<BarImpl> = Vec::with_capacity(20);
    let mut updated_this_tick: Vec<BarImpl> = Vec::with_capacity(20);
    let mut all_bars: Vec<BarImpl> = Vec::with_capacity(100);

    let start = Local::now().naive_local();

    for _ in 1..100 {
        closed_this_tick.clear();
        updated_this_tick.clear();
        all_bars.clear();

        let mut bb = InstBarBuilder::new1("ag2510", &bar_size_vec, &ts, false);

        let tickcsv = include_str!("../../tick_ag2510_full.csv");
        let mut rdr = csv::Reader::from_reader(tickcsv.as_bytes());

        let mut prev_volume = 0;
        let mut prev_turnover = 0.0;

        let mut tick_index = 0;
        let mut tick_time = NaiveDateTime::default();

        for result in rdr.deserialize() {
            tick_index += 1;
            let mut tick: TickImpl = result?;
            tick.vol_delta = tick.volume.saturating_sub(prev_volume);
            tick.tnov_delta = tick.turnover - prev_turnover;

            // 更新前值
            prev_volume = tick.volume;
            prev_turnover = tick.turnover;

            // println!(
            //     "\n\n{:<5} tick: {:<23}, px {}, v {}",
            //     tick_index, tick.datetime, tick.last, tick.vol_delta
            // );

            // if tick_index == 124 {
            //     let x = 0;
            //     println!("debug here, {}", x);
            // }

            tick_time = tick.datetime;
            let _insession = bb.on_tick(&tick, &mut closed_this_tick, Some(&mut updated_this_tick));
            // println!("in? {}", _insession);
            if !closed_this_tick.is_empty() {
                // println!("closed bar:");
                // for bar in closed_this_tick.iter() {
                //     print_bar(bar);
                // }
                // closed_this_tick.clear();
                all_bars.extend(closed_this_tick.drain(..));
            }

            if !updated_this_tick.is_empty() {
                // println!("updated bar:");
                // for bar in updated_this_tick.iter() {
                //     print_bar(bar);
                // }
                updated_this_tick.clear();
            }
        }

        tick_time += Duration::seconds(6);
        // println!("\n\nontimer: {}", tick_time);
        bb.on_timer(&tick_time, Duration::seconds(5), &mut closed_this_tick);

        if !closed_this_tick.is_empty() {
            // println!("ontimer closed bar:");
            // for bar in closed_this_tick.iter() {
            //     print_bar(bar);
            // }
            // closed_this_tick.clear();
            all_bars.extend(closed_this_tick.drain(..));
        }
    }
    let end = Local::now().naive_local();
    println!("all_bars count = {}", all_bars.len());
    let elapsed: chrono::TimeDelta = end - start;
    println!("time elapsed {} ms", elapsed.num_milliseconds());
    Ok(())
}
