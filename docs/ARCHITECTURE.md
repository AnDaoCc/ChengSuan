# ChengSuan architecture

## Boundaries

`app` is the composition root, routing, module registry and shared application service. Feature entry points handle plans, costs, exports and settings. Shared UI uses local shadcn-style primitives and Radix Dialog. Infrastructure owns the desktop repository and serialized autosave queue. `core` only imports TypeScript business types, Zod and decimal.js: it has no React, Tauri or SQLite dependency.

The Rust backend owns SQLite, native dialogs, clipboard, portable directory locking, filesystem access and exchange-rate HTTP requests. UI components do not execute SQL or access another feature's private state. Features consume application services and public repository contracts.

## Persistence and portability

Data is resolved from `current_exe().parent()/ChengSuanData`, never the current working directory. `appDirectoriesOverride` points all Tauri app directories at this dedicated root; WebView2 uses its `webview` subdirectory. Database, WAL/SHM, caches, logs, backups and default exports stay under the root. Write failure is reported, without an AppData fallback or elevation.

An OS file lock prevents simultaneous access to the same directory. SQLite uses WAL, FULL synchronization and transactions. Each plan aggregate and settings document is stored as JSON in TEXT columns; all money, quantities and FX values remain decimal strings. Plan model version is distinct from SQLite `user_version`; migrations preserve prior documents. SQLite `application_id` identifies ChengSuan databases. Future schemas and corrupt files are rejected without replacement.

Backups use SQLite's online backup API, never a raw copy of an active main database. Migration and restore have safety backups. Restores validate the selected snapshot and require confirmation; native export writes to a sibling temporary file before replacing the destination.

## Calculations and exchange rates

The engine validates inputs and returns an immutable CalculationResult with the input snapshot, individual rule explanations, currency conversions, recurring and amortized totals, upfront originals and methodology. Costs are annualized consistently: 365 days, 365/7 weeks, 12 months. Decimal.js uses high precision and formatting applies half-up rounding only at presentation/export boundaries.

The user chooses a plan summary currency; each rule retains its original currency. Each saved rate has source, target, decimal-string rate, reference date, fetch timestamp and provider label. Rates refresh only on user request. A failed request does not replace old snapshots; missing rates mark totals incomplete.

External Frankfurter responses are translated by `src-tauri/src/exchange_rates.rs`. The expanded `providers` array is metadata, not a string field. Its entries are summarized in a short attribution label, preventing oversized JSON metadata from violating the application schema. Arbitrary-precision JSON preserves original numeric lexemes without conversion through f64.

## Export interfaces

All reports consume CalculationResult snapshots, without recalculation. XLSX preserves raw parameters as text and formats display amounts; CSV uses UTF-8 BOM, quoting and formula-injection protection. Project files contain a format version and raw snapshots. Import validates complete documents, previews contents, remaps IDs and inserts new plans transactionally. Project files can preserve incomplete plans for later editing; complete monetary reports require all rates.

PDF/PNG and update functionality remain future interface extensions, without inactive user-facing buttons. Software upgrades replace the exited EXE and keep the data directory.

## Investment module (1.1.0)

`src/core/investment` exposes Instrument, InvestmentAccount, Trade, MarketQuote, Candle, AlertRule and PortfolioResult. It is pure TypeScript and uses decimal.js with 512 significant digits. Prices, quantities, fees and FX are decimal strings through UI, IPC and SQLite. Only chart rendering converts candles to native numbers.

`features/investment/service.ts` is an independent external store. It serializes explicit trade/account/watch/rule writes, validates the entire chronological ledger before committing, and uses revision checks to reject stale replacement documents. A rejected edit leaves the prior document intact and does not poison navigation or later writes. Unsaved trade forms are explicitly identified on close/navigation. Global startup subscribes to quotes and alerts even while the investment page is unmounted. Normal shutdown drains cost autosave and investment writes before destroying the native window.

Buy fees increase basis. Partial sales release quantity × weighted average cost; sell fees reduce realized profit. Full liquidation releases all remaining basis, preventing rounding residue. Opening positions must precede all other trades for an account/asset. Corrections replace quantity/basis, preserve realized profit, and require a recorded reason. Trades freeze the actual CNY conversion basis; USDT uses its own current/historical reference price or an explicit manual conversion. No automatic dividends, splits or transfers are inferred. Holdings exclude external cash balances.

SQLite user_version 3 adds separate accounts, assets, trades, watchlist, rules, settings, alert states and event tables. Payload columns are TEXT. Version 2 migration first takes a SQLite consistent backup and preserves cost plan payloads. Backups include investment records; restore previews account/trade counts, warns for pre-investment backups and reloads both feature stores. Rebuildable quotes, catalogue, candles, FX and request budgets live under `ChengSuanData/caches`. Keys live in `investment-secrets.bin`, protected with Windows DPAPI and never returned to the frontend. Copying to another Windows identity/machine requires key re-entry. Errors never include request URLs or secret headers.

`src-tauri/src/market` is the provider interface and request scheduler: directory search, identity resolution, quotes, candles, historical FX, visibility and secret settings. Native polling collects holdings, watchlist, enabled alerts and visible rows, batches requests and applies source-specific gates, usage caps and exponential backoff. It stops when the native application exits. It uses observed source timestamps and preserves old quotes with an explicit cached status after failure. A missing price or FX stays null in valuation; totals explicitly identify known partial values. Trading hours are weekday reference windows; actual quote timestamps handle stale/holiday data conservatively, without claiming an exchange holiday calendar.

Stock/ETF catalogues are fully paginated and published only when deduplicated row counts equal the provider totals. Chinese names and six-digit exchange-specific IDs are preserved, with GBK decoding for Tencent. Contract identity is chain + address; EVM address casing is normalized, non-EVM casing is retained. Pool relationships must match the target token, and target base/quote direction determines its price and candles. Highest liquidity is selected initially; alternate pools are retained. Pool quotes without an observed source timestamp are delayed reference valuations and do not trigger real-time alerts.

Gold quotes use the SGE minute price array, not the `now` clock label. Source trade-day labels remain visible; weekday/weekend night sessions map to their preceding business night. Holiday-crossing calendar assignments need verification and are described as such. Valuation uses yuan/gram multiplied by purity / 0.9999; brand premium and buyback spread are excluded.

Charts bundle Lightweight Charts locally, with TradingView attribution and Apache license. CN day/week/month and supported minutes keep forward-adjusted and unadjusted data isolated; portfolio prices always use actual unadjusted quotes. CoinGecko interval choices follow its actual public aggregation, and unavailable candle volume remains null. AllTick unadjusted history and CoinGecko Pro daily OHLC are permission dependent. Historical caches preserve source/period/currency and report fallback reason. Alerts evaluate only a new observed, valid live quote; cached, manual and timestamp-less quotes never trigger. Crossing latches and cooldowns persist across restarts.

The existing `.chengsuan` cost-project format remains unchanged. Investment ledger backup/restore uses the complete database, not cost project import/export.

Optional AllTick push uses the official 22004 subscription, 22998 trade tick and 22000/22001 heartbeat protocol, with 10-second heartbeats, source timestamp validation, masked errors and exponential reconnect. Symbol changes replace the subscription list. The default subscription cap is 5 per category, configurable to actual authorization; additional assets retain polling. Connections/subscription changes share the native AllTick daily request budget, while heartbeat control frames are exempt. Changing saved keys invalidates active connections. HTTP results cannot replace a newer live pushed tick. Without supplied authorization this transport is tested with protocol fixtures, not represented as live verified.

AllTick crypto identity is explicitly mapped from confirmed CoinGecko platform IDs, never inferred solely from a symbol. The initial mapped products are BTC, ETH, BNB, SOL, XRP and DOGE; USDT and other IDs keep the identity-aware CoinGecko source. This prevents a different token with a familiar ticker from receiving another token's price or candles.
