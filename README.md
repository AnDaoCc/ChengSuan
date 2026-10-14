# 澄算 ChengSuan

Windows x64 便携成本计算与投资监控工具，版本 1.1.0。成品位于 `release/ChengSuan.exe`。

直接使用：[下载 Windows 便携版 ChengSuan.exe](https://github.com/AnDaoCc/ChengSuan/releases/download/v1.1.0/ChengSuan.exe)。源码仓库不保存构建产物和个人数据，下载成品请前往 [Releases](https://github.com/AnDaoCc/ChengSuan/releases)。

双击 EXE 即可运行；系统需已有 Microsoft Edge WebView2 Runtime。首次启动在 EXE 旁创建 `ChengSuanData`。移动时携带整个数据文件夹，升级前退出软件并仅替换 EXE。无需安装 Node、Rust 或 Python。

## 功能

方案及成本项目管理、有效输入自动保存、周期费用、用量计费、明确启用的一次性分摊、多币种换算、参考汇率手动联网更新、手动汇率、数据库备份恢复、剪贴板、Excel、CSV 和澄算工程导出导入。

成本按一年 365 天、12 个月计算等效平均值。按完整计费单位计费时，每次使用或每份产出独立向上取整。参考汇率不包含银行手续费；断网时保留方案内已有汇率。

## 投资与持仓

侧栏进入“投资与持仓”，支持沪市、深市、北交所、主要指数、普通及黄金 ETF、实物黄金和加密货币。股票及 ETF 从完整分页目录检索；初次加载需要联网建立目录，之后保留本地目录。默认币种为 BTC、ETH、USDT、BNB、SOL、XRP、DOGE，其他币种可搜索，链上币通过“公链 ID + 合约地址”读取后核对交易池加入自选。

在资产详情点击“记录交易”，或从“我的持仓”选择标的。数量填实际到账的股、份、克或币；币圈默认 USDT。填实际单价和同币种手续费，人民币换算依据按成交时冻结。USDT 不按固定美元汇率处理。起始持仓用于已有资产；持仓校正需要填写依据。采用移动加权平均成本，历史编辑会重算，任何时间的超额卖出都会阻止保存。

“我的持仓”按账户或全部账户显示市值、剩余成本、未实现盈亏、已实现收益及占比。缺报价、缓存和手动估价有明确标记。黄金按人民币／克及设置纯度估算，不包含品牌溢价或回购价差；不同纯度的黄金目前按同一资产设置处理，请核对后使用。

软件运行时后台持续监控，包括切换页面及最小化。关闭软件后停止。提醒为应用内提示；价格阈值使用人民币。有效新报价首次达到或重新越过阈值才触发，默认冷却 15 分钟，缓存及手动估价不触发。无报价时间的链上池参考价仅用于估值，不触发实时提醒。

行情设置默认免费源，股票 30 秒、币种及黄金 60 秒尝试刷新。数据可能延迟或限流；失败保留最后有效报价。可填写 AllTick Token、CoinGecko Demo / Pro Key，选择对应模式及用量上限。密钥本机加密，换电脑后重新填写。授权周期和标的按实际权限提供，不提供自动交易功能。

图表本地打包，包含蜡烛、缩放、十字光标和 MA5/10/20。股票默认日 K 前复权；持仓估值使用实际报价。只有源提供成交量时才显示成交量。免费 CoinGecko 历史周期依源实际聚合提供。

AllTick 可选择轮询或逐笔推送。推送订阅上限默认每类 5 个，按实际购买权限设置；其余标的保持轮询。权限拒绝、连接中断及配额不足会显示提示并退避重连。当前未提供授权密钥，因此授权报价、推送和付费历史周期的实际联网验收仍待完成。

升级 1.1.0 前退出旧软件，仅替换 EXE 并保留 `ChengSuanData`。首次启动自动迁移数据库到版本 3，迁移前自动备份。数据库备份覆盖成本和投资数据；旧成本工程文件仍兼容，但仅包含成本方案。

## 开发与构建

安装 Node.js、Rust MSVC、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
npm ci
npm run desktop
npm run typecheck
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run release
```

本工作区也提供隔离 Rust 工具链：

```powershell
./scripts/cargo.ps1 test --manifest-path src-tauri/Cargo.toml --lib
./scripts/release.ps1
```

Release 使用 Tauri 官方 `--no-bundle`，静态链接 VC Runtime 和 SQLite，页面、字体和图标嵌入 EXE。不要将 `target`、开发工具链或 `node_modules` 一起分发。

## 1.0.2 改进

全部币种选择器统一显示“代码 · 中文名称”，覆盖方案汇总币种、成本项目原币种和默认币种设置。使用系统内置中文国际化数据，离线可用；常用币种仍优先排列。

## 1.0.1 修复

修复在线汇率获取失败：Frankfurter 展开后的来源信息是一组详细对象，原程序将整组 JSON 错当来源名称，超过前端 200 字限制。现在保存简短来源说明，保持原始十进制汇率，并提供可读的中文校验错误。

工程规范见 `AGENTS.md`，架构说明见 `docs/ARCHITECTURE.md`，实际验证范围见 `docs/RELEASE_VERIFICATION.md`。
