use serde_json::Value;
use std::collections::HashSet;
fn text<'a>(v: &'a Value, key: &str, max: usize) -> Result<&'a str, String> {
    let s = v
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("缺少文本字段 {key}"))?;
    if s.len() > max {
        return Err(format!("字段 {key} 过长"));
    }
    Ok(s)
}
fn number(v: &Value, key: &str, positive: bool) -> Result<(), String> {
    let s = text(v, key, 80)?;
    let mut dots = 0;
    if s.is_empty()
        || !s.bytes().all(|b| {
            if b == b'.' {
                dots += 1;
                true
            } else {
                b.is_ascii_digit()
            }
        })
        || dots > 1
        || s.starts_with('.')
        || s.ends_with('.')
    {
        return Err(format!("{key} 必须是十进制字符串"));
    }
    if positive && !s.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        return Err(format!("{key} 必须大于 0"));
    }
    Ok(())
}
fn currency(v: &Value, key: &str) -> Result<(), String> {
    let s = text(v, key, 3)?;
    if s.len() != 3 || !s.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err("币种无效".into());
    }
    Ok(())
}
fn period(v: &Value) -> Result<(), String> {
    if !["day", "week", "month", "year"].contains(&text(v, "period", 10)?) {
        return Err("周期无效".into());
    }
    Ok(())
}
fn date(v: &Value, key: &str) -> Result<(), String> {
    chrono::NaiveDate::parse_from_str(text(v, key, 10)?, "%Y-%m-%d")
        .map_err(|_| "日期无效".to_string())?;
    Ok(())
}
fn uuid(v: &Value, key: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(text(v, key, 36)?).map_err(|_| "ID 无效".to_string())?;
    Ok(())
}
pub fn validate_plan(p: &Value) -> Result<(), String> {
    if p.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err("不支持的方案版本".into());
    }
    uuid(p, "id")?;
    if text(p, "name", 800)?.trim().is_empty() {
        return Err("方案名称不能为空".into());
    }
    text(p, "description", 16000)?;
    currency(p, "currency")?;
    for key in ["createdAt", "updatedAt"] {
        chrono::DateTime::parse_from_rfc3339(text(p, key, 100)?)
            .map_err(|_| "时间格式无效".to_string())?;
    }
    let items = p
        .get("items")
        .and_then(Value::as_array)
        .ok_or("缺少成本项目")?;
    if items.len() > 10000 {
        return Err("成本项目过多".into());
    }
    let mut ids = HashSet::new();
    for i in items {
        uuid(i, "id")?;
        if !ids.insert(text(i, "id", 36)?) {
            return Err("成本项目 ID 重复".into());
        }
        if text(i, "name", 800)?.trim().is_empty() {
            return Err("项目名称不能为空".into());
        }
        text(i, "note", 16000)?;
        currency(i, "currency")?;
        match text(i, "kind", 20)? {
            "fixed" => {
                number(i, "amount", false)?;
                period(i)?;
            }
            "usage" => {
                for key in ["unitPrice", "quantity", "count", "extraPercent"] {
                    number(i, key, false)?;
                }
                number(i, "billingQuantity", true)?;
                period(i)?;
                if text(i, "unit", 320)?.trim().is_empty() {
                    return Err("单位不能为空".into());
                }
                if !["use", "output"].contains(&text(i, "quantityMode", 10)?)
                    || !["proportional", "ceil"].contains(&text(i, "billing", 20)?)
                {
                    return Err("用量计费规则无效".into());
                }
            }
            "oneTime" => {
                number(i, "amount", false)?;
                date(i, "date")?;
                let a = i.get("amortization").ok_or("缺少分摊配置")?;
                if !a.is_null() {
                    number(a, "duration", true)?;
                    period(a)?;
                }
            }
            _ => return Err("未知成本规则".into()),
        }
    }
    let rates = p
        .get("rates")
        .and_then(Value::as_array)
        .ok_or("缺少汇率列表")?;
    if rates.len() > 300 {
        return Err("汇率过多".into());
    }
    let mut sources = HashSet::new();
    for r in rates {
        currency(r, "source")?;
        currency(r, "target")?;
        number(r, "rate", true)?;
        date(r, "date")?;
        if r["target"] != p["currency"] || !sources.insert(text(r, "source", 3)?) {
            return Err("汇率目标不匹配或重复".into());
        }
        chrono::DateTime::parse_from_rfc3339(text(r, "fetchedAt", 100)?)
            .map_err(|_| "汇率时间无效".to_string())?;
        if text(r, "provider", 800)?.is_empty() {
            return Err("缺少汇率来源".into());
        }
    }
    Ok(())
}
pub fn validate_settings(v: &Value) -> Result<(), String> {
    currency(v, "defaultCurrency")?;
    for (k, options) in [
        ("theme", vec!["light", "dark", "system"]),
        ("lastFormat", vec!["text", "xlsx", "csv", "chengsuan"]),
        ("lastScope", vec!["current", "all"]),
        ("lastDetail", vec!["summary", "full"]),
    ] {
        if !options.contains(&text(v, k, 30)?) {
            return Err(format!("设置 {k} 无效"));
        }
    }
    let precision = v.get("precision").ok_or("缺少精度设置")?;
    if !precision.is_null() && precision.as_u64().filter(|x| *x <= 8).is_none() {
        return Err("显示精度必须为 0 到 8".into());
    }
    Ok(())
}
