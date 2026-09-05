use reqwest::{header, Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub cookie: String,
    #[serde(default)]
    pub proxy: String,
}

pub fn client(account: &Account, origin: &str) -> Result<Client, String> {
    if account.cookie.trim().is_empty() {
        return Err("请先填写 Cookie".into());
    }
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::COOKIE,
        account
            .cookie
            .trim()
            .parse()
            .map_err(|_| "Cookie 包含无效字符或换行")?,
    );
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::ORIGIN,
        origin.parse().map_err(|_| "无效的请求来源")?,
    );
    headers.insert(
        header::REFERER,
        format!("{origin}/").parse().map_err(|_| "无效的请求来源")?,
    );
    let mut builder = Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15));
    if !account.proxy.trim().is_empty() {
        let proxy = reqwest::Url::parse(account.proxy.trim()).map_err(|_| "代理地址无效")?;
        if !matches!(proxy.scheme(), "http" | "https" | "socks5" | "socks5h")
            || proxy.host_str().is_none()
        {
            return Err("代理需使用 http、https 或 socks5 地址".into());
        }
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| "代理地址无效")?);
    }
    builder.build().map_err(|_| "无法初始化网络连接".into())
}

pub async fn json(request: RequestBuilder) -> Result<Value, String> {
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            "请求超时，请检查网络连接".to_string()
        } else {
            "网络请求失败，请检查网络或代理设置".to_string()
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "登录状态失效或访问受限，请在官方页面检查账号".into(),
            412 | 429 => "请求受限，请稍后重试或在官方页面完成验证".into(),
            _ => format!("服务器返回 HTTP {}", status.as_u16()),
        });
    }
    response
        .json()
        .await
        .map_err(|_| "服务器返回了非 JSON 数据，请在官方页面检查账号状态".into())
}

pub fn string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

pub fn number(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

pub fn first<'a>(value: &'a Value, keys: &[&str]) -> &'a Value {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .find(|v| !v.is_null() && **v != "")
        .unwrap_or(&Value::Null)
}

pub fn cookie_value(cookie: &str, key: &str) -> String {
    cookie
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value.to_string())
        .unwrap_or_default()
}
