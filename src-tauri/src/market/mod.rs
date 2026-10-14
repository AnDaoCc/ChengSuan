pub mod credentials;
mod network;
pub mod parsers;
mod providers;
mod stream;
#[cfg(test)]
mod tests;
use crate::{investment, storage::Store};
use chrono::{Datelike, Timelike};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager, State};

pub struct MarketRuntime {
    pub root: PathBuf,
    pub network: network::Network,
    pub fx_cache: Mutex<HashMap<String, (i64, String, String)>>,
    catalog: Mutex<Vec<Value>>,
    catalog_busy: AtomicBool,
    visible: Mutex<Vec<Value>>,
    quotes: Mutex<HashMap<String, Value>>,
    force: AtomicBool,
    stopped: AtomicBool,
    refresh_gate: tokio::sync::Mutex<()>,
    catalog_stamp: Mutex<String>,
    errors: Mutex<Vec<String>>,
    latest_doc: Mutex<Value>,
    stream_epoch: AtomicU64,
    stream_errors: Mutex<HashMap<String, String>>,
}
pub fn replace_file(temp: &Path, path: &Path) -> std::io::Result<()> {
    crate::replace_export(temp, path)
}
pub fn stock_session(now: chrono::DateTime<chrono::Utc>) -> bool {
    let local = now.with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap());
    let weekday = local.weekday().number_from_monday();
    let minute = local.hour() * 60 + local.minute();
    weekday <= 5 && ((555..=690).contains(&minute) || (780..=930).contains(&minute))
}
impl MarketRuntime {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        let directory: Value = std::fs::read(root.join("caches/investment-directory.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        let mut catalog = parsers::defaults();
        if let Some(items) = directory["instruments"].as_array() {
            catalog.extend(
                items
                    .iter()
                    .filter(|a| investment::validate_instrument(a).is_ok())
                    .cloned(),
            );
        }
        let raw: Vec<Value> = std::fs::read(root.join("caches/investment-quotes.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let quotes = raw
            .into_iter()
            .filter_map(|mut q| {
                let id = q["instrumentId"].as_str()?.to_owned();
                q["status"] = json!("cached");
                Some((id, q))
            })
            .collect();
        let fx_cache = std::fs::read(root.join("caches/investment-fx.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Ok(Self {
            network: network::Network::new(root.clone())?,
            root,
            catalog: Mutex::new(catalog),
            catalog_busy: AtomicBool::new(false),
            visible: Mutex::new(Vec::new()),
            quotes: Mutex::new(quotes),
            force: AtomicBool::new(true),
            stopped: AtomicBool::new(false),
            refresh_gate: tokio::sync::Mutex::new(()),
            fx_cache: Mutex::new(fx_cache),
            catalog_stamp: Mutex::new(directory["date"].as_str().unwrap_or("").into()),
            errors: Mutex::new(Vec::new()),
            latest_doc: Mutex::new(Value::Null),
            stream_epoch: AtomicU64::new(0),
            stream_errors: Mutex::new(HashMap::new()),
        })
    }
    fn persist(&self, name: &str, value: &Value) -> Result<(), String> {
        let file = self.root.join("caches").join(name);
        let temp = file.with_extension("tmp");
        std::fs::write(&temp, value.to_string()).map_err(|e| e.to_string())?;
        replace_file(&temp, &file).map_err(|e| e.to_string())
    }
    pub fn snapshot(&self) -> Value {
        let quotes = self
            .quotes
            .lock()
            .map(|q| q.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut errors = self.errors.lock().map(|e| e.clone()).unwrap_or_default();
        if let Ok(stream) = self.stream_errors.lock() {
            errors.extend(stream.values().cloned());
        }
        json!({"quotes":quotes,"errors":errors,"fetchedAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true)})
    }
    pub fn start(self: &Arc<Self>, app: tauri::AppHandle) {
        self.start_streams(app.clone());
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut last_stock = 0i64;
            let mut last_crypto = 0i64;
            let mut last_gold = 0i64;
            let mut revision = -1i64;
            let mut doc = Value::Null;
            while !runtime.stopped.load(Ordering::Relaxed) {
                let force = runtime.force.swap(false, Ordering::Relaxed);
                let changed = {
                    let store = app.state::<Store>();
                    let db = store.db.lock();
                    if let Ok(db) = db {
                        db.query_row(
                            "SELECT revision FROM investment_settings WHERE id=1",
                            [],
                            |r| r.get::<_, i64>(0),
                        )
                        .ok()
                    } else {
                        None
                    }
                };
                if force || changed != Some(revision) {
                    let loaded = {
                        let store = app.state::<Store>();
                        let db = store.db.lock();
                        db.ok().and_then(|db| investment::load(&db).ok())
                    };
                    if let Some(loaded) = loaded {
                        revision = loaded["revision"].as_i64().unwrap_or(-1);
                        doc = loaded;
                        if let Ok(mut latest) = runtime.latest_doc.lock() {
                            *latest = doc.clone();
                        }
                    }
                }
                if !doc.is_null() {
                    let settings = &doc["settings"];
                    let now = chrono::Utc::now().timestamp();
                    let stock_due = force
                        || stock_session(chrono::Utc::now())
                            && now - last_stock >= settings["stockSeconds"].as_i64().unwrap_or(30);
                    let crypto_due = force
                        || now - last_crypto >= settings["cryptoSeconds"].as_i64().unwrap_or(60);
                    let gold_due =
                        force || now - last_gold >= settings["goldSeconds"].as_i64().unwrap_or(60);
                    let assets = runtime.active(&doc);
                    let selected: Vec<Value> = assets
                        .into_iter()
                        .filter(|a| {
                            if a["provider"] == "cn" {
                                stock_due
                            } else if a["provider"] == "sge" {
                                gold_due
                            } else {
                                crypto_due
                            }
                        })
                        .collect();
                    if !selected.is_empty() {
                        let _ = runtime.refresh(&selected, settings).await;
                        let _ = app.emit("investment:quotes", runtime.snapshot());
                    }
                    if stock_due {
                        last_stock = now;
                    }
                    if crypto_due {
                        last_crypto = now;
                    }
                    if gold_due {
                        last_gold = now;
                    }
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
    }
    fn active(&self, doc: &Value) -> Vec<Value> {
        let mut qty = HashMap::<String, bigdecimal::BigDecimal>::new();
        let mut trades = doc["trades"].as_array().cloned().unwrap_or_default();
        trades.sort_by(|a, b| {
            a["executedAt"]
                .as_str()
                .cmp(&b["executedAt"].as_str())
                .then_with(|| a["sequence"].as_u64().cmp(&b["sequence"].as_u64()))
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        let mut account_qty = HashMap::<(String, String), bigdecimal::BigDecimal>::new();
        for t in trades {
            let Some(q) = t["quantity"]
                .as_str()
                .and_then(|s| s.parse::<bigdecimal::BigDecimal>().ok())
            else {
                continue;
            };
            let key = (
                t["accountId"].as_str().unwrap_or("").to_owned(),
                t["instrumentId"].as_str().unwrap_or("").to_owned(),
            );
            let old = account_qty.get(&key).cloned().unwrap_or_default();
            let next = match t["side"].as_str().unwrap_or("") {
                "sell" => old - q,
                "adjust" => q,
                _ => old + q,
            };
            account_qty.insert(key, next);
        }
        for ((_, asset), count) in account_qty {
            *qty.entry(asset).or_default() += count;
        }
        let watch: HashSet<&str> = doc["watchlist"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut assets = BTreeMap::<String, Value>::new();
        for a in doc["instruments"].as_array().into_iter().flatten() {
            let id = a["id"].as_str().unwrap_or("");
            if watch.contains(id)
                || qty
                    .get(id)
                    .is_some_and(|n| n > &bigdecimal::BigDecimal::from(0))
                || doc["alerts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|r| r["enabled"] == true && r["instrumentId"] == id)
            {
                assets.insert(id.into(), a.clone());
            }
        }
        if let Ok(visible) = self.visible.lock() {
            for a in visible.iter() {
                assets
                    .entry(a["id"].as_str().unwrap_or("").into())
                    .or_insert_with(|| a.clone());
            }
        }
        assets.into_values().collect()
    }
    pub async fn refresh(&self, assets: &[Value], settings: &Value) -> Result<(), String> {
        let _gate = self.refresh_gate.lock().await;
        let mut result = Vec::new();
        let mut errors = Vec::new();
        {
            let mut quotes = self.quotes.lock().map_err(|_| "行情状态错误")?;
            for a in assets.iter().filter(|a| a["provider"] == "gecko") {
                let id = a["id"].as_str().unwrap_or("");
                if quotes
                    .get(id)
                    .is_some_and(|q| q["poolAddress"] != a["pool"]["address"])
                {
                    quotes.remove(id);
                }
            }
        }
        if settings["allTickEnabled"] == true {
            match self.authorized_quotes(assets, settings).await {
                Ok(q) => result.extend(q),
                Err(e) => errors.push(e),
            }
        }
        let remaining: Vec<Value> = assets
            .iter()
            .filter(|a| !result.iter().any(|q| q["instrumentId"] == a["id"]))
            .cloned()
            .collect();
        let (free, issues) = self.free_quotes(&remaining, settings).await;
        result.extend(free);
        errors.extend(issues);
        let received: HashSet<String> = result
            .iter()
            .filter_map(|q| q["instrumentId"].as_str().map(str::to_owned))
            .collect();
        {
            let mut quotes = self.quotes.lock().map_err(|_| "行情状态错误")?;
            for a in assets {
                let id = a["id"].as_str().unwrap_or("");
                if !received.contains(id) {
                    if let Some(q) = quotes.get_mut(id) {
                        q["status"] = json!("cached");
                        q["note"] = json!("本次更新失败，保留最后有效报价");
                    } else {
                        errors.push(format!("{} 暂无有效报价", a["name"].as_str().unwrap_or(id)));
                    }
                }
            }
            for q in result {
                let id = q["instrumentId"].as_str().unwrap_or("");
                let incoming = q["sourceAt"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
                let pushed = quotes
                    .get(id)
                    .filter(|old| old["source"] == "AllTick 授权推送" && old["status"] == "live")
                    .and_then(|old| old["sourceAt"].as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
                if pushed.is_some() && pushed >= incoming {
                    continue;
                }
                quotes.insert(id.into(), q);
            }
            if quotes.len() > 20000 {
                quotes.clear();
                errors.push("行情缓存已达到上限，下一轮重建活跃标的报价".into());
            }
            let saved: Vec<Value> = quotes.values().cloned().collect();
            let _ = self.persist("investment-quotes.json", &json!(saved));
        }
        errors.sort();
        errors.dedup();
        if let Ok(mut out) = self.errors.lock() {
            *out = errors.into_iter().take(30).collect();
        }
        Ok(())
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
    pub fn force_refresh(&self) {
        self.force.store(true, Ordering::Relaxed);
    }
    fn request_catalog(self: &Arc<Self>, app: tauri::AppHandle, force: bool) {
        let today = chrono::Utc::now()
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .format("%Y-%m-%d")
            .to_string();
        if !force && self.catalog_stamp.lock().is_ok_and(|date| *date == today) {
            return;
        }
        if self.catalog_busy.swap(true, Ordering::Relaxed) {
            return;
        }
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = runtime.download_catalog(Some(&app)).await;
            match result {
                Ok(assets) => {
                    let _ = runtime.persist(
                        "investment-directory.json",
                        &json!({"date":today,"instruments":assets,"complete":true}),
                    );
                    let mut full = parsers::defaults();
                    full.extend(assets);
                    if let Ok(mut catalog) = runtime.catalog.lock() {
                        *catalog = full;
                    }
                    if let Ok(mut stamp) = runtime.catalog_stamp.lock() {
                        *stamp = today;
                    }
                    let _ = app.emit(
                        "investment:directory",
                        json!({"busy":false,"message":"股票与 ETF 完整目录已更新"}),
                    );
                }
                Err(error) => {
                    let _ = app.emit(
                        "investment:directory",
                        json!({"busy":false,"message":error}),
                    );
                }
            }
            runtime.catalog_busy.store(false, Ordering::Relaxed);
        });
    }
    pub async fn download_catalog(
        &self,
        app: Option<&tauri::AppHandle>,
    ) -> Result<Vec<Value>, String> {
        let settings = investment::default_settings();
        let mut result = Vec::new();
        for (kind, fs) in [
            ("stock", "m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23,m:0+t:81+s:2048"),
            ("etf", "b:MK0021,b:MK0022,b:MK0023,b:MK0024"),
        ] {
            let mut page = 1;
            let mut expected = 0;
            let mut collected = HashMap::<String, Value>::new();
            loop {
                let data = self
                    .network
                    .json(
                        "https://push2.eastmoney.com/api/qt/clist/get",
                        &[
                            ("pn", page.to_string()),
                            ("pz", "100".into()),
                            ("po", "1".into()),
                            ("np", "1".into()),
                            ("fltt", "2".into()),
                            ("invt", "2".into()),
                            ("fid", "f12".into()),
                            ("fs", fs.into()),
                            ("fields", "f12,f13,f14".into()),
                        ],
                        "catalog",
                        &settings,
                        None,
                        false,
                    )
                    .await?;
                let (total, assets) = parsers::catalogue_page(&data, kind)?;
                if page == 1 {
                    expected = total;
                }
                if assets.is_empty() {
                    break;
                }
                for a in assets {
                    collected.insert(a["id"].as_str().unwrap().into(), a);
                }
                if let Some(app) = app {
                    let _=app.emit("investment:directory",json!({"busy":true,"message":format!("正在更新 {} 目录：{}/{}",if kind=="stock"{"股票"}else{"ETF"},collected.len(),expected)}));
                }
                if collected.len() >= expected {
                    break;
                }
                page += 1;
                if page > 400 {
                    return Err("股票目录分页超出上限，未用不完整数据覆盖旧目录".into());
                }
            }
            if collected.len() != expected {
                return Err(format!(
                    "{} 目录不完整（{}/{}），旧目录保留",
                    kind,
                    collected.len(),
                    expected
                ));
            }
            result.extend(collected.into_values());
        }
        result.sort_by(|a, b| a["symbol"].as_str().cmp(&b["symbol"].as_str()));
        Ok(result)
    }
}
fn settings(app: &tauri::AppHandle) -> Result<Value, String> {
    let store = app.state::<Store>();
    let db = store.db.lock().map_err(|_| "数据库繁忙")?;
    let doc = investment::load(&db)?;
    Ok(doc["settings"].clone())
}
#[tauri::command]
pub async fn market_search(
    app: tauri::AppHandle,
    runtime: State<'_, Arc<MarketRuntime>>,
    category: String,
    query: String,
    page: u32,
    force: bool,
) -> Result<Value, String> {
    if query.chars().count() > 200 {
        return Err("搜索内容过长".into());
    }
    runtime.request_catalog(app.clone(), force);
    let normalized = query.trim().to_lowercase();
    let mut directory = runtime.catalog.lock().map_err(|_| "目录状态错误")?.clone();
    {
        let store = app.state::<Store>();
        let db = store.db.lock().map_err(|_| "数据库繁忙")?;
        let doc = investment::load(&db)?;
        for a in doc["instruments"].as_array().into_iter().flatten() {
            if !directory.iter().any(|saved| saved["id"] == a["id"]) {
                directory.push(a.clone());
            }
        }
    }
    let mut matches = directory
        .iter()
        .filter(|a| {
            let class = match category.as_str() {
                "SH" | "SZ" | "BJ" => a["kind"] == "stock" && a["market"] == category,
                "stocks" => a["kind"] == "stock",
                "etf" => a["kind"] == "etf" && a["goldEtf"] != true,
                "goldEtf" => a["kind"] == "etf" && a["goldEtf"] == true,
                "gold" => a["kind"] == "gold",
                "crypto" => a["kind"] == "crypto",
                "index" => a["kind"] == "index",
                _ => true,
            };
            class
                && (normalized.is_empty()
                    || ["symbol", "name", "id", "providerId", "contract"]
                        .iter()
                        .any(|k| {
                            a[*k]
                                .as_str()
                                .is_some_and(|s| s.to_lowercase().contains(&normalized))
                        }))
        })
        .cloned()
        .collect::<Vec<_>>();
    if category == "crypto" && !normalized.is_empty() {
        let cfg = settings(&app)?;
        match runtime.crypto_search(&query, &cfg).await {
            Ok(remote) => {
                let ids: HashSet<String> = matches
                    .iter()
                    .filter_map(|a| a["id"].as_str().map(str::to_owned))
                    .collect();
                matches.extend(
                    remote
                        .into_iter()
                        .filter(|a| !ids.contains(a["id"].as_str().unwrap_or(""))),
                );
            }
            Err(error) => {
                if matches.is_empty() {
                    return Err(error);
                }
            }
        }
    }
    let total = matches.len();
    let start = (page.max(1) as usize - 1) * 40;
    let items = matches.into_iter().skip(start).take(40).collect::<Vec<_>>();
    Ok(
        json!({"items":items,"total":total,"page":page.max(1),"busy":runtime.catalog_busy.load(Ordering::Relaxed),"catalogDate":runtime.catalog_stamp.lock().map(|d|d.clone()).unwrap_or_default()}),
    )
}
#[tauri::command]
pub fn market_visible(
    runtime: State<Arc<MarketRuntime>>,
    instruments: Vec<Value>,
) -> Result<(), String> {
    if instruments.len() > 100 {
        return Err("可见行情数量过多".into());
    }
    for a in &instruments {
        investment::validate_instrument(a)?;
    }
    let mut visible = runtime.visible.lock().map_err(|_| "行情状态错误")?;
    let changed = *visible != instruments;
    *visible = instruments;
    if changed {
        runtime.force_refresh();
    }
    Ok(())
}
#[tauri::command]
pub fn market_snapshot(runtime: State<Arc<MarketRuntime>>) -> Value {
    runtime.snapshot()
}
#[tauri::command]
pub fn market_refresh(runtime: State<Arc<MarketRuntime>>) {
    runtime.force_refresh();
}
#[tauri::command]
pub async fn market_resolve(
    app: tauri::AppHandle,
    runtime: State<'_, Arc<MarketRuntime>>,
    chain: String,
    contract: String,
) -> Result<Value, String> {
    let cfg = settings(&app)?;
    runtime.resolve_token(&chain, &contract, &cfg).await
}
#[tauri::command]
pub async fn market_fx(
    app: tauri::AppHandle,
    runtime: State<'_, Arc<MarketRuntime>>,
    currency: String,
    date: String,
) -> Result<Value, String> {
    let cfg = settings(&app)?;
    runtime.fx(&currency, &date, &cfg).await
}
#[tauri::command]
pub async fn market_candles(
    app: tauri::AppHandle,
    runtime: State<'_, Arc<MarketRuntime>>,
    instrument: Value,
    timeframe: String,
    adjust: String,
) -> Result<Value, String> {
    investment::validate_instrument(&instrument)?;
    let cfg = settings(&app)?;
    let key = format!(
        "{}|{}|{}|{}|{}",
        instrument, timeframe, adjust, cfg["coinGeckoMode"], cfg["allTickEnabled"]
    );
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let name = format!("investment-bars-{:016x}.json", hasher.finish());
    let cached: Option<Value> = std::fs::read(runtime.root.join("caches").join(&name))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .filter(|v: &Value| v["key"] == key);
    if let Some(c) = &cached {
        if chrono::DateTime::parse_from_rfc3339(c["value"]["fetchedAt"].as_str().unwrap_or(""))
            .is_ok_and(|t| chrono::Utc::now().timestamp() - t.timestamp() < 60)
        {
            return Ok(c["value"].clone());
        }
    }
    let since = if adjust == "none" {
        cached
            .as_ref()
            .and_then(|c| c["value"]["bars"].as_array())
            .and_then(|bars| bars.get(bars.len().saturating_sub(2)))
            .and_then(|b| b["time"].as_i64())
    } else {
        None
    };
    match runtime
        .candles_since(&instrument, &timeframe, &adjust, &cfg, since)
        .await
    {
        Ok(mut value) => {
            if adjust == "none" {
                if let Some(c) = &cached {
                    merge_candle_cache(&c["value"], &mut value);
                }
            }
            let _ = runtime.persist(&name, &json!({"key":key,"value":value}));
            Ok(value)
        }
        Err(error) => {
            if let Some(mut c) = cached {
                c["value"]["cached"] = json!(true);
                c["value"]["cacheReason"] = json!(error);
                Ok(c["value"].clone())
            } else {
                Err(error)
            }
        }
    }
}
fn merge_candle_cache(old: &Value, fresh: &mut Value) {
    if old["source"] != fresh["source"]
        || old["currency"] != fresh["currency"]
        || old["interval"] != fresh["interval"]
    {
        return;
    }
    let mut bars = BTreeMap::new();
    for bar in old["bars"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(fresh["bars"].as_array().into_iter().flatten())
    {
        if let Some(time) = bar["time"].as_i64() {
            bars.insert(time, bar.clone());
        }
    }
    let start = bars.len().saturating_sub(600);
    fresh["bars"] = json!(bars.into_values().skip(start).collect::<Vec<_>>());
}
#[tauri::command]
pub fn market_secret_status(runtime: State<Arc<MarketRuntime>>) -> Value {
    match credentials::read(&runtime.root) {
        Ok(v) => credentials::status(&v),
        Err(e) => json!({"allTick":false,"coinGecko":false,"error":e}),
    }
}
#[tauri::command]
pub fn market_secret_save(
    runtime: State<Arc<MarketRuntime>>,
    patch: Value,
) -> Result<Value, String> {
    let status = credentials::save(&runtime.root, &patch)?;
    runtime.stream_epoch.fetch_add(1, Ordering::Relaxed);
    runtime.force_refresh();
    Ok(status)
}
