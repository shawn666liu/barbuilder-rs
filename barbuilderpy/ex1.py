import datetime as dt
import pandas as pd
from pathlib import Path
from dateutil.parser import parse as parse_datetime


from barbuilderpy import InstBarBuilder, BarData, TickData
from tradesessionpy import TradeSession


def print_bar(bar: BarData):
    print(
        f"   {int(bar.barsz_sec / 60)}, {bar.begin}~{bar.internal_end.time()}, ohlc({bar.open}, {bar.high}, {bar.low}, {bar.close}), v {bar.volume}"
    )


def row2tick(row: pd.Series) -> TickData:
    obj = TickData()
    for idx in row.index:
        if idx == "datetime":
            obj.datetime = parse_datetime(row[idx])
        elif idx == "tradeday":
            obj.tradeday = parse_datetime(row[idx])
        else:
            setattr(obj, idx, row[idx])
    return obj


def genbar_test():
    # 生成5分钟，15分钟，30分钟K线
    BAR_SIZE_SECONDS = [300, 900, 1800]

    file = Path(__file__).parent.parent / "tick_ag2510_partial.csv"
    df = pd.read_csv(file)
    df["vol_delta"] = df["volume"].diff().fillna(df["volume"]).astype(int)
    df["tnov_delta"] = df["turnover"].diff().fillna(df["turnover"]).astype(int)

    print(df)
    session = TradeSession.new_commodity_session_night()
    minutelist = session.minutes_list()
    instbb = InstBarBuilder(
        "ag2510", BAR_SIZE_SECONDS, minutelist, {}, zero_vol_bar=True
    )
    ticktime = dt.datetime.now()
    for index, row in df.iterrows():
        tick = row2tick(row)
        ticktime = tick.datetime
        print(
            f"\n\n{index+1:<5} tick: {tick.datetime}, px {tick.last}, v {tick.vol_delta}"  # type: ignore
        )
        insession, closed, updated = instbb.on_tick(tick, realtime_feed=False)
        if len(closed) > 0:
            print("closed bar:")
            for bar in closed:
                print_bar(bar)
            pass
        if updated and len(updated) > 0:
            print("updated bar:")
            for bar in updated:
                print_bar(bar)
        pass

    ticktime += dt.timedelta(seconds=6)
    print(f"\n\nontimer {ticktime}")
    closed = instbb.on_timer(ticktime, dt.timedelta(seconds=5))
    if len(closed) > 0:
        print("ontimer closed bar:")
        for bar in closed:
            print_bar(bar)
        pass


if __name__ == "__main__":
    genbar_test()
    pass
