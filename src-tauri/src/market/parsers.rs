use bigdecimal::BigDecimal;
use serde_json::{json, Value};
use std::str::FromStr;
pub fn number(v: &Value) -> Option<String> {
    let raw = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if raw.len() > 1000 {
        return None;
    }
    let n = BigDecimal::from_str(&raw).ok()?;
    Some(n.to_plain_string())
}
pub fn positive(v: &Value) -> Option<String> {
    let n = number(v)?;
    if BigDecimal::from_str(&n).ok()? <= BigDecimal::from(0) {
        None
    } else {
        Some(n)
    }
}
pub fn asset(
    id: &str,
    symbol: &str,
    name: &str,
    kind: &str,
    market: &str,
    provider: &str,
    provider_id: &str,
    currency: &str,
) -> Value {
    json!({"id":id,"symbol":symbol,"name":name,"kind":kind,"market":market,"provider":provider,"providerId":provider_id,"currency":currency,"unit":if kind=="stock"{"股"}else if kind=="etf"{"份"}else if kind=="gold"{"克"}else if kind=="index"{"点"}else{"币"},"goldEtf":kind=="etf"&&name.contains("黄金"),"purity":"0.9999","manualPrice":null,"pools":[]})
}
pub fn defaults() -> Vec<Value> {
    let mut assets = Vec::new();
    for (code, name, market, secid) in [
        ("000001", "上证指数", "SH", "1.000001"),
        ("399001", "深证成指", "SZ", "0.399001"),
        ("399006", "创业板指", "SZ", "0.399006"),
        ("000300", "沪深300", "SH", "1.000300"),
        ("000905", "中证500", "SH", "1.000905"),
        ("899050", "北证50", "BJ", "0.899050"),
    ] {
        assets.push(asset(
            &format!("cn:{market}:{code}"),
            code,
            name,
            "index",
            market,
            "cn",
            secid,
            "CNY",
        ));
    }
    assets.push(asset(
        "gold:au9999",
        "Au99.99",
        "实物黄金参考 · Au99.99",
        "gold",
        "SGE",
        "sge",
        "Au99.99",
        "CNY",
    ));
    for (id, symbol, name) in [
        ("bitcoin", "BTC", "比特币"),
        ("ethereum", "ETH", "以太坊"),
        ("tether", "USDT", "泰达币"),
        ("binancecoin", "BNB", "币安币"),
        ("solana", "SOL", "索拉纳"),
        ("ripple", "XRP", "瑞波币"),
        ("dogecoin", "DOGE", "狗狗币"),
    ] {
        assets.push(asset(
            &format!("cg:{id}"),
            symbol,
            name,
            "crypto",
            "CRYPTO",
            "coingecko",
            id,
            "USD",
        ));
    }
    assets
}
pub fn catalogue_page(v: &Value, kind: &str) -> Result<(usize, Vec<Value>), String> {
    let data = v
        .get("data")
        .filter(|v| !v.is_null())
        .ok_or("股票目录响应缺少数据")?;
    let total = data["total"].as_u64().ok_or("股票目录缺少总数")? as usize;
    let diff = data["diff"].as_array().ok_or("股票目录列表格式错误")?;
    let mut result = Vec::new();
    for row in diff {
        let code = row["f12"].as_str().ok_or("股票目录代码无效")?;
        let name = row["f14"].as_str().ok_or("股票目录名称无效")?;
        let num = row["f13"].as_u64().ok_or("交易所字段无效")?;
        let market = if num == 1 {
            "SH"
        } else if code.starts_with("92") || code.starts_with('8') || code.starts_with('4') {
            "BJ"
        } else {
            "SZ"
        };
        result.push(asset(
            &format!("cn:{market}:{code}"),
            code,
            name,
            kind,
            market,
            "cn",
            &format!("{num}.{code}"),
            "CNY",
        ));
    }
    Ok((total, result))
}
pub fn quote(
    id: &str,
    price: &str,
    currency: &str,
    fx: Option<&str>,
    fx_date: &str,
    percent: Option<String>,
    source: &str,
    source_at: &str,
    status: &str,
    note: &str,
) -> Value {
    json!({"instrumentId":id,"price":price,"currency":currency,"cnyRate":fx,"fxDate":fx_date,"changePercent":percent,"source":source,"sourceAt":source_at,"fetchedAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"status":status,"note":note})
}
pub fn tencent(bytes: &[u8], assets: &[Value]) -> Vec<Value> {
    let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
    let mut result = Vec::new();
    for line in decoded.lines() {
        let Some((prefix, payload)) = line.split_once('=') else {
            continue;
        };
        let fields: Vec<&str> = payload
            .trim()
            .trim_matches(';')
            .trim_matches('"')
            .split('~')
            .collect();
        if fields.len() < 33 {
            continue;
        }
        let code = prefix.trim_start_matches("v_");
        let Some(asset) = assets.iter().find(|a| {
            format!(
                "{}{}",
                a["market"].as_str().unwrap_or("").to_lowercase(),
                a["symbol"].as_str().unwrap_or("")
            ) == code
        }) else {
            continue;
        };
        let Some(price) = positive(&json!(fields[3])) else {
            continue;
        };
        let stamp = chrono::NaiveDateTime::parse_from_str(fields[30], "%Y%m%d%H%M%S")
            .ok()
            .and_then(|n| {
                n.and_local_timezone(chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                    .single()
            });
        let Some(stamp) = stamp else { continue };
        let now = chrono::Utc::now();
        let age = now.timestamp() - stamp.timestamp();
        let status = if super::stock_session(now) && (-120..=180).contains(&age) {
            "live"
        } else if super::stock_session(now) {
            "delayed"
        } else {
            "closed"
        };
        result.push(quote(
            asset["id"].as_str().unwrap(),
            &price,
            "CNY",
            Some("1"),
            &stamp.format("%Y-%m-%d").to_string(),
            number(&json!(fields[32])),
            "腾讯公开参考行情",
            &stamp.to_rfc3339(),
            status,
            "免费参考行情，非交易所授权直连",
        ));
    }
    result
}
pub fn gold_calendar_time(
    label: &str,
) -> Result<(chrono::DateTime<chrono::FixedOffset>, String), String> {
    use chrono::{Datelike, Timelike};
    let labelled = chrono::NaiveDateTime::parse_from_str(label, "%Y年%m月%d日 %H:%M:%S")
        .map_err(|_| "黄金报价时间格式无效")?;
    let trade_day = labelled.date().to_string();
    let mut day = labelled.date();
    if labelled.hour() >= 19 || labelled.hour() < 3 {
        day -= chrono::Duration::days(1);
        while day.weekday().number_from_monday() > 5 {
            day -= chrono::Duration::days(1);
        }
        if labelled.hour() < 3 {
            day += chrono::Duration::days(1);
        }
    }
    let stamp = day
        .and_time(labelled.time())
        .and_local_timezone(chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .single()
        .ok_or("黄金报价时间无效")?;
    Ok((stamp, trade_day))
}
pub fn sge_quote(v: &Value, asset: &Value) -> Result<Value, String> {
    let label = v["delaystr"].as_str().ok_or("黄金报价时间缺失")?;
    let (stamp, trade_day) = gold_calendar_time(label)?;
    // SGE `now` is a clock label, not a price. Read the observed minute in its price series.
    let minute = stamp.format("%H:%M").to_string();
    let index = v["times"]
        .as_array()
        .and_then(|times| times.iter().position(|time| time.as_str() == Some(&minute)))
        .ok_or("黄金行情没有对应报价时间")?;
    let price = v["data"]
        .as_array()
        .and_then(|prices| prices.get(index))
        .and_then(positive)
        .ok_or("黄金报价缺失或为 0")?;
    let age = chrono::Utc::now().timestamp() - stamp.timestamp();
    let status = if (-120..=180).contains(&age) {
        "live"
    } else {
        "delayed"
    };
    Ok(quote(asset["id"].as_str().unwrap(),&price,"CNY",Some("1"),&trade_day,None,"上海黄金交易所 Au99.99",&stamp.to_rfc3339(),status,&format!("元/克；源交易日 {trade_day}，标签 {label}；夜盘按前交易工作日晚间映射，节假日跨期需核对；实物黄金参考估值")))
}
pub fn token_side(pool: &Value, chain: &str, address: &str) -> Option<&'static str> {
    let own = format!("{chain}_{address}");
    let matches = |id: &str| {
        if address.starts_with("0x") {
            id.eq_ignore_ascii_case(&own)
        } else {
            id == own
        }
    };
    let base = pool["relationships"]["base_token"]["data"]["id"]
        .as_str()
        .unwrap_or("");
    let other = pool["relationships"]["quote_token"]["data"]["id"]
        .as_str()
        .unwrap_or("");
    if matches(base) {
        Some("base")
    } else if matches(other) {
        Some("quote")
    } else {
        None
    }
}
pub fn candle(
    time: i64,
    open: &Value,
    high: &Value,
    low: &Value,
    close: &Value,
    volume: &Value,
) -> Option<Value> {
    let o = positive(open)?;
    let h = positive(high)?;
    let l = positive(low)?;
    let c = positive(close)?;
    let high = BigDecimal::from_str(&h).ok()?;
    let low = BigDecimal::from_str(&l).ok()?;
    let op = BigDecimal::from_str(&o).ok()?;
    let cl = BigDecimal::from_str(&c).ok()?;
    if high < low || high < op || high < cl || low > op || low > cl {
        return None;
    }
    Some(json!({"time":time,"open":o,"high":h,"low":l,"close":c,"volume":number(volume)}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_numbers_and_classification() {
        assert_eq!(
            number(&serde_json::from_str::<Value>("0.000000000000123456789").unwrap()).unwrap(),
            "0.000000000000123456789"
        );
        assert_eq!(number(&json!("1.23e-8")).unwrap(), "0.0000000123");
        let (_,a)=catalogue_page(&json!({"data":{"total":2,"diff":[{"f12":"920001","f13":0,"f14":"北交所测试"},{"f12":"518880","f13":1,"f14":"黄金ETF华安"}]}}),"etf").unwrap();
        assert_eq!(a[0]["market"], "BJ");
        assert_eq!(a[1]["goldEtf"], true);
    }
    #[test]
    fn no_fake_zero_candles() {
        assert!(candle(1, &json!(0), &json!(2), &json!(1), &json!(2), &Value::Null).is_none());
        assert!(candle(1, &json!(3), &json!(2), &json!(1), &json!(2), &Value::Null).is_none());
    }
}
