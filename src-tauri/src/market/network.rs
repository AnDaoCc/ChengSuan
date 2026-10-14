use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Budget {
    period: String,
    count: u64,
    last: i64,
    blocked_until: i64,
    failures: u32,
}
pub struct Network {
    client: reqwest::Client,
    root: PathBuf,
    budgets: Mutex<HashMap<String, Budget>>,
    gates: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}
impl Network {
    pub async fn reserve_stream(&self, settings: &Value) -> Result<(), String> {
        let gate = self
            .gates
            .lock()
            .map_err(|_| "请求额度状态错误")?
            .entry("alltick".into())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = gate.lock().await;
        let period = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let wait = {
            let mut budgets = self.budgets.lock().map_err(|_| "请求额度错误")?;
            let budget = budgets.entry("alltick".into()).or_default();
            if budget.period != period {
                *budget = Budget {
                    period,
                    ..Default::default()
                };
            }
            if budget.count >= settings["allTickDailyCap"].as_u64().unwrap_or(1000) {
                return Err("已达到 AllTick 日请求上限".into());
            }
            (budget.last + 3000 - chrono::Utc::now().timestamp_millis()).max(0)
        };
        if wait > 0 {
            tokio::time::sleep(Duration::from_millis(wait as u64)).await;
        }
        let mut budgets = self.budgets.lock().map_err(|_| "请求额度错误")?;
        let budget = budgets.get_mut("alltick").unwrap();
        budget.count += 1;
        budget.last = chrono::Utc::now().timestamp_millis();
        let _ = std::fs::write(
            self.root.join("caches/investment-usage.json"),
            serde_json::to_vec(&*budgets).unwrap_or_default(),
        );
        Ok(())
    }
    #[cfg(test)]
    pub fn block_for_test(&self, group: &str, until: i64) {
        let period = chrono::Utc::now().format("%Y-%m-%d").to_string();
        self.budgets.lock().unwrap().insert(
            group.into(),
            Budget {
                period,
                blocked_until: until,
                ..Default::default()
            },
        );
    }
    pub fn new(root: PathBuf) -> Result<Self, String> {
        let budgets = std::fs::read(root.join("caches/investment-usage.json"))
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("ChengSuan/1.1.0")
                .build()
                .map_err(|_| "无法初始化行情网络")?,
            root,
            budgets: Mutex::new(budgets),
            gates: Mutex::new(HashMap::new()),
        })
    }
    pub async fn bytes(
        &self,
        url: &str,
        params: &[(&str, String)],
        group: &str,
        settings: &Value,
        header: Option<(&str, &str)>,
        post: bool,
    ) -> Result<Vec<u8>, String> {
        let gate = {
            self.gates
                .lock()
                .map_err(|_| "行情队列错误")?
                .entry(group.into())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _gate = gate.lock().await;
        let today = chrono::Utc::now();
        let period = if group == "coingecko-keyed" {
            today.format("%Y-%m").to_string()
        } else {
            today.format("%Y-%m-%d").to_string()
        };
        let (gap, cap) = match group {
            "catalog" => (200, i64::MAX as u64),
            "cn" => (1000, i64::MAX as u64),
            "sge" => (3000, i64::MAX as u64),
            "gecko" => (10000, i64::MAX as u64),
            "coingecko-keyed" => (
                1000,
                settings["coinGeckoMonthlyCap"].as_u64().unwrap_or(10000),
            ),
            "alltick" => (10000, settings["allTickDailyCap"].as_u64().unwrap_or(1000)),
            "coingecko" => (6000, i64::MAX as u64),
            _ => (1000, i64::MAX as u64),
        };
        let wait = {
            let mut budgets = self.budgets.lock().map_err(|_| "行情预算错误")?;
            let b = budgets.entry(group.into()).or_default();
            if b.period != period {
                *b = Budget {
                    period: period.clone(),
                    ..Default::default()
                };
            }
            let now = chrono::Utc::now().timestamp_millis();
            if now < b.blocked_until {
                return Err(format!("{group} 正在退避重试，保留最后有效报价"));
            }
            if b.count >= cap {
                return Err(format!(
                    "{group} 已达到配置的请求额度，保留缓存；请调整额度或等待服务计费周期重置"
                ));
            }
            (b.last + gap - now).max(0)
        };
        if wait > 0 {
            tokio::time::sleep(Duration::from_millis(wait as u64)).await;
        }
        {
            let mut budgets = self.budgets.lock().map_err(|_| "行情预算错误")?;
            let b = budgets.get_mut(group).unwrap();
            b.count += 1;
            b.last = chrono::Utc::now().timestamp_millis();
            let _ = std::fs::write(
                self.root.join("caches/investment-usage.json"),
                serde_json::to_vec(&*budgets).unwrap_or_default(),
            );
        }
        let mut request = if post {
            self.client.post(url).form(params)
        } else {
            self.client.get(url).query(params)
        };
        if let Some((name, token)) = header {
            request = request.header(name, token);
        }
        let response = match request.send().await {
            Ok(r) => r,
            Err(_) => {
                self.failed(group, 30);
                return Err(format!("{group} 请求超时或网络不可用，保留原报价"));
            }
        };
        if !response.status().is_success() {
            let code = response.status().as_u16();
            let seconds = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(30);
            self.failed(group, seconds);
            return Err(format!(
                "{group} 服务返回 HTTP {code}{}",
                if code == 401 || code == 403 {
                    "，请检查行情权限或密钥"
                } else {
                    "，稍后自动重试"
                }
            ));
        }
        if response
            .content_length()
            .is_some_and(|n| n > 20 * 1024 * 1024)
        {
            return Err("行情响应过大".into());
        }
        let body = response.bytes().await.map_err(|_| "行情响应读取失败")?;
        if body.len() > 20 * 1024 * 1024 {
            return Err("行情响应过大".into());
        }
        if let Ok(mut budgets) = self.budgets.lock() {
            if let Some(b) = budgets.get_mut(group) {
                b.failures = 0;
                b.blocked_until = 0;
            }
        }
        Ok(body.to_vec())
    }
    fn failed(&self, group: &str, seconds: i64) {
        if let Ok(mut budgets) = self.budgets.lock() {
            if let Some(b) = budgets.get_mut(group) {
                b.failures = (b.failures + 1).min(8);
                b.blocked_until = chrono::Utc::now().timestamp_millis()
                    + seconds
                        .max(15)
                        .saturating_mul(1i64 << b.failures.saturating_sub(1))
                        .min(900)
                        * 1000;
            }
        }
    }
    pub async fn json(
        &self,
        url: &str,
        params: &[(&str, String)],
        group: &str,
        settings: &Value,
        header: Option<(&str, &str)>,
        post: bool,
    ) -> Result<Value, String> {
        let bytes = self
            .bytes(url, params, group, settings, header, post)
            .await?;
        serde_json::from_slice(&bytes).map_err(|_| format!("{group} 返回的行情格式无效"))
    }
}
