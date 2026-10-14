use super::{
    parsers::{self, candle, number, positive, quote},
    MarketRuntime,
};
use serde_json::{json, Value};

impl MarketRuntime {
    pub async fn cg(
        &self,
        path: &str,
        params: &[(&str, String)],
        settings: &Value,
    ) -> Result<Value, String> {
        let mode = settings["coinGeckoMode"].as_str().unwrap_or("public");
        if mode == "public" {
            return self
                .network
                .json(
                    &format!("https://api.coingecko.com/api/v3{path}"),
                    params,
                    "coingecko",
                    settings,
                    None,
                    false,
                )
                .await;
        }
        let secret = super::credentials::read(&self.root)?;
        let key = secret["coinGecko"]
            .as_str()
            .filter(|k| !k.is_empty())
            .ok_or("请先填写 CoinGecko 行情密钥")?;
        self.network
            .json(
                &format!(
                    "https://{}-api.coingecko.com/api/v3{path}",
                    if mode == "pro" { "pro" } else { "" }
                )
                .replace("https://-api.", "https://api."),
                params,
                "coingecko-keyed",
                settings,
                Some((
                    if mode == "pro" {
                        "x-cg-pro-api-key"
                    } else {
                        "x-cg-demo-api-key"
                    },
                    key,
                )),
                false,
            )
            .await
    }
    pub async fn gecko(
        &self,
        path: &str,
        params: &[(&str, String)],
        settings: &Value,
    ) -> Result<Value, String> {
        if settings["coinGeckoMode"].as_str().unwrap_or("public") != "public" {
            return self.cg(&format!("/onchain{path}"), params, settings).await;
        }
        self.network
            .json(
                &format!("https://api.geckoterminal.com/api/v2{path}"),
                params,
                "gecko",
                settings,
                None,
                false,
            )
            .await
    }
    pub async fn crypto_search(&self, query: &str, settings: &Value) -> Result<Vec<Value>, String> {
        let base = query.split(['/', '-']).next().unwrap_or(query).trim();
        let response = self
            .cg("/search", &[("query", base.into())], settings)
            .await?;
        let mut assets = Vec::new();
        for coin in response["coins"]
            .as_array()
            .ok_or("币种搜索响应无效")?
            .iter()
            .take(100)
        {
            let Some(id) = coin["id"].as_str() else {
                continue;
            };
            let symbol = coin["symbol"].as_str().unwrap_or(id);
            let name = coin["name"].as_str().unwrap_or(symbol);
            assets.push(parsers::asset(
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
        Ok(assets)
    }
    pub async fn resolve_token(
        &self,
        chain: &str,
        contract: &str,
        settings: &Value,
    ) -> Result<Value, String> {
        if !chain
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !contract.bytes().all(|b| b.is_ascii_alphanumeric())
            || chain.is_empty()
            || chain.len() > 100
            || contract.is_empty()
            || contract.len() > 250
        {
            return Err("请选择公链并填写有效合约地址".into());
        }
        let address = if contract.starts_with("0x") {
            contract.to_lowercase()
        } else {
            contract.to_string()
        };
        if address.starts_with("0x")
            && (address.len() != 42 || !address[2..].bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("EVM 合约地址需要 0x 加 40 位十六进制字符".into());
        }
        let response = self
            .gecko(
                &format!("/networks/{chain}/tokens/{address}"),
                &[],
                settings,
            )
            .await?;
        let attrs = &response["data"]["attributes"];
        let actual = attrs["address"]
            .as_str()
            .ok_or("该公链没有返回可识别的代币")?;
        let identity = format!("{chain}_{address}");
        let returned_id = response["data"]["id"].as_str().ok_or("代币公链身份缺失")?;
        if !(returned_id == identity
            || (address.starts_with("0x") && returned_id.eq_ignore_ascii_case(&identity)))
        {
            return Err("返回代币公链或合约身份不匹配".into());
        }
        if actual != address && !(actual.starts_with("0x") && actual.eq_ignore_ascii_case(&address))
        {
            return Err("返回合约地址不匹配，未添加".into());
        }
        let symbol = attrs["symbol"].as_str().ok_or("代币符号缺失")?;
        let name = attrs["name"].as_str().unwrap_or(symbol);
        let mut asset = parsers::asset(
            &format!("chain:{chain}:{address}"),
            symbol,
            name,
            "crypto",
            "CRYPTO",
            "gecko",
            &address,
            "USD",
        );
        asset["chain"] = json!(chain);
        asset["contract"] = json!(address);
        if let Some(n) = attrs["decimals"].as_u64() {
            asset["decimals"] = json!(n.min(255));
        }
        let pools = self
            .gecko(
                &format!("/networks/{chain}/tokens/{address}/pools"),
                &[("page", "1".into())],
                settings,
            )
            .await?;
        let mut selected = Vec::new();
        for pool in pools["data"]
            .as_array()
            .ok_or("代币交易池数据格式错误")?
            .iter()
            .take(30)
        {
            let a = &pool["attributes"];
            let Some(pool_address) = a["address"].as_str() else {
                continue;
            };
            let Some(side) = parsers::token_side(pool, chain, &address) else {
                continue;
            };
            let liquidity = number(&a["reserve_in_usd"])
                .and_then(|s| s.parse::<bigdecimal::BigDecimal>().ok())
                .map(|v| v.with_scale(8).to_plain_string())
                .unwrap_or_else(|| "0".into());
            selected.push(json!({"address":pool_address,"name":a["name"].as_str().unwrap_or(pool_address),"liquidity":liquidity,"tokenSide":side}));
        }
        selected.sort_by(|a, b| {
            let l = a["liquidity"]
                .as_str()
                .unwrap_or("0")
                .parse::<bigdecimal::BigDecimal>()
                .unwrap_or_default();
            let r = b["liquidity"]
                .as_str()
                .unwrap_or("0")
                .parse::<bigdecimal::BigDecimal>()
                .unwrap_or_default();
            r.cmp(&l)
        });
        asset["pools"] = json!(selected);
        if let Some(pool) = selected.first() {
            asset["pool"] = pool.clone();
        }
        Ok(asset)
    }
    pub async fn fx(&self, currency: &str, date: &str, settings: &Value) -> Result<Value, String> {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "换算日期无效")?;
        if currency == "CNY" {
            return Ok(json!({"rate":"1","date":date,"source":"人民币"}));
        }
        if currency == "USD" {
            let data = self
                .network
                .json(
                    "https://api.frankfurter.dev/v2/rates",
                    &[
                        ("base", "USD".into()),
                        ("quotes", "CNY".into()),
                        ("date", date.into()),
                    ],
                    "fx",
                    settings,
                    None,
                    false,
                )
                .await?;
            let row = data
                .as_array()
                .and_then(|v| v.first())
                .ok_or("该日期没有美元参考汇率，可手动填写")?;
            return Ok(
                json!({"rate":positive(&row["rate"]).ok_or("参考汇率无效")?,"date":row["date"],"source":"Frankfurter 参考汇率"}),
            );
        }
        if currency == "USDT" {
            let today = chrono::Utc::now()
                .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                .format("%Y-%m-%d")
                .to_string();
            if date == today {
                let data = self
                    .cg(
                        "/simple/price",
                        &[
                            ("ids", "tether".into()),
                            ("vs_currencies", "cny".into()),
                            ("precision", "full".into()),
                        ],
                        settings,
                    )
                    .await?;
                return Ok(
                    json!({"rate":positive(&data["tether"]["cny"]).ok_or("USDT 当前人民币价格缺失")?,"date":date,"source":"CoinGecko USDT/CNY 参考价"}),
                );
            }
            let date_value = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .format("%d-%m-%Y")
                .to_string();
            let data = self
                .cg(
                    "/coins/tether/history",
                    &[("date", date_value), ("localization", "false".into())],
                    settings,
                )
                .await?;
            return Ok(
                json!({"rate":positive(&data["market_data"]["current_price"]["cny"]).ok_or("该日期的 USDT 人民币价不可用，请手动填写成交换算依据")?,"date":date,"source":"CoinGecko USDT/CNY 历史参考价"}),
            );
        }
        Err("成交币种仅支持 CNY、USD、USDT".into())
    }
    pub async fn current_fx(&self, currency: &str, settings: &Value) -> Option<(String, String)> {
        let now = chrono::Utc::now().timestamp();
        if let Ok(cache) = self.fx_cache.lock() {
            if let Some((saved, rate, date)) = cache.get(currency) {
                if now - *saved < if currency == "USDT" { 60 } else { 3600 } {
                    return Some((rate.clone(), date.clone()));
                }
            }
        }
        let date = chrono::Utc::now()
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .format("%Y-%m-%d")
            .to_string();
        match self.fx(currency, &date, settings).await {
            Ok(value) => {
                let rate = value["rate"].as_str()?.to_owned();
                let date = value["date"].as_str()?.to_owned();
                if let Ok(mut cache) = self.fx_cache.lock() {
                    cache.insert(currency.into(), (now, rate.clone(), date.clone()));
                    let _ = std::fs::write(
                        self.root.join("caches/investment-fx.json"),
                        serde_json::to_vec(&*cache).unwrap_or_default(),
                    );
                }
                Some((rate, date))
            }
            Err(_) => self
                .fx_cache
                .lock()
                .ok()
                .and_then(|c| c.get(currency).map(|(_, r, d)| (r.clone(), d.clone()))),
        }
    }
    pub async fn free_quotes(
        &self,
        assets: &[Value],
        settings: &Value,
    ) -> (Vec<Value>, Vec<String>) {
        let mut quotes = Vec::new();
        let mut errors = Vec::new();
        let stocks: Vec<Value> = assets
            .iter()
            .filter(|a| a["provider"] == "cn")
            .cloned()
            .collect();
        for chunk in stocks.chunks(50) {
            let codes = chunk
                .iter()
                .map(|a| {
                    format!(
                        "{}{}",
                        a["market"].as_str().unwrap_or("").to_lowercase(),
                        a["symbol"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            match self
                .network
                .bytes(
                    "https://qt.gtimg.cn/",
                    &[("q", codes)],
                    "cn",
                    settings,
                    None,
                    false,
                )
                .await
            {
                Ok(bytes) => quotes.extend(parsers::tencent(&bytes, chunk)),
                Err(e) => errors.push(e),
            }
        }
        // Missing quotes (including a provider's unsupported Beijing codes) use an independent reference source.
        for a in &stocks {
            if quotes.iter().any(|q| q["instrumentId"] == a["id"]) {
                continue;
            }
            match self
                .network
                .json(
                    "https://push2.eastmoney.com/api/qt/stock/get",
                    &[
                        ("secid", a["providerId"].as_str().unwrap_or("").into()),
                        ("fltt", "2".into()),
                        ("fields", "f43,f57,f58,f59,f60,f169,f170,f86".into()),
                    ],
                    "cn",
                    settings,
                    None,
                    false,
                )
                .await
            {
                Ok(data) => {
                    let row = &data["data"];
                    if let Some(price) = positive(&row["f43"]) {
                        let time = row["f86"]
                            .as_i64()
                            .and_then(|t| chrono::DateTime::from_timestamp(t, 0));
                        if let Some(time) = time {
                            let age = chrono::Utc::now().timestamp() - time.timestamp();
                            let state = if super::stock_session(chrono::Utc::now()) {
                                if (-120..=180).contains(&age) {
                                    "live"
                                } else {
                                    "delayed"
                                }
                            } else {
                                "closed"
                            };
                            quotes.push(quote(
                                a["id"].as_str().unwrap(),
                                &price,
                                "CNY",
                                Some("1"),
                                &time.format("%Y-%m-%d").to_string(),
                                number(&row["f170"]),
                                "东方财富公开参考行情",
                                &time.to_rfc3339(),
                                state,
                                "报价以供应商实际时间为准",
                            ));
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        let coins: Vec<&Value> = assets
            .iter()
            .filter(|a| a["provider"] == "coingecko")
            .collect();
        if !coins.is_empty() {
            let ids = coins
                .iter()
                .filter_map(|a| a["providerId"].as_str())
                .collect::<Vec<_>>()
                .join(",");
            match self
                .cg(
                    "/simple/price",
                    &[
                        ("ids", ids),
                        ("vs_currencies", "usd".into()),
                        ("include_24hr_change", "true".into()),
                        ("include_last_updated_at", "true".into()),
                        ("precision", "full".into()),
                    ],
                    settings,
                )
                .await
            {
                Ok(data) => {
                    let fx = self.current_fx("USD", settings).await;
                    for a in coins {
                        let row = &data[a["providerId"].as_str().unwrap_or("")];
                        let Some(price) = positive(&row["usd"]) else {
                            continue;
                        };
                        let Some(stamp) = row["last_updated_at"]
                            .as_i64()
                            .and_then(|n| chrono::DateTime::from_timestamp(n, 0))
                        else {
                            continue;
                        };
                        let age = chrono::Utc::now().timestamp() - stamp.timestamp();
                        quotes.push(quote(
                            a["id"].as_str().unwrap(),
                            &price,
                            "USD",
                            fx.as_ref().map(|(r, _)| r.as_str()),
                            fx.as_ref().map_or("", |(_, d)| d),
                            number(&row["usd_24h_change"]),
                            "CoinGecko 综合参考价",
                            &stamp.to_rfc3339(),
                            if (-120..=180).contains(&age) {
                                "live"
                            } else {
                                "delayed"
                            },
                            "24 小时涨跌；汇率为单独的人民币参考换算",
                        ));
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        let gold: Vec<&Value> = assets.iter().filter(|a| a["provider"] == "sge").collect();
        if !gold.is_empty() {
            match self
                .network
                .json(
                    "https://www.sge.com.cn/graph/quotations",
                    &[("instid", "Au99.99".into())],
                    "sge",
                    settings,
                    None,
                    false,
                )
                .await
            {
                Ok(data) => {
                    for a in gold {
                        match parsers::sge_quote(&data, a) {
                            Ok(q) => quotes.push(q),
                            Err(e) => errors.push(e),
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        let mut chains = std::collections::BTreeMap::<String, Vec<&Value>>::new();
        for a in assets.iter().filter(|a| a["provider"] == "gecko") {
            chains
                .entry(a["chain"].as_str().unwrap_or("").into())
                .or_default()
                .push(a);
        }
        if !chains.is_empty() {
            let fx = self.current_fx("USD", settings).await;
            for (chain, coins) in chains {
                let with_pool: Vec<&Value> = coins
                    .iter()
                    .filter(|a| a["pool"]["address"].as_str().is_some())
                    .copied()
                    .collect();
                for chunk in with_pool.chunks(20) {
                    let addresses = chunk
                        .iter()
                        .filter_map(|a| a["pool"]["address"].as_str())
                        .collect::<Vec<_>>()
                        .join(",");
                    match self
                        .gecko(
                            &format!("/networks/{chain}/pools/multi/{addresses}"),
                            &[],
                            settings,
                        )
                        .await
                    {
                        Ok(data) => {
                            for a in chunk {
                                let Some(pool) = data["data"].as_array().and_then(|p| {
                                    p.iter().find(|p| {
                                        p["attributes"]["address"] == a["pool"]["address"]
                                    })
                                }) else {
                                    continue;
                                };
                                let attrs = &pool["attributes"];
                                let field = if a["pool"]["tokenSide"] == "quote" {
                                    "quote_token_price_usd"
                                } else {
                                    "base_token_price_usd"
                                };
                                let Some(price) = positive(&attrs[field]) else {
                                    continue;
                                };
                                quotes.push(quote(
                                    a["id"].as_str().unwrap(),
                                    &price,
                                    "USD",
                                    fx.as_ref().map(|(r, _)| r.as_str()),
                                    fx.as_ref().map_or("", |(_, d)| d),
                                    if a["pool"]["tokenSide"] == "base" {
                                        number(&attrs["price_change_percentage"]["h24"])
                                    } else {
                                        None
                                    },
                                    "GeckoTerminal 指定池参考价",
                                    "",
                                    "delayed",
                                    "池行情缓存约 1 分钟；无交易时间时不触发实时提醒",
                                ));
                                if let Some(q) = quotes.last_mut() {
                                    q["poolAddress"] = a["pool"]["address"].clone();
                                }
                            }
                        }
                        Err(e) => errors.push(e),
                    }
                }
                for a in coins
                    .iter()
                    .filter(|a| a["pool"]["address"].as_str().is_none())
                {
                    errors.push(format!(
                        "{} 没有可估值交易池，请选择有效池或使用手动估价",
                        a["name"].as_str().unwrap_or("代币")
                    ));
                }
            }
        }
        (quotes, errors)
    }
    pub(super) fn all_tick_code(a: &Value) -> Option<String> {
        if a["provider"] == "cn" {
            Some(format!(
                "{}.{}",
                a["symbol"].as_str()?,
                a["market"].as_str()?
            ))
        } else if a["provider"] == "coingecko" {
            let code = match a["providerId"].as_str()? {
                "bitcoin" => "BTCUSDT",
                "ethereum" => "ETHUSDT",
                "binancecoin" => "BNBUSDT",
                "solana" => "SOLUSDT",
                "ripple" => "XRPUSDT",
                "dogecoin" => "DOGEUSDT",
                _ => return None,
            };
            Some(code.into())
        } else {
            None
        }
    }
    pub async fn authorized_quotes(
        &self,
        assets: &[Value],
        settings: &Value,
    ) -> Result<Vec<Value>, String> {
        let secrets = super::credentials::read(&self.root)?;
        let token = secrets["allTick"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or("请先填写 AllTick Token")?;
        let mut result = Vec::new();
        for is_stock in [true, false] {
            let selected: Vec<&Value> = assets
                .iter()
                .filter(|a| {
                    if is_stock {
                        a["provider"] == "cn"
                    } else {
                        a["provider"] == "coingecko"
                    }
                })
                .filter(|a| Self::all_tick_code(a).is_some())
                .collect();
            for chunk in selected.chunks(10) {
                let symbols: Vec<Value> = chunk
                    .iter()
                    .filter_map(|a| Self::all_tick_code(a))
                    .map(|code| json!({"code":code}))
                    .collect();
                let payload=json!({"trace":uuid::Uuid::new_v4().to_string(),"data":{"symbol_list":symbols}}).to_string();
                let data = self
                    .network
                    .json(
                        if is_stock {
                            "https://quote.alltick.co/quote-stock-b-api/trade-tick"
                        } else {
                            "https://quote.alltick.co/quote-b-api/trade-tick"
                        },
                        &[("token", token.into()), ("query", payload)],
                        "alltick",
                        settings,
                        None,
                        false,
                    )
                    .await?;
                if data["ret"] != 200 {
                    return Err(format!(
                        "AllTick 行情权限或产品访问失败（代码 {}），公开参考源将继续尝试",
                        data["ret"]
                    ));
                }
                let fx = if is_stock {
                    Some((
                        "1".into(),
                        chrono::Utc::now().format("%Y-%m-%d").to_string(),
                    ))
                } else {
                    self.current_fx("USDT", settings).await
                };
                for row in data["data"]["tick_list"]
                    .as_array()
                    .ok_or("AllTick 报价字段缺失")?
                {
                    let Some(a) = chunk
                        .iter()
                        .find(|a| Self::all_tick_code(a).as_deref() == row["code"].as_str())
                    else {
                        continue;
                    };
                    let Some(price) = positive(&row["price"]) else {
                        continue;
                    };
                    let Some(stamp) = row["tick_time"]
                        .as_str()
                        .and_then(|s| s.parse::<i64>().ok())
                        .and_then(chrono::DateTime::from_timestamp_millis)
                    else {
                        continue;
                    };
                    let age = chrono::Utc::now().timestamp() - stamp.timestamp();
                    let state = if is_stock && !super::stock_session(chrono::Utc::now()) {
                        "closed"
                    } else if (-120..=180).contains(&age) {
                        "live"
                    } else {
                        "delayed"
                    };
                    result.push(quote(
                        a["id"].as_str().unwrap(),
                        &price,
                        if is_stock { "CNY" } else { "USDT" },
                        fx.as_ref().map(|(r, _)| r.as_str()),
                        fx.as_ref().map_or("", |(_, d)| d),
                        None,
                        "AllTick 授权成交报价",
                        &stamp.to_rfc3339(),
                        state,
                        "覆盖与延迟取决于用户授权",
                    ));
                }
            }
        }
        Ok(result)
    }
    async fn all_tick_candles(
        &self,
        a: &Value,
        timeframe: &str,
        settings: &Value,
    ) -> Result<Value, String> {
        let kind = match timeframe {
            "1m" => 1,
            "5m" => 2,
            "15m" => 3,
            "30m" => 4,
            "60m" | "1h" => 5,
            "4h" => 7,
            "day" => 8,
            "week" => 9,
            "month" => 10,
            _ => return Err("AllTick 不支持这个周期".into()),
        };
        let secrets = super::credentials::read(&self.root)?;
        let token = secrets["allTick"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or("请先填写 AllTick Token")?;
        let code = Self::all_tick_code(a).ok_or("该资产不适用于 AllTick K 线")?;
        let is_stock = a["provider"] == "cn";
        let query=json!({"trace":uuid::Uuid::new_v4().to_string(),"data":{"code":code,"kline_type":kind,"kline_timestamp_end":0,"query_kline_num":500,"adjust_type":0}}).to_string();
        let response = self
            .network
            .json(
                if is_stock {
                    "https://quote.alltick.co/quote-stock-b-api/kline"
                } else {
                    "https://quote.alltick.co/quote-b-api/kline"
                },
                &[("token", token.into()), ("query", query)],
                "alltick",
                settings,
                None,
                false,
            )
            .await?;
        if response["ret"] != 200 {
            return Err(format!(
                "AllTick 历史周期权限不足或服务失败（代码 {}）",
                response["ret"]
            ));
        }
        let mut bars = Vec::new();
        for row in response["data"]["kline_list"]
            .as_array()
            .ok_or("AllTick 没有返回历史 K 线")?
        {
            let time = row["timestamp"]
                .as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .or_else(|| row["timestamp"].as_i64());
            if let Some(t) = time {
                if let Some(c) = candle(
                    t,
                    &row["open_price"],
                    &row["high_price"],
                    &row["low_price"],
                    &row["close_price"],
                    &row["volume"],
                ) {
                    bars.push(c)
                }
            }
        }
        bars.sort_by_key(|v| v["time"].as_i64());
        bars.dedup_by(|a, b| a["time"] == b["time"]);
        if bars.is_empty() {
            return Err("AllTick 没有返回有效历史数据".into());
        }
        Ok(
            json!({"bars":bars,"source":"AllTick 授权 K 线 · 不复权","interval":timeframe,"currency":if is_stock{"CNY"}else{"USDT"},"volumeUnit":"源成交量（供应商口径）","fetchedAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"cached":false}),
        )
    }
    #[cfg(test)]
    pub async fn candles(
        &self,
        a: &Value,
        timeframe: &str,
        adjust: &str,
        settings: &Value,
    ) -> Result<Value, String> {
        self.candles_since(a, timeframe, adjust, settings, None)
            .await
    }
    pub async fn candles_since(
        &self,
        a: &Value,
        timeframe: &str,
        adjust: &str,
        settings: &Value,
        since: Option<i64>,
    ) -> Result<Value, String> {
        if settings["allTickEnabled"] == true
            && Self::all_tick_code(a).is_some()
            && ((a["provider"] == "cn" && adjust == "none")
                || (a["provider"] == "coingecko"
                    && ["day", "week", "month", "1m", "5m", "15m", "60m", "4h"]
                        .contains(&timeframe)))
        {
            match self.all_tick_candles(a, timeframe, settings).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if timeframe == "1m" || a["provider"] == "coingecko" {
                        return Err(e);
                    }
                }
            }
        }
        let provider = a["provider"].as_str().ok_or("行情源无效")?;
        let mut bars = Vec::new();
        let source: String;
        let interval: String;
        let mut currency = a["currency"].as_str().unwrap_or("CNY").to_string();
        let volume_unit;
        if provider == "cn" {
            let klt = match timeframe {
                "5m" => 5,
                "15m" => 15,
                "60m" => 60,
                "day" => 101,
                "week" => 102,
                "month" => 103,
                _ => return Err("股票周期不支持".into()),
            };
            let fqt = if adjust == "qfq" { 1 } else { 0 };
            let response = self
                .network
                .json(
                    "https://push2his.eastmoney.com/api/qt/stock/kline/get",
                    &[
                        ("secid", a["providerId"].as_str().unwrap().into()),
                        ("klt", klt.to_string()),
                        ("fqt", fqt.to_string()),
                        (
                            "beg",
                            if adjust == "none" {
                                since
                                    .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                                    .map(|t| {
                                        t.with_timezone(
                                            &chrono::FixedOffset::east_opt(8 * 3600).unwrap(),
                                        )
                                        .format("%Y%m%d")
                                        .to_string()
                                    })
                                    .unwrap_or_else(|| "20200101".into())
                            } else {
                                "20200101".into()
                            },
                        ),
                        ("end", "20500101".into()),
                        ("fields1", "f1,f2,f3,f4,f5,f6".into()),
                        ("fields2", "f51,f52,f53,f54,f55,f56,f57".into()),
                    ],
                    "cn",
                    settings,
                    None,
                    false,
                )
                .await?;
            for row in response["data"]["klines"]
                .as_array()
                .ok_or("该周期暂无历史行情")?
            {
                let fields: Vec<&str> = row.as_str().unwrap_or("").split(',').collect();
                if fields.len() < 6 {
                    continue;
                }
                let stamp =
                    if let Ok(date) = chrono::NaiveDate::parse_from_str(fields[0], "%Y-%m-%d") {
                        date.and_hms_opt(0, 0, 0).unwrap()
                    } else {
                        chrono::NaiveDateTime::parse_from_str(fields[0], "%Y-%m-%d %H:%M")
                            .map_err(|_| "K 线时间格式错误")?
                    };
                let time = stamp
                    .and_local_timezone(chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                    .single()
                    .unwrap()
                    .timestamp();
                if let Some(c) = candle(
                    time,
                    &json!(fields[1]),
                    &json!(fields[3]),
                    &json!(fields[4]),
                    &json!(fields[2]),
                    &json!(fields[5]),
                ) {
                    bars.push(c)
                }
            }
            source = "东方财富公开 K 线".into();
            interval = match timeframe {
                "day" => "日 K",
                "week" => "周 K",
                "month" => "月 K",
                "5m" => "5 分钟",
                "15m" => "15 分钟",
                _ => "60 分钟",
            }
            .into();
            volume_unit = "手";
        } else if provider == "sge" {
            if timeframe != "day" {
                return Err("黄金参考源仅提供日 K".into());
            }
            let response = self
                .network
                .json(
                    "https://www.sge.com.cn/graph/Dailyhq",
                    &[("instid", "Au99.99".into())],
                    "sge",
                    settings,
                    None,
                    true,
                )
                .await?;
            for row in response["time"].as_array().ok_or("黄金历史数据缺失")? {
                let cols = row.as_array().ok_or("黄金历史数据格式错误")?;
                if cols.len() < 5 {
                    continue;
                }
                let date =
                    chrono::NaiveDate::parse_from_str(cols[0].as_str().unwrap_or(""), "%Y-%m-%d")
                        .map_err(|_| "黄金 K 线日期无效")?;
                let time = date
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_local_timezone(chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                    .single()
                    .unwrap()
                    .timestamp();
                if let Some(c) = candle(time, &cols[1], &cols[4], &cols[3], &cols[2], &Value::Null)
                {
                    bars.push(c)
                }
            }
            source = "上海黄金交易所 Au99.99".into();
            interval = "日 K · 元/克".into();
            volume_unit = "未提供";
        } else if provider == "coingecko" {
            let (days, label) = match timeframe {
                "1d" => ("1", "近 1 天 · 30 分钟 K"),
                "7d" => ("7", "近 7 天 · 4 小时 K"),
                "30d" => ("30", "近 30 天 · 4 小时 K"),
                "90d" => ("90", "近 90 天 · 4 日 K"),
                "day" => ("180", "日 K（授权）"),
                _ => return Err("该币价源周期不支持".into()),
            };
            let mut params = vec![
                ("vs_currency", "usd".into()),
                ("days", days.into()),
                ("precision", "full".into()),
            ];
            if timeframe == "day" {
                if settings["coinGeckoMode"] != "pro" {
                    return Err("精确日 K 需要 CoinGecko 授权计划".into());
                }
                params.push(("interval", "daily".into()))
            }
            let response = self
                .cg(
                    &format!("/coins/{}/ohlc", a["providerId"].as_str().unwrap()),
                    &params,
                    settings,
                )
                .await?;
            for row in response.as_array().ok_or("币价 K 线响应格式错误")? {
                let r = row.as_array().ok_or("币价 K 线字段无效")?;
                if r.len() < 5 {
                    continue;
                }
                if let Some(time) = r[0].as_i64() {
                    if let Some(c) = candle(time / 1000, &r[1], &r[2], &r[3], &r[4], &Value::Null) {
                        bars.push(c)
                    }
                }
            }
            source = "CoinGecko 综合 OHLC".into();
            interval = label.into();
            currency = "USD".into();
            volume_unit = "未提供";
        } else if provider == "gecko" {
            let (tf, aggregate, label) = match timeframe {
                "1m" => ("minute", "1", "1 分钟 K"),
                "5m" => ("minute", "5", "5 分钟 K"),
                "15m" => ("minute", "15", "15 分钟 K"),
                "1h" => ("hour", "1", "1 小时 K"),
                "4h" => ("hour", "4", "4 小时 K"),
                "day" => ("day", "1", "日 K"),
                _ => return Err("链上周期不支持".into()),
            };
            let pool = &a["pool"];
            let address = pool["address"].as_str().ok_or("请先选择有效交易池")?;
            let side = pool["tokenSide"]
                .as_str()
                .ok_or("交易池未识别目标币所在方向")?;
            let response = self
                .gecko(
                    &format!(
                        "/networks/{}/pools/{address}/ohlcv/{tf}",
                        a["chain"].as_str().unwrap_or("")
                    ),
                    &[
                        ("aggregate", aggregate.into()),
                        ("token", side.into()),
                        ("currency", "usd".into()),
                        ("limit", "500".into()),
                    ],
                    settings,
                )
                .await?;
            for row in response["data"]["attributes"]["ohlcv_list"]
                .as_array()
                .ok_or("链上 K 线缺失")?
            {
                let r = row.as_array().ok_or("链上 K 线格式错误")?;
                if r.len() >= 6 {
                    if let Some(time) = r[0].as_i64() {
                        if let Some(c) = candle(time, &r[1], &r[2], &r[3], &r[4], &r[5]) {
                            bars.push(c)
                        }
                    }
                }
            }
            source = format!("GeckoTerminal 池 {address} · {side}");
            interval = label.into();
            currency = "USD".into();
            volume_unit = "USD 成交额";
        } else {
            return Err("行情图表来源不支持".into());
        }
        bars.sort_by_key(|v| v["time"].as_i64().unwrap_or(0));
        bars.dedup_by(|a, b| a["time"] == b["time"]);
        let start = bars.len().saturating_sub(600);
        let bars = bars.split_off(start);
        if bars.is_empty() {
            return Err("该资产暂无有效 K 线数据".into());
        }
        Ok(
            json!({"bars":bars,"source":source,"interval":interval,"currency":currency,"volumeUnit":volume_unit,"fetchedAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"cached":false}),
        )
    }
}
