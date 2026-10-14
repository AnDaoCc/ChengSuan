use crate::storage::Store;
use bigdecimal::BigDecimal;
use rusqlite::{params, Connection, Transaction};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};
use tauri::State;

pub fn migrate(tx: &Transaction<'_>) -> Result<(), String> {
    tx.execute_batch("CREATE TABLE investment_accounts(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
        CREATE TABLE investment_assets(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
        CREATE TABLE investment_trades(id TEXT PRIMARY KEY,account_id TEXT NOT NULL,instrument_id TEXT NOT NULL,executed_at TEXT NOT NULL,payload TEXT NOT NULL);
        CREATE INDEX investment_trade_account ON investment_trades(account_id,instrument_id,executed_at);
        CREATE TABLE investment_watchlist(id TEXT PRIMARY KEY,ordinal INTEGER NOT NULL);
        CREATE TABLE investment_rules(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
        CREATE TABLE investment_settings(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL,payload TEXT NOT NULL);
        CREATE TABLE investment_alert_states(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
        CREATE TABLE investment_alert_events(id TEXT PRIMARY KEY,triggered_at TEXT NOT NULL,payload TEXT NOT NULL);") .map_err(|e|e.to_string())?;
    let default =
        json!({"id":uuid::Uuid::new_v4().to_string(),"name":"我的投资","kind":"general","note":""});
    tx.execute(
        "INSERT INTO investment_accounts VALUES(?1,?2)",
        params![default["id"].as_str(), default.to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO investment_settings VALUES(1,0,?1)",
        [default_settings().to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn default_settings() -> Value {
    json!({"stockSeconds":30,"cryptoSeconds":60,"goldSeconds":60,"coinGeckoMode":"public","allTickEnabled":false,"allTickTransport":"polling","allTickSubscriptionCap":5,"allTickDailyCap":1000,"coinGeckoMonthlyCap":10000})
}
pub fn rows(db: &Connection, table: &str) -> Result<Vec<Value>, String> {
    let mut stmt = db
        .prepare(&format!("SELECT payload FROM {table} ORDER BY id"))
        .map_err(|e| e.to_string())?;
    let raw = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    raw.map(|row| serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string()))
        .collect()
}
pub fn load(db: &Connection) -> Result<Value, String> {
    let (revision, settings): (i64, String) = db
        .query_row(
            "SELECT revision,payload FROM investment_settings WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut query = db
        .prepare("SELECT id FROM investment_watchlist ORDER BY ordinal")
        .map_err(|e| e.to_string())?;
    let watch: Vec<String> = query
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let doc = json!({"version":1,"revision":revision,"accounts":rows(db,"investment_accounts")?,"instruments":rows(db,"investment_assets")?,"trades":rows(db,"investment_trades")?,"watchlist":watch,"alerts":rows(db,"investment_rules")?,"settings":serde_json::from_str::<Value>(&settings).map_err(|e|e.to_string())?});
    validate(&doc)?;
    Ok(doc)
}
pub fn text<'a>(v: &'a Value, key: &str, max: usize) -> Result<&'a str, String> {
    let s = v
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("缺少 {key}"))?;
    if s.chars().count() > max {
        return Err(format!("{key} 过长"));
    }
    Ok(s)
}
pub fn decimal(v: &Value, key: &str, positive: bool, signed: bool) -> Result<BigDecimal, String> {
    let s = text(v, key, 120)?;
    let n = s.strip_prefix('-').unwrap_or(s);
    let mut dots = 0;
    if (!signed && s.starts_with('-'))
        || n.is_empty()
        || n.starts_with('.')
        || n.ends_with('.')
        || !n.bytes().all(|c| {
            if c == b'.' {
                dots += 1;
                true
            } else {
                c.is_ascii_digit()
            }
        })
        || dots > 1
    {
        return Err(format!("{key} 必须为十进制字符串"));
    }
    let number = BigDecimal::from_str(s).map_err(|_| format!("{key} 无效"))?;
    if positive && number <= BigDecimal::from(0) {
        return Err(format!("{key} 必须大于 0"));
    }
    Ok(number)
}
fn list<'a>(v: &'a Value, key: &str, max: usize) -> Result<&'a Vec<Value>, String> {
    let items = v
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("缺少 {key}"))?;
    if items.len() > max {
        return Err(format!("{key} 数量过多"));
    }
    Ok(items)
}
fn uuid(v: &Value, key: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(text(v, key, 36)?).map_err(|_| format!("{key} ID 无效"))?;
    Ok(())
}
pub fn validate_instrument(v: &Value) -> Result<(), String> {
    if text(v, "id", 300)?.is_empty()
        || text(v, "name", 400)?.is_empty()
        || text(v, "symbol", 100)?.is_empty()
    {
        return Err("标的名称或代码为空".into());
    }
    if !["stock", "etf", "index", "gold", "crypto"].contains(&text(v, "kind", 20)?)
        || !["SH", "SZ", "BJ", "SGE", "CRYPTO"].contains(&text(v, "market", 10)?)
        || !["cn", "sge", "coingecko", "gecko"].contains(&text(v, "provider", 20)?)
    {
        return Err("标的类别或来源无效".into());
    }
    text(v, "providerId", 300)?;
    text(v, "unit", 20)?;
    if !["CNY", "USD", "USDT"].contains(&text(v, "currency", 10)?) {
        return Err("计价币种无效".into());
    }
    if v["goldEtf"].as_bool().is_none() {
        return Err("ETF 类型标记无效".into());
    }
    let pools = list(v, "pools", 100)?;
    for pool in pools {
        if text(pool, "address", 250)?.is_empty()
            || !["base", "quote"].contains(&text(pool, "tokenSide", 10)?)
        {
            return Err("交易池或币种方向无效".into());
        }
        text(pool, "name", 400)?;
        decimal(pool, "liquidity", false, false)?;
    }
    if let Some(pool) = v.get("pool").filter(|p| !p.is_null()) {
        if !pools.contains(pool) {
            return Err("选定交易池不在该币可用池列表中".into());
        }
    }
    if v["provider"] == "cn" {
        let code = text(v, "symbol", 100)?;
        let pid = text(v, "providerId", 300)?;
        if code.len() != 6
            || !code.bytes().all(|b| b.is_ascii_digit())
            || pid != format!("{}.{}", if v["market"] == "SH" { "1" } else { "0" }, code)
        {
            return Err("股票代码与交易所不匹配".into());
        }
    }
    if v["provider"] == "coingecko"
        && !text(v, "providerId", 300)?
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("CoinGecko 平台 ID 无效".into());
    }
    if let Some(purity) = v.get("purity") {
        let n = decimal(&json!({"purity":purity}), "purity", true, false)?;
        if n > BigDecimal::from(1) {
            return Err("黄金纯度不能超过 1".into());
        }
    }
    if v.get("manualPrice").is_some_and(|p| !p.is_null()) {
        decimal(v, "manualPrice", true, false)?;
    }
    if text(v, "provider", 20)? == "gecko" {
        let chain = text(v, "chain", 100)?;
        let address = text(v, "contract", 250)?;
        if chain.is_empty()
            || address.is_empty()
            || !chain
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !address.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err("公链或合约地址无效".into());
        }
    }
    Ok(())
}
pub fn validate(doc: &Value) -> Result<(), String> {
    if doc["version"] != 1 || doc["revision"].as_u64().is_none() {
        return Err("投资记录版本无效".into());
    }
    let account_rows = list(doc, "accounts", 100)?;
    if account_rows.is_empty() {
        return Err("至少保留一个投资账户".into());
    }
    let mut accounts = HashSet::new();
    for a in account_rows {
        uuid(a, "id")?;
        if text(a, "name", 100)?.trim().is_empty()
            || !["general", "securities", "exchange", "wallet", "gold"]
                .contains(&text(a, "kind", 20)?)
        {
            return Err("账户名称或类型无效".into());
        }
        text(a, "note", 1000)?;
        if !accounts.insert(text(a, "id", 36)?) {
            return Err("账户 ID 重复".into());
        }
    }
    let mut assets = HashMap::new();
    for a in list(doc, "instruments", 10000)? {
        validate_instrument(a)?;
        if assets.insert(text(a, "id", 300)?, a).is_some() {
            return Err("标的 ID 重复".into());
        }
    }
    let mut ids = HashSet::new();
    let mut trades = Vec::new();
    for t in list(doc, "trades", 100000)? {
        uuid(t, "id")?;
        if !ids.insert(text(t, "id", 36)?) {
            return Err("交易 ID 重复".into());
        }
        let account = text(t, "accountId", 36)?;
        let instrument = text(t, "instrumentId", 300)?;
        if !accounts.contains(account) || !assets.contains_key(instrument) {
            return Err("交易引用账户或标的不存在".into());
        }
        let side = text(t, "side", 10)?;
        if !["buy", "sell", "opening", "adjust"].contains(&side) {
            return Err("交易类型无效".into());
        }
        let qty = decimal(t, "quantity", side != "adjust", false)?;
        let price = decimal(t, "unitPrice", side == "buy" || side == "sell", false)?;
        let fee = decimal(t, "fee", false, false)?;
        let fx = decimal(t, "fxToCny", true, false)?;
        let currency = text(t, "currency", 10)?;
        if !["CNY", "USD", "USDT"].contains(&currency)
            || (currency == "CNY" && fx != BigDecimal::from(1))
        {
            return Err("成交币种或换算率无效".into());
        }
        chrono::NaiveDate::parse_from_str(text(t, "fxDate", 10)?, "%Y-%m-%d")
            .map_err(|_| "汇率日期无效")?;
        text(t, "fxSource", 200)?;
        let note = text(t, "note", 4000)?;
        if side == "adjust"
            && (currency != "CNY" || fee != BigDecimal::from(0) || note.trim().is_empty())
        {
            return Err("校正需要人民币成本、零费用和依据".into());
        }
        let kind = assets[instrument]["kind"].as_str().unwrap_or("");
        if kind == "index" {
            return Err("指数不能记录买卖，请选择可交易股票或 ETF".into());
        }
        if ["stock", "etf"].contains(&kind) && qty != qty.with_scale(0) {
            return Err("股票和 ETF 数量必须为整数".into());
        }
        let stamp = chrono::DateTime::parse_from_rfc3339(text(t, "executedAt", 100)?)
            .map_err(|_| "成交时间无效")?
            .timestamp_millis();
        let sequence = t["sequence"].as_u64().ok_or("交易顺序无效")?;
        let _ = price;
        trades.push((
            stamp,
            sequence,
            text(t, "id", 36)?,
            account,
            instrument,
            side,
            qty,
        ));
    }
    trades.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    let mut balance: HashMap<(&str, &str), BigDecimal> = HashMap::new();
    for (_, _, _, account, instrument, side, qty) in trades {
        let key = (account, instrument);
        if side == "opening" && balance.contains_key(&key) {
            return Err("起始持仓必须在其他交易之前".into());
        }
        let old = balance
            .get(&key)
            .cloned()
            .unwrap_or_else(|| BigDecimal::from(0));
        let next = match side {
            "sell" => {
                if qty > old {
                    return Err("卖出数量超过当时持仓，未保存".into());
                }
                old - qty
            }
            "adjust" => qty,
            _ => old + qty,
        };
        balance.insert(key, next);
    }
    let mut watches = HashSet::new();
    for id in list(doc, "watchlist", 10000)? {
        let id = id.as_str().ok_or("自选 ID 无效")?;
        if !assets.contains_key(id) || !watches.insert(id) {
            return Err("自选标的不存在或重复".into());
        }
    }
    let mut rules = HashSet::new();
    for rule in list(doc, "alerts", 1000)? {
        uuid(rule, "id")?;
        if !rules.insert(text(rule, "id", 36)?)
            || !assets.contains_key(text(rule, "instrumentId", 300)?)
        {
            return Err("提醒 ID 重复或标的不存在".into());
        }
        if !rule["accountId"].is_null() && !accounts.contains(text(rule, "accountId", 36)?) {
            return Err("提醒账户不存在".into());
        }
        if ![
            "priceAbove",
            "priceBelow",
            "gainAbove",
            "gainBelow",
            "percentAbove",
            "percentBelow",
        ]
        .contains(&text(rule, "kind", 30)?)
        {
            return Err("提醒类型无效".into());
        }
        decimal(rule, "threshold", false, true)?;
        if rule["enabled"].as_bool().is_none()
            || rule["cooldownMinutes"]
                .as_u64()
                .filter(|n| *n >= 1 && *n <= 1440)
                .is_none()
        {
            return Err("提醒冷却设置无效".into());
        }
    }
    let settings = &doc["settings"];
    if settings.get("allTickTransport").is_some()
        && !matches!(
            settings["allTickTransport"].as_str(),
            Some("polling" | "push")
        )
    {
        return Err("AllTick 传输设置无效".into());
    }
    if settings.get("allTickSubscriptionCap").is_some()
        && settings["allTickSubscriptionCap"]
            .as_u64()
            .is_none_or(|cap| !(1..=3000).contains(&cap))
    {
        return Err("AllTick 订阅上限无效".into());
    }
    for (key, min, max) in [
        ("stockSeconds", 15, 3600),
        ("cryptoSeconds", 30, 3600),
        ("goldSeconds", 30, 3600),
        ("allTickDailyCap", 1, 10000000),
        ("coinGeckoMonthlyCap", 1, 10000000),
    ] {
        if settings[key]
            .as_u64()
            .filter(|n| *n >= min && *n <= max)
            .is_none()
        {
            return Err(format!("监控设置 {key} 无效"));
        }
    }
    if !["public", "demo", "pro"].contains(&text(settings, "coinGeckoMode", 10)?)
        || settings["allTickEnabled"].as_bool().is_none()
    {
        return Err("行情授权设置无效".into());
    }
    Ok(())
}
pub fn save(db: &mut Connection, doc: &Value) -> Result<i64, String> {
    validate(doc)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let revision: i64 = tx
        .query_row(
            "SELECT revision FROM investment_settings WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if Some(revision) != doc["revision"].as_i64() {
        return Err("投资数据已变化，请刷新后重试；本次修改未覆盖旧数据".into());
    }
    let old_rules: HashMap<String, Value> = rows(&tx, "investment_rules")?
        .into_iter()
        .map(|v| (v["id"].as_str().unwrap_or("").to_owned(), v))
        .collect();
    tx.execute_batch("DELETE FROM investment_accounts;DELETE FROM investment_assets;DELETE FROM investment_trades;DELETE FROM investment_watchlist;DELETE FROM investment_rules;").map_err(|e|e.to_string())?;
    for (key, table) in [
        ("accounts", "investment_accounts"),
        ("instruments", "investment_assets"),
        ("alerts", "investment_rules"),
    ] {
        for v in doc[key].as_array().unwrap() {
            tx.execute(
                &format!("INSERT INTO {table}(id,payload) VALUES(?1,?2)"),
                params![v["id"].as_str(), v.to_string()],
            )
            .map_err(|e| e.to_string())?;
            if key == "alerts" && old_rules.get(v["id"].as_str().unwrap()) != Some(v) {
                tx.execute(
                    "DELETE FROM investment_alert_states WHERE id=?1",
                    [v["id"].as_str()],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    for v in doc["trades"].as_array().unwrap() {
        tx.execute(
            "INSERT INTO investment_trades VALUES(?1,?2,?3,?4,?5)",
            params![
                v["id"].as_str(),
                v["accountId"].as_str(),
                v["instrumentId"].as_str(),
                v["executedAt"].as_str(),
                v.to_string()
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    for (n, v) in doc["watchlist"].as_array().unwrap().iter().enumerate() {
        tx.execute(
            "INSERT INTO investment_watchlist VALUES(?1,?2)",
            params![v.as_str(), n as i64],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute(
        "DELETE FROM investment_alert_states WHERE id NOT IN (SELECT id FROM investment_rules)",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE investment_settings SET revision=?1,payload=?2 WHERE id=1",
        params![revision + 1, doc["settings"].to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(revision + 1)
}
#[tauri::command]
pub fn investment_load(store: State<Store>) -> Result<Value, String> {
    let db = store.db.lock().map_err(|e| e.to_string())?;
    let document = load(&db)?;
    let states = rows(&db, "investment_alert_states")?;
    let mut query = db
        .prepare("SELECT payload FROM investment_alert_events ORDER BY triggered_at DESC LIMIT 200")
        .map_err(|e| e.to_string())?;
    let events = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .map(|s| {
            serde_json::from_str::<Value>(&s.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"document":document,"alertStates":states,"events":events}))
}
#[tauri::command]
pub fn investment_save(store: State<Store>, document: Value) -> Result<i64, String> {
    let mut db = store.db.lock().map_err(|e| e.to_string())?;
    save(&mut db, &document)
}
#[tauri::command]
pub fn investment_alert_report(
    store: State<Store>,
    states: Value,
    events: Vec<Value>,
) -> Result<(), String> {
    if events.len() > 1000 {
        return Err("提醒事件过多".into());
    }
    let mut db = store.db.lock().map_err(|e| e.to_string())?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    for (id, state) in states.as_object().ok_or("提醒状态无效")? {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM investment_rules WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists {
            let payload = json!({"id":id,"state":state});
            tx.execute("INSERT INTO investment_alert_states VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![id,payload.to_string()]).map_err(|e|e.to_string())?;
        }
    }
    for event in events {
        uuid(&event, "id")?;
        uuid(&event, "ruleId")?;
        text(&event, "message", 1000)?;
        chrono::DateTime::parse_from_rfc3339(text(&event, "triggeredAt", 100)?)
            .map_err(|_| "提醒事件时间无效")?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM investment_rules WHERE id=?1)",
                [event["ruleId"].as_str()],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO investment_alert_events VALUES(?1,?2,?3)",
            params![
                event["id"].as_str(),
                event["triggeredAt"].as_str(),
                event.to_string()
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute("DELETE FROM investment_alert_events WHERE id NOT IN (SELECT id FROM investment_alert_events ORDER BY triggered_at DESC LIMIT 1000)",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}
