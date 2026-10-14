use super::{credentials, parsers, MarketRuntime};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tauri::Emitter;
use tokio_tungstenite::{connect_async, tungstenite::Message};
pub(super) fn tick_quote(
    row: &Value,
    asset: &Value,
    fx: Option<&(String, String)>,
) -> Option<Value> {
    let price = parsers::positive(&row["price"])?;
    let time = row["tick_time"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| row["tick_time"].as_i64())?;
    let stamp = if time >= 1_000_000_000_000 {
        chrono::DateTime::from_timestamp_millis(time)
    } else {
        chrono::DateTime::from_timestamp(time, 0)
    }?;
    let stock = asset["provider"] == "cn";
    let age = chrono::Utc::now().timestamp() - stamp.timestamp();
    let state = if stock && !super::stock_session(chrono::Utc::now()) {
        "closed"
    } else if (-120..=180).contains(&age) {
        "live"
    } else {
        "delayed"
    };
    let date = stamp.format("%Y-%m-%d").to_string();
    Some(parsers::quote(
        asset["id"].as_str()?,
        &price,
        if stock { "CNY" } else { "USDT" },
        if stock {
            Some("1")
        } else {
            fx.map(|(rate, _)| rate.as_str())
        },
        if stock {
            &date
        } else {
            fx.map_or("", |(_, date)| date)
        },
        None,
        "AllTick 授权推送",
        &stamp.to_rfc3339(),
        state,
        "逐笔推送；标的与频率取决于授权，HTTP 参考行情作为回退",
    ))
}
impl MarketRuntime {
    pub(super) fn start_streams(self: &Arc<Self>, app: tauri::AppHandle) {
        for stock in [true, false] {
            let runtime = self.clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let group = if stock {
                    "AllTick 股票推送"
                } else {
                    "AllTick 币种推送"
                };
                let mut failures = 0u32;
                while !runtime.stopped.load(Ordering::Relaxed) {
                    let doc = runtime
                        .latest_doc
                        .lock()
                        .map(|d| d.clone())
                        .unwrap_or(Value::Null);
                    let cfg = &doc["settings"];
                    if cfg["allTickEnabled"] != true
                        || cfg["allTickTransport"] != "push"
                        || runtime.stream_assets(&doc, stock).is_empty()
                    {
                        if runtime
                            .stream_errors
                            .lock()
                            .is_ok_and(|mut errors| errors.remove(group).is_some())
                        {
                            let _ = app.emit("investment:quotes", runtime.snapshot());
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                    match runtime.stream_session(&app, &doc, stock, group).await {
                        Ok(()) => {
                            failures = 0;
                        }
                        Err(error) => {
                            failures = (failures + 1).min(5);
                            if let Ok(mut errors) = runtime.stream_errors.lock() {
                                errors.insert(
                                    group.into(),
                                    format!("{group}：{error}；轮询回退继续工作"),
                                );
                            }
                            if let Ok(mut quotes) = runtime.quotes.lock() {
                                for quote in quotes.values_mut().filter(|q| {
                                    q["source"] == "AllTick 授权推送"
                                        && (q["currency"] == "CNY") == stock
                                }) {
                                    quote["status"] = json!("cached");
                                }
                            }
                            let _ = app.emit("investment:quotes", runtime.snapshot());
                        }
                    }
                    let delay = if failures == 0 {
                        3
                    } else {
                        (15u64 << failures).min(900)
                    };
                    for _ in 0..delay / 3 {
                        if runtime.stopped.load(Ordering::Relaxed) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            });
        }
    }
    fn stream_assets(&self, doc: &Value, stock: bool) -> Vec<Value> {
        self.active(doc)
            .into_iter()
            .filter(|a| {
                if stock {
                    a["provider"] == "cn"
                } else {
                    a["provider"] == "coingecko"
                }
            })
            .filter(|a| Self::all_tick_code(a).is_some())
            .collect()
    }
    async fn stream_session(
        &self,
        app: &tauri::AppHandle,
        doc: &Value,
        stock: bool,
        group: &str,
    ) -> Result<(), String> {
        let cfg = &doc["settings"];
        let secrets = credentials::read(&self.root)?;
        let token = secrets["allTick"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or("请先配置 AllTick Token")?;
        let epoch = self.stream_epoch.load(Ordering::Relaxed);
        self.network.reserve_stream(cfg).await?;
        let mut url = reqwest::Url::parse(if stock {
            "wss://quote.alltick.co/quote-stock-b-ws-api"
        } else {
            "wss://quote.alltick.co/quote-b-ws-api"
        })
        .map_err(|_| "推送地址无效")?;
        url.query_pairs_mut().append_pair("token", token);
        let (mut socket, _) =
            tokio::time::timeout(Duration::from_secs(20), connect_async(url.as_str()))
                .await
                .map_err(|_| "推送连接超时")?
                .map_err(|_| "推送连接失败或授权拒绝")?;
        let mut assets = self.stream_assets(doc, stock);
        let cap = cfg["allTickSubscriptionCap"].as_u64().unwrap_or(5) as usize;
        let mut warning = if assets.len() > cap {
            format!("订阅配置限额 {cap}，其余资产继续轮询；实际权限可能更低")
        } else {
            String::new()
        };
        assets.truncate(cap);
        let mut codes: HashMap<String, Value> = assets
            .into_iter()
            .filter_map(|a| Self::all_tick_code(&a).map(|code| (code, a)))
            .collect();
        let subscribe = |codes: &HashMap<String, Value>| {
            json!({"cmd_id":22004,"seq_id":1,"trace":uuid::Uuid::new_v4().to_string(),"data":{"symbol_list":codes.keys().map(|code|json!({"code":code})).collect::<Vec<_>>()}}).to_string()
        };
        socket
            .send(Message::Text(subscribe(&codes).into()))
            .await
            .map_err(|_| "推送订阅发送失败")?;
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(10),
            Duration::from_secs(10),
        );
        let mut fx = if stock {
            None
        } else {
            self.current_fx("USDT", cfg).await
        };
        let mut last_seen = chrono::Utc::now().timestamp();
        let mut last_emit = 0i64;
        loop {
            tokio::select! {
                _=heartbeat.tick()=>{
                    if self.stopped.load(Ordering::Relaxed){let _=socket.close(None).await;return Ok(())}
                    let latest=self.latest_doc.lock().map(|d|d.clone()).unwrap_or(Value::Null);let settings=&latest["settings"];
                    if settings["allTickEnabled"]!=true||settings["allTickTransport"]!="push"||epoch!=self.stream_epoch.load(Ordering::Relaxed){let _=socket.close(None).await;return Ok(())}
                    if chrono::Utc::now().timestamp()-last_seen>40{return Err("推送心跳超时".into())}
                    let mut selected=self.stream_assets(&latest,stock);let cap=settings["allTickSubscriptionCap"].as_u64().unwrap_or(5)as usize;warning=if selected.len()>cap{format!("订阅配置限额 {cap}，其余资产继续轮询")}else{String::new()};selected.truncate(cap);let next:HashMap<String,Value>=selected.into_iter().filter_map(|a|Self::all_tick_code(&a).map(|code|(code,a))).collect();
                    if next.is_empty(){let _=socket.close(None).await;return Ok(())}
                    if codes.keys().any(|c|!next.contains_key(c))||next.keys().any(|c|!codes.contains_key(c)){self.network.reserve_stream(settings).await?;socket.send(Message::Text(subscribe(&next).into())).await.map_err(|_|"更新推送订阅失败")?;codes=next;}else{socket.send(Message::Text(json!({"cmd_id":22000,"seq_id":2,"trace":uuid::Uuid::new_v4().to_string(),"data":{}}).to_string().into())).await.map_err(|_|"推送心跳发送失败")?;}
                    if !stock{fx=self.current_fx("USDT",settings).await;}
                    let cache=self.quotes.lock().map(|q|q.values().cloned().collect::<Vec<_>>()).unwrap_or_default();let _=self.persist("investment-quotes.json",&json!(cache));
                }
                message=socket.next()=>{
                    let message=message.ok_or("推送连接已关闭")?.map_err(|_|"推送连接中断")?;last_seen=chrono::Utc::now().timestamp();
                    match message{Message::Text(text)=>{if text.len()>1024*1024{return Err("推送响应过大".into())}let value:Value=serde_json::from_str(&text).map_err(|_|"推送格式无效")?;if value["ret"].as_i64().is_some_and(|ret|ret!=200){return Err(format!("推送权限或订阅被拒绝（代码 {}）",value["ret"]))}if let Ok(mut errors)=self.stream_errors.lock(){if warning.is_empty(){errors.remove(group);}else{errors.insert(group.into(),warning.clone());}}
                        if value["cmd_id"]==22998{let row=&value["data"];if let Some(asset)=row["code"].as_str().and_then(|code|codes.get(code)){if let Some(quote)=tick_quote(row,asset,fx.as_ref()){let mut quotes=self.quotes.lock().map_err(|_|"推送状态错误")?;let id=asset["id"].as_str().unwrap();let incoming=chrono::DateTime::parse_from_rfc3339(quote["sourceAt"].as_str().unwrap()).ok();let old=quotes.get(id).and_then(|q|q["sourceAt"].as_str()).and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok());if incoming>=old{quotes.insert(id.into(),quote);}drop(quotes);let now=chrono::Utc::now().timestamp();if now!=last_emit{last_emit=now;let _=app.emit("investment:quotes",self.snapshot());}}}}
                    },Message::Ping(payload)=>socket.send(Message::Pong(payload)).await.map_err(|_|"推送响应失败")?,Message::Close(_)=>return Err("推送连接已关闭".into()),_=>{}}
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn push_precision_time_units_and_missing_prices() {
        let asset = parsers::defaults()
            .into_iter()
            .find(|a| a["id"] == "cg:bitcoin")
            .unwrap();
        let milliseconds = chrono::Utc::now().timestamp_millis();
        let q = tick_quote(
            &json!({"price":"0.000000000000000123456789","tick_time":milliseconds}),
            &asset,
            Some(&("6.700001".into(), "2026-10-13".into())),
        )
        .unwrap();
        assert_eq!(q["price"], "0.000000000000000123456789");
        assert_eq!(q["currency"], "USDT");
        assert_eq!(q["cnyRate"], "6.700001");
        assert_eq!(q["status"], "live");
        assert!(tick_quote(&json!({"price":"0","tick_time":milliseconds}), &asset, None).is_none());
        assert!(tick_quote(
            &json!({"price":"1","tick_time":milliseconds/1000}),
            &asset,
            None
        )
        .is_some());
    }
}
