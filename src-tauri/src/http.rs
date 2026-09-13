use reqwest::{header, Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub fn user_agent() -> String {
    ua_generator::ua::spoof_ua().to_string()
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub cookie: String,
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub subscription_id: String,
    #[serde(default)]
    pub subscription_node_id: String,
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
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15));
    let proxy_address = if account.subscription_id.trim().is_empty() {
        account.proxy.trim().to_string()
    } else {
        crate::subscriptions::resolve_proxy(
            account.subscription_id.trim(),
            account.subscription_node_id.trim(),
        )?
    };
    if !proxy_address.is_empty() {
        let proxy = reqwest::Url::parse(&proxy_address).map_err(|_| "代理地址无效")?;
        if !matches!(proxy.scheme(), "http" | "https" | "socks5" | "socks5h")
            || proxy.host_str().is_none()
        {
            return Err("代理需使用 http、https 或 socks5 地址".into());
        }
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| "代理地址无效")?);
    }
    builder.build().map_err(|_| "无法初始化网络连接".into())
}

async fn send_with_user_agent(
    request: RequestBuilder,
    user_agent: &str,
) -> Result<Response, String> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::USER_AGENT,
        user_agent
            .parse()
            .map_err(|_| "无法生成有效的 User-Agent")?,
    );
    request.headers(headers).send().await.map_err(|error| {
        if error.is_timeout() {
            "请求超时，请检查网络连接".to_string()
        } else {
            "网络请求失败，请检查网络或代理设置".to_string()
        }
    })
}

pub async fn send(request: RequestBuilder) -> Result<Response, String> {
    send_with_user_agent(request, &user_agent()).await
}

pub async fn json(request: RequestBuilder) -> Result<Value, String> {
    json_with_user_agent(request, &user_agent()).await
}

pub async fn json_with_user_agent(
    request: RequestBuilder,
    user_agent: &str,
) -> Result<Value, String> {
    let response = send_with_user_agent(request, user_agent).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn requests_refresh_user_agent_and_preserve_explicit_fingerprint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..33 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 2048];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let size = socket.read(&mut buffer).await.unwrap();
                    assert!(size > 0);
                    request.extend_from_slice(&buffer[..size]);
                }
                let request = String::from_utf8(request).unwrap();
                let user_agents: Vec<_> = request
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .filter(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                    .map(|(_, value)| value.trim())
                    .collect();
                assert_eq!(user_agents.len(), 1);
                assert!(request.contains("cookie: session=local-test"));
                let body = serde_json::json!({ "userAgent": user_agents[0] }).to_string();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let client = Client::builder()
            .no_proxy()
            .user_agent("stale-client-agent")
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let request = || {
            client
                .get(&url)
                .header(header::COOKIE, "session=local-test")
                .header(header::USER_AGENT, "stale-request-agent")
        };
        let mut generated = HashSet::new();
        for index in 0..32 {
            let response = if index % 2 == 0 {
                json(request()).await.unwrap()
            } else {
                send(request())
                    .await
                    .unwrap()
                    .json::<Value>()
                    .await
                    .unwrap()
            };
            let agent = response["userAgent"].as_str().unwrap();
            assert!(!agent.is_empty());
            assert_ne!(agent, "stale-client-agent");
            assert_ne!(agent, "stale-request-agent");
            generated.insert(agent.to_string());
        }
        assert!(generated.len() > 1);

        let agent = user_agent();
        let response = json_with_user_agent(request(), &agent).await.unwrap();
        assert_eq!(response["userAgent"], agent);
        server.await.unwrap();
    }
}
