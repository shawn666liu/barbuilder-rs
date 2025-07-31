# BarBuilder, K线生成器

依赖tradesession crate  
https://github.com/shawn666liu/tradesession-rs.git

### 用途及特点
输入tick,输出K线   
可以输出完结的bar及实时更新的bar  
可以输出成交量为零的bar  
正确处理集合竞价数据  
支持rust/c++/python

### 用法示例
参看 barbuilderpy/ex1.py 文件  
rust/c++用法类似  

### K线切分说明
切分k线时，使用左开右闭区间(]，整点时间是属于前一个周期的，集合竞价属于其后周期，    
比如上午休市时的10:15:00, 属于前一个bar，比如收盘时15:00:00,它属于上一个bar  
比如商品期货，早上的第一个一分钟bar,  
如果该品种不含集合竞价，它是`[9:00:00～9:01:00]`, 第二个bar`(9:01:00~9:02:00]`  
如果该品种包含集合竞价，它是`[8:59:00～9:01:00]`, 第二个bar`(9:01:00~9:02:00]`  


### zero_volume_bar及未知品种tradesession的选择
对于一个新上市的品种，尚不知道其具体的TradeSession, 需要为它指定一个。   
首先看交易所，如果是CFFEX，可以使用国债的session, TradeSession::new_bond_session()  
否则，使用一个带夜盘的商品session, TradeSession::new_commodity_session_night(), 同时不要生成zero_vol_bar  
这样，Session应该可以覆盖该品种时段，不会丢失行情数据，也不会生成多余的bar  
所以，创建InstBarBuilder时，zero_vol_bar和session参数需要调用方进行考虑   

### 应用场景考虑
- 只在Bar完成时推送，用于K线库接收行情组Bar的场景，不用关心Bar的实时变化，
- Bar变化时实时推送，用于实盘交易场景，Bar的高开低收量每一tick都会变，需要持续推送。这种情况开销较大  
  在创建InstBarBuilder时，指定realtime_feed为true，则持续进行变化推送
- 历史数据回放，生成K线
  本模块内部不会出现now()函数获取实时时间的情况，以便支持历史数据进行回放，

### on_timer() 关闭边界上的K线
K线在走完被关闭(closed列表里面返回且finished设置为true)的驱动力，来自对其结束时间的判断，  
- tick的时间戳刚好落在bar的结束时间上，  
- tick的时间戳已经超过该bar的结束时间， 

但这两种情况，在最后一个bar时，可能无法实现。对于上期所，每一个session结束后的500ms时，会推送一个volume为零的tick，  
用于驱动结束该session, 但其他交易所没有这个机制，甚至可能在15:00:00.???时推送有成交量的tick。  
为了保证收集数据的完整性，本模块在每一个session结束时，增加1秒的时间，把这1秒归于上一个bar，包括10:15（仅商品）, 11:30。  
(类似的，在集合竞价所在的session, 该bar会包含其前61秒的数据，即集合竞价的价量计入该bar)  
对于边界上最后的bar,其后没有tick再推送，无法通过tick时间来驱动该bar关闭，所以这里设置了一个定时器来驱动，  
on_timer(now, threshold), 如果now时间超出该bar结束时间已经大于threshold，则强制关闭该bar。  
一般设置threshold为5～8秒比较合适，这个机制也适用于那些交易很不活跃的品种。

对于收集数据的场景，bar关闭得晚了没有影响，数据记录正确就行了。对于实盘交易模式，该bar推送晚了也没有影响，  
因为此时市场已经停止交易了，即使该bar产生了交易信号，也无法在该时段进行交易。


### 周五夜盘延续到周一白天的问题
on_tick()总是在市场行情有推送的时候才会调用，所以不会有问题。  
但是on_timer()不同，内部没有处理非交易日的情况，如果在周六周日白天调用就会有问题，  
所以，应该在tradesession的时间段外，停止on_timer()调用。  
建议每个交易日结束之后，销毁barbuilder，下一个交易日再重建。  

### 其他注意事项
创建InstBarBuilder时，pre_bar的交易日必须是当前交易日(对于回放则必须是特定的交易日)，非当前交易日的应该提前过滤掉。  
同样，调用on_tick时，所有tick的交易日也必须是当前或特定的交易日。 
建议每个交易日结束之后，销毁barbuilder，下一个交易日再重建。  

### Python 绑定
- 切换到需要的虚拟环境  
conda activate your-env-name
- 生成/更新pyi, (可能需要把LD_LIBARY_PATH指向你env所在的lib目录)  
cargo run --bin stub_gen  或者  
LD_LIBARY_PATH=???env/lib  cargo run --bin stub_gen   
- 进入barbuilderpy子目录  
cd barbuilderpy
- 安装maturin  
https://github.com/PyO3/maturin  
conda install maturin 
或者 pip install maturin  
- 编译该虚拟环境对应python版本的whl包,用以分发然后手动安装  
maturin build --release
- 或者,直接为当前虚拟环境安装whl包  
maturin develop --release

### C++绑定
- 编译release版本通过
- 复制target/cxxbridge/{rust, barbuilderpp}及之下的所有.h和.cc文件  
  包括cxx.h, ???.rs.h, ???.rs.cc  
- 下载cxx.cc文件,   
  https://raw.githubusercontent.com/dtolnay/cxx/refs/heads/master/src/cxx.cc
- 复制target/release下面的barbuilderpp.{dll,lib}文件, linux下则为libbarbuilderpp.so
- c++封装文件: 在barbuilderpp/wrapper目录下，复制到c++项目