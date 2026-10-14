use super::*;
use parsers::*;
#[test]
fn authorized_crypto_mapping_uses_platform_identity_instead_of_ambiguous_symbol() {
    let mut coin = asset(
        "cg:bitcoin",
        "BTC",
        "比特币",
        "crypto",
        "CRYPTO",
        "coingecko",
        "bitcoin",
        "USD",
    );
    assert_eq!(
        MarketRuntime::all_tick_code(&coin).as_deref(),
        Some("BTCUSDT")
    );
    coin["providerId"] = json!("different-token-with-btc-symbol");
    assert_eq!(MarketRuntime::all_tick_code(&coin), None);
    coin["providerId"] = json!("tether");
    coin["symbol"] = json!("USDT");
    assert_eq!(MarketRuntime::all_tick_code(&coin), None);
}
#[test]
fn incremental_cache_replaces_current_candle_and_never_mixes_adjustment_sources() {
    let old = json!({"source":"A","currency":"CNY","interval":"day","bars":[{"time":1,"close":"10"},{"time":2,"close":"11"}]});
    let mut fresh = json!({"source":"A","currency":"CNY","interval":"day","bars":[{"time":2,"close":"12"},{"time":3,"close":"13"}]});
    merge_candle_cache(&old, &mut fresh);
    assert_eq!(fresh["bars"].as_array().unwrap().len(), 3);
    assert_eq!(fresh["bars"][1]["close"], "12");
    fresh["source"] = json!("other source");
    let before = fresh.clone();
    merge_candle_cache(&old, &mut fresh);
    assert_eq!(fresh, before);
}
#[test]
fn gold_now_is_clock_and_quote_uses_observed_minute() {
    let a = defaults()
        .into_iter()
        .find(|a| a["provider"] == "sge")
        .unwrap();
    let q=sge_quote(&json!({"now":"21:53","delaystr":"2026年10月14日 21:51:54","times":["21:51","21:52","21:53"],"data":["899.12","900.34","0"]}),&a).unwrap();
    assert_eq!(q["price"], "899.12");
    assert_eq!(q["fxDate"], "2026-10-14");
    assert_eq!(q["sourceAt"], "2026-10-13T21:51:54+08:00");
}
#[test]
fn gold_weekday_and_weekend_night_belong_to_trade_day() {
    let (time, day) = gold_calendar_time("2026年10月14日 21:02:03").unwrap();
    assert_eq!(day, "2026-10-14");
    assert_eq!(time.format("%F %T").to_string(), "2026-10-13 21:02:03");
    let (time, day) = gold_calendar_time("2026年10月19日 21:02:03").unwrap();
    assert_eq!(day, "2026-10-19");
    assert_eq!(time.format("%F %T").to_string(), "2026-10-16 21:02:03");
    let (time, _) = gold_calendar_time("2026年10月19日 01:02:03").unwrap();
    assert_eq!(time.format("%F %T").to_string(), "2026-10-17 01:02:03");
    let (time, _) = gold_calendar_time("2026年10月19日 10:02:03").unwrap();
    assert_eq!(time.format("%F %T").to_string(), "2026-10-19 10:02:03");
}
#[test]
fn gbk_quotes_preserve_names_and_numeric_precision() {
    let a = asset(
        "cn:SH:600000",
        "600000",
        "浦发银行",
        "stock",
        "SH",
        "cn",
        "1.600000",
        "CNY",
    );
    let mut columns = vec![""; 33];
    columns[1] = "浦发银行";
    columns[3] = "12.34567890123456789";
    columns[30] = "20261013150000";
    columns[32] = "-0.123456789";
    let line = format!("v_sh600000=\"{}\";", columns.join("~"));
    let (bytes, _, _) = encoding_rs::GBK.encode(&line);
    let q = tencent(&bytes, &[a]);
    assert_eq!(q.len(), 1);
    assert_eq!(q[0]["price"], "12.34567890123456789");
    assert_eq!(q[0]["changePercent"], "-0.123456789");
    assert_eq!(q[0]["sourceAt"], "2026-10-13T15:00:00+08:00");
}
#[test]
fn target_token_direction_chain_and_case_are_checked() {
    let pool = json!({"relationships":{"base_token":{"data":{"id":"eth_0xabc"}},"quote_token":{"data":{"id":"eth_0xdef"}}}});
    assert_eq!(token_side(&pool, "eth", "0xDEF"), Some("quote"));
    assert_eq!(token_side(&pool, "bsc", "0xdef"), None);
    let pool = json!({"relationships":{"base_token":{"data":{"id":"solana_AbC123"}},"quote_token":{"data":{"id":"solana_Def456"}}}});
    assert_eq!(token_side(&pool, "solana", "AbC123"), Some("base"));
    assert_eq!(token_side(&pool, "solana", "abc123"), None);
}
#[test]
fn failed_requests_retain_cached_prices_and_back_off() {
    tauri::async_runtime::block_on(async {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("caches")).unwrap();
        let root = temp.path().to_path_buf();
        let q = quote(
            "cg:bitcoin",
            "123.000000000000000001",
            "USD",
            Some("6.7"),
            "2026-10-13",
            None,
            "fixture",
            "2026-10-13T00:00:00Z",
            "live",
            "",
        );
        std::fs::write(
            root.join("caches/investment-quotes.json"),
            json!([q]).to_string(),
        )
        .unwrap();
        let runtime = MarketRuntime::new(root).unwrap();
        let cfg = investment::default_settings();
        let blocked = chrono::Utc::now().timestamp_millis() + 120000;
        runtime.network.block_for_test("coingecko", blocked);
        let asset = defaults()
            .into_iter()
            .find(|a| a["id"] == "cg:bitcoin")
            .unwrap();
        runtime.refresh(&[asset], &cfg).await.unwrap();
        let value = runtime.snapshot();
        assert_eq!(value["quotes"][0]["price"], "123.000000000000000001");
        assert_eq!(value["quotes"][0]["status"], "cached");
        assert!(!value["errors"].as_array().unwrap().is_empty());
    });
}
#[test]
#[ignore = "requires real free market sources"]
fn live_quotes_stock_gold_and_crypto() {
    tauri::async_runtime::block_on(async {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("caches")).unwrap();
        let runtime = MarketRuntime::new(temp.path().into()).unwrap();
        let assets = vec![
            asset(
                "cn:SH:600000",
                "600000",
                "浦发银行",
                "stock",
                "SH",
                "cn",
                "1.600000",
                "CNY",
            ),
            defaults()
                .into_iter()
                .find(|a| a["provider"] == "sge")
                .unwrap(),
            defaults()
                .into_iter()
                .find(|a| a["id"] == "cg:bitcoin")
                .unwrap(),
        ];
        let (quotes, errors) = runtime
            .free_quotes(&assets, &investment::default_settings())
            .await;
        println!("LIVE_QUOTES {} ISSUES {}", json!(quotes), json!(errors));
        for a in assets {
            assert!(
                quotes.iter().any(|q| q["instrumentId"] == a["id"]),
                "missing {}: {}",
                a["id"],
                json!(errors)
            );
        }
    });
}
#[test]
#[ignore = "requires real full market directory"]
fn live_complete_stock_and_etf_directory() {
    tauri::async_runtime::block_on(async {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("caches")).unwrap();
        let runtime = MarketRuntime::new(temp.path().into()).unwrap();
        let catalog = runtime.download_catalog(None).await.unwrap();
        let stock = catalog.iter().filter(|a| a["kind"] == "stock").count();
        let etf = catalog.iter().filter(|a| a["kind"] == "etf").count();
        let bj = catalog.iter().filter(|a| a["market"] == "BJ").count();
        assert!(stock > 5000 && etf > 1000 && bj > 100);
        assert!(catalog
            .iter()
            .any(|a| a["name"].as_str().is_some_and(|s| s.contains("浦发"))));
        println!("LIVE_COMPLETE_DIRECTORY stocks={stock} ETFs={etf} Beijing={bj}");
    });
}
#[test]
#[ignore = "requires real historical market services"]
fn live_history_and_contract_token_direction() {
    tauri::async_runtime::block_on(async {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("caches")).unwrap();
        let runtime = MarketRuntime::new(temp.path().into()).unwrap();
        let settings = investment::default_settings();
        let token = runtime
            .resolve_token(
                "eth",
                "0xdAC17F958D2ee523a2206206994597C13D831ec7",
                &settings,
            )
            .await
            .unwrap();
        assert_eq!(token["symbol"], "USDT");
        assert!(token["pool"]["tokenSide"] == "base" || token["pool"]["tokenSide"] == "quote");
        println!("LIVE_TOKEN {}", token);
        for a in [
            asset(
                "cn:SH:600000",
                "600000",
                "浦发银行",
                "stock",
                "SH",
                "cn",
                "1.600000",
                "CNY",
            ),
            defaults()
                .into_iter()
                .find(|a| a["provider"] == "sge")
                .unwrap(),
            defaults()
                .into_iter()
                .find(|a| a["id"] == "cg:bitcoin")
                .unwrap(),
            token,
        ] {
            let tf = if a["provider"] == "coingecko" {
                "7d"
            } else {
                "day"
            };
            let bars = runtime.candles(&a, tf, "qfq", &settings).await.unwrap();
            assert!(!bars["bars"].as_array().unwrap().is_empty());
            println!(
                "LIVE_HISTORY {} count={} source={} volume={}",
                a["id"],
                bars["bars"].as_array().unwrap().len(),
                bars["source"],
                bars["volumeUnit"]
            );
        }
    });
}
