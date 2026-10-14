# Release verification

## 1.1.0 — Investment market and portfolio (2026-10-13)

Verified on the current Windows development machine; existing `release/ChengSuanData` was not opened by tests. All development/GUI fixtures used isolated temporary or `verification/investment-1.1.0` directories.

- Strict TypeScript checks passed. 31 Vitest tests passed: original cost/export/autosave coverage plus weighted investment cost, partial sales/fees, liquidation, historical overselling, CNY/USD/USDT conversion, small prices, purity, accounts/corrections, missing quotes, alert latches/cooldown, candle ordering/MA, sequential persistence and rejected writes.
- 23 offline Rust tests passed: version 2 → 3 migration with a consistent pre-migration backup, exact cost-plan preservation, investment restart/move, backup restore, revision conflict, transaction rollback, integer-share checks, corrupt/future databases, portable directory locking, encrypted DPAPI credentials, GBK quotes, contract/pool direction/case, gold minute selection and weekday/weekend night dates, offline backoff/cache retention, incremental candles and identity-safe authorized currency mapping. Push packet parsing was tested using protocol fixtures.
- Explicit live free-source tests verified the complete Eastmoney directory: **5,923 stocks, 1,636 ETFs and 361 Beijing stock entries** at this run. Counts are observations, not hardcoded acceptance constants in the product.
- Real Tencent stock quotes, CoinGecko Bitcoin quotes and Frankfurter USD/CNY references were received. Real SGE Au99.99 night quotes were matched to the observed minute and source trade-day label. One parallel live run encountered a CoinGecko timeout; a targeted retry passed. Free service availability is intermittent and is not presented as an uptime guarantee.
- Real historical candles were returned: CN stock and SGE (600 displayed bars each), CoinGecko 7-day OHLC (42 bars), Ethereum USDT pool daily OHLCV (183 bars). The target USDT was on the **quote** side of the highest-liquidity pool. Missing SGE/CoinGecko volume remained missing; pool volume was identified as USD turnover.
- Native EXE GUI verification passed in `verification/investment-1.1.0/session-1791902001837/report.json`: a version-2 database upgraded with backup and original cost plan intact; seven default crypto assets, real Bitcoin candles and MA lines, actual buy/fee/frozen-FX entry, oversell rejection, valid partial sell, light/dark rendering, monitoring after leaving the investment page and while minimized, consistent backup, request cessation on close, replacement/move to a Chinese path and exact ledger recovery on restart.
- The UI sale fixture bought 0.01 BTC for 80,000 USDT/unit with a 1 USDT fee and 6.7 CNY/USDT conversion. Selling 0.005 at 90,000 with a 1 USDT fee left **2,683.35 CNY** basis and **324.95 CNY** realized profit. This is synthetic test data, not a production example account.
- Windows x64 Tauri no-bundle release was built with static VC Runtime and bundled SQLite. PE imports contain Windows system DLLs and Windows UCRT API sets; there is no separately shipped SQLite, WebView2Loader or VC Runtime DLL. WebView2 remains a system prerequisite.
- Lightweight Charts and its license/attribution are bundled locally. Dependency lockfiles are included with the source delivery. Final npm production dependency audit reported zero known vulnerabilities.

### Limits of this verification

No AllTick Token or CoinGecko Demo/Pro Key was supplied. Authorized live quotes, WebSocket permissions/connection quotas, paid historical periods and key migration to a different physical computer were **not** live verified. Protocol compilation/fixtures and same-user DPAPI encryption were verified; errors/fallbacks are implemented without exposing tokens.

No clean Windows VM, physical USB drive or full filesystem/registry tracing was available. Native screenshots were checked on the current display (150% WebView device scale), not a complete DPI matrix. Stock live tests ran outside the mainland trading session; production session timing, holiday calendars and prolonged disconnect/reconnect sessions still need independent acceptance. Gold weekend night mapping is covered; holiday-crossing dates are identified as needing verification. Chain pools without an observed trade timestamp provide delayed reference valuation and intentionally do not fire real-time alerts.

Final artifact: `release/ChengSuan.exe`, **20,174,848 bytes**, version **1.1.0**. SHA256: `863AA8EF06E5645365B463E93A1CCC690E62238943F66F6AF332A694463A1041`.

Final-file smoke verification passed in `verification/investment-1.1.0/session-1791903506278/report.json` with no issues: real executable migration/backup, seven default currencies, real candles/MA, buy, oversell rejection, partial sell accounting, dark theme, backup, shutdown and Chinese-path restart. Measured `devicePixelRatio` was **1.5**. The prior full monitor run remains the evidence for page-switch and minimized refresh. Final PE dependency audit is `verification/dependencies-1.1.0-final.txt`.

The existing production database remained at its original modification time, **2026-10-13 09:40:41**, and 20,480 bytes. Verification did not launch the EXE against this directory; the user's first 1.1.0 launch will perform its backed-up migration.

## 1.0.2 — Currency names (2026-10-13)

- All shared currency selectors now use offline `Intl.DisplayNames('zh-CN')` labels, preserving the stored ISO codes and common-currency ordering.
- Checked the actual display helper against all 162 currencies supported by the development runtime: no missing Chinese names. Examples include HKD 港元, GBP 英镑, AED 阿联酋迪拉姆 and AMD 亚美尼亚德拉姆.
- TypeScript passed; all 16 Vitest tests and 8 offline Rust tests passed. The explicitly network-dependent Rust test was skipped in this run; its prior 1.0.1 live verification is recorded below.
- Tauri Windows x64 `--no-bundle` release built successfully. SHA256: `279600C0921C1B44C743A641BE3190216422248CD8B4152EB5AEB70F52479323`.
- No database schema or user data changes. Currency names were checked programmatically; a complete native dropdown/high-DPI matrix was not repeated.

## 1.0.1 — Exchange-rate fix

Verified on the current Windows development machine, 2026-10-13 (Asia/Shanghai).

## Confirmed

- TypeScript strict typecheck passed; Vitest: 16 tests passed.
- Rust: 8 offline tests passed, covering transactional import, migration safety, persistent reload, consistent backup, directory locking, corrupt/future database rejection and exchange-rate parsing.
- Explicit live Rust regression passed: real Frankfurter USD/CNY response, rate `6.6874`, date `2026-10-13`, 75 expanded source records. The corrected short attribution saved to isolated SQLite storage and reloaded unchanged.
- The subscription regression fixture calculates 20 USD/month as 133.748 CNY/month at that rate before display rounding.
- Windows x64 release built successfully using Tauri `--no-bundle`. Output: `release/ChengSuan.exe`, 18,572,800 bytes.
- SHA256: `4E3A5DF7FB17FEA1EB97B4F21AEA68EFA2AAFE6755E33CAB0A5506DDC8D261E0`.
- PE import audit lists Windows system DLLs, including Windows Universal CRT API sets. No separately shipped SQLite, WebView2Loader or VC runtime DLL is listed. System WebView2 remains required.
- Only the EXE was copied into `verification/portable-1.0.1`. Launching with a different working directory created the database beside the copied EXE and opened the window titled `澄算 ChengSuan`. The test process closed normally.
- npm dependency audit reports zero known vulnerabilities at this verification time.

## Bug fixed

The original code serialized the complete expanded `providers` array into the `provider` label. Normal responses with dozens of source objects exceeded the frontend's 200-character limit, causing successfully retrieved rates to fail validation. The adapter now summarizes the metadata, preserves decimal precision and retains validation. Unexpected schema failures display understandable Chinese messages instead of raw validation JSON.

The patch does not change the database version. Existing `release/ChengSuanData` was not opened or modified by verification.

## Unverified environments

No clean Windows VM, physical USB drive, full filesystem/registry tracing, native import/export-dialog automation, or complete high-DPI matrix was available. Persistence and migrations were verified using real Rust storage tests. Full GUI workflows in those environments require further acceptance testing. Network failure preserves existing exchange-rate snapshots.
