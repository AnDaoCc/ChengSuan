use serde_json::{json, Value};

/// Convert the external response into the small, stable snapshot used by the app.
/// Expanded provider metadata is an array of objects, not a display label.
pub fn parse_response(
    data: &[u8],
    source: &str,
    target: &str,
    fetched_at: &str,
) -> Result<Value, String> {
    let rows: Value = serde_json::from_slice(data).map_err(|_| "汇率响应格式错误")?;
    let row = rows
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["base"] == source && row["quote"] == target)
        })
        .ok_or_else(|| format!("汇率服务没有返回 {source}/{target}"))?;
    // arbitrary_precision keeps the original decimal lexeme, without an f64 roundtrip.
    let rate = row["rate"]
        .as_number()
        .ok_or("汇率必须是十进制数")?
        .to_string();
    let date = row["date"].as_str().ok_or("缺少汇率日期")?;
    let count = row
        .get("providers")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let provider = if count > 0 {
        format!("Frankfurter / 综合参考汇率（{count} 个来源）")
    } else {
        "Frankfurter / 参考汇率".to_string()
    };
    Ok(
        json!({"source":source,"target":target,"rate":rate,"date":date,"fetchedAt":fetched_at,"provider":provider}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_provider_objects_do_not_overflow_the_snapshot_label() {
        let providers: Vec<Value> = (0..100)
            .map(|i| json!({"key":format!("BANK{i}"),"date":"2026-10-13","rate":6.6874}))
            .collect();
        let response = json!([{"base":"USD","quote":"CNY","rate":6.6874,"date":"2026-10-13","providers":providers}]).to_string();
        assert!(response.len() > 200);
        let rate =
            parse_response(response.as_bytes(), "USD", "CNY", "2026-10-13T00:00:00Z").unwrap();
        assert_eq!(rate["provider"], "Frankfurter / 综合参考汇率（100 个来源）");
        assert!(rate["provider"].as_str().unwrap().chars().count() <= 200);
        assert_eq!(rate["rate"], "6.6874");
    }
    #[test]
    fn preserves_decimal_precision_and_handles_missing_provider_expansion() {
        let response =
            br#"[{"base":"USD","quote":"CNY","rate":6.123456789123456789,"date":"2026-10-13"}]"#;
        let rate = parse_response(response, "USD", "CNY", "2026-10-13T00:00:00Z").unwrap();
        assert_eq!(rate["rate"], "6.123456789123456789");
        assert_eq!(rate["provider"], "Frankfurter / 参考汇率");
    }
    #[test]
    fn rejects_wrong_currency_pairs_and_invalid_responses() {
        assert!(parse_response(b"not json", "USD", "CNY", "2026-10-13T00:00:00Z").is_err());
        assert!(parse_response(
            br#"[{"base":"EUR","quote":"CNY","rate":7,"date":"2026-10-13"}]"#,
            "USD",
            "CNY",
            "2026-10-13T00:00:00Z"
        )
        .is_err());
    }
}
