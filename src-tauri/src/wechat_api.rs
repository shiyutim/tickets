use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{header, Client, RequestBuilder, Url};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{fmt, time::Duration};
use uuid::Uuid;

pub const DEFAULT_API_BASE: &str = "https://ilinkai.weixin.qq.com";
const CHANNEL_VERSION: &str = "2.4.8";
const CLIENT_VERSION: &str = "132104";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum ApiError {
    Timeout,
    Expired,
    Failed(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => formatter.write_str("微信请求超时，请检查连接状态"),
            Self::Expired => formatter.write_str("微信登录已失效，请重新扫码绑定"),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ApiError {}

#[derive(Clone, Deserialize)]
pub struct QrResponse {
    pub qrcode: String,
    pub qrcode_img_content: String,
}

#[derive(Clone, Deserialize)]
pub struct LoginResponse {
    pub status: String,
    pub bot_token: Option<String>,
    pub ilink_bot_id: Option<String>,
    pub baseurl: Option<String>,
    pub ilink_user_id: Option<String>,
    pub redirect_host: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct UpdatesResponse {
    #[serde(default)]
    pub msgs: Vec<InboundMessage>,
    pub get_updates_buf: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct InboundMessage {
    pub from_user_id: Option<String>,
    pub to_user_id: Option<String>,
    pub message_type: Option<u32>,
    pub context_token: Option<String>,
}

pub fn api_base(value: &str) -> Result<String, String> {
    let invalid = || "微信返回的服务地址无效，请重新绑定".to_string();
    if value
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err(invalid());
    }
    let url = Url::parse(value).map_err(|_| invalid())?;
    let host = url.host_str().ok_or_else(invalid)?;
    if url.scheme() != "https"
        || !(host == "ilinkai.weixin.qq.com" || host.ends_with(".ilinkai.weixin.qq.com"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url.origin().ascii_serialization())
}

#[derive(Clone)]
pub struct WechatApi {
    client: Client,
    #[cfg(test)]
    test_base: Option<Url>,
}

impl WechatApi {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(40))
            .build()
            .map_err(|_| "无法初始化微信连接".to_string())?;
        Ok(Self {
            client,
            #[cfg(test)]
            test_base: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_test_base(base: &str) -> Self {
        let url = Url::parse(base).unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        let mut api = Self::new().unwrap();
        api.test_base = Some(url);
        api
    }

    fn endpoint(&self, base: &str, path: &str) -> Result<Url, ApiError> {
        let base = api_base(base).map_err(ApiError::Failed)?;
        let url = Url::parse(&format!("{base}/{path}"))
            .map_err(|_| ApiError::Failed("微信请求地址无效".into()))?;
        #[cfg(test)]
        if let Some(base) = &self.test_base {
            let mut test_url = base.clone();
            test_url.set_path(url.path());
            test_url.set_query(url.query());
            return Ok(test_url);
        }
        Ok(url)
    }

    fn common_headers(&self, request: RequestBuilder) -> RequestBuilder {
        request
            .header("iLink-App-Id", "bot")
            .header("iLink-App-ClientVersion", CLIENT_VERSION)
    }

    fn post(&self, url: Url, token: Option<&str>) -> Result<RequestBuilder, ApiError> {
        let random = Uuid::new_v4();
        let uin = u32::from_be_bytes(random.as_bytes()[..4].try_into().unwrap());
        let mut request = self
            .common_headers(self.client.post(url))
            .header("AuthorizationType", "ilink_bot_token")
            .header("X-WECHAT-UIN", STANDARD.encode(uin.to_string()))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            let token = token.trim();
            if token.is_empty() || token.len() > 8192 || token.chars().any(char::is_control) {
                return Err(ApiError::Failed("微信登录凭证无效，请重新扫码绑定".into()));
            }
            let mut authorization = header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| ApiError::Failed("微信登录凭证无效，请重新扫码绑定".into()))?;
            authorization.set_sensitive(true);
            request = request.header(header::AUTHORIZATION, authorization);
        }
        Ok(request)
    }

    pub async fn qr(&self, tokens: Vec<String>) -> Result<QrResponse, ApiError> {
        let tokens: Vec<_> = tokens
            .into_iter()
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty())
            .take(10)
            .collect();
        if tokens
            .iter()
            .any(|token| token.len() > 8192 || token.chars().any(char::is_control))
        {
            return Err(ApiError::Failed("微信登录凭证无效，请重新扫码绑定".into()));
        }
        let request = self
            .post(
                self.endpoint(DEFAULT_API_BASE, "ilink/bot/get_bot_qrcode?bot_type=3")?,
                None,
            )?
            .timeout(Duration::from_secs(15))
            .json(&json!({ "local_token_list": tokens }));
        let response: QrResponse = decode(receive(request, false).await?)?;
        if response.qrcode.is_empty()
            || response.qrcode.len() > 8192
            || response.qrcode_img_content.is_empty()
            || response.qrcode_img_content.len() > 16384
        {
            return Err(ApiError::Failed("微信返回的二维码无效，请重新获取".into()));
        }
        Ok(response)
    }

    pub async fn poll_qr(
        &self,
        base: &str,
        qrcode: &str,
        verify_code: Option<&str>,
    ) -> Result<LoginResponse, ApiError> {
        if qrcode.is_empty() || qrcode.len() > 8192 {
            return Err(ApiError::Failed("二维码无效，请重新获取".into()));
        }
        let mut url = self.endpoint(base, "ilink/bot/get_qrcode_status")?;
        url.query_pairs_mut().append_pair("qrcode", qrcode);
        if let Some(code) = verify_code {
            if code.is_empty() || code.len() > 64 || code.chars().any(char::is_control) {
                return Err(ApiError::Failed("微信配对码格式无效".into()));
            }
            url.query_pairs_mut().append_pair("verify_code", code);
        }
        let request = self
            .common_headers(self.client.get(url))
            .timeout(Duration::from_secs(35));
        decode(receive(request, false).await?)
    }

    pub async fn updates(
        &self,
        base: &str,
        token: &str,
        cursor: &str,
    ) -> Result<UpdatesResponse, ApiError> {
        if cursor.len() > MAX_RESPONSE_BYTES {
            return Err(ApiError::Failed("微信会话同步状态无效，请重新绑定".into()));
        }
        let request = self
            .post(self.endpoint(base, "ilink/bot/getupdates")?, Some(token))?
            .timeout(Duration::from_secs(40))
            .json(&json!({ "get_updates_buf": cursor, "base_info": base_info() }));
        let value = receive(request, false).await?;
        if value.get("ret").and_then(Value::as_i64) != Some(0)
            && !value.get("msgs").is_some_and(Value::is_array)
        {
            return Err(ApiError::Failed(
                "微信响应缺少会话同步结果，请稍后重试".into(),
            ));
        }
        decode(value)
    }

    pub async fn send(
        &self,
        base: &str,
        token: &str,
        target: &str,
        context_token: &str,
        text: &str,
    ) -> Result<(), ApiError> {
        if target.is_empty() || target.len() > 1024 || target.chars().any(char::is_control) {
            return Err(ApiError::Failed("微信通知对象无效，请重新建立会话".into()));
        }
        if context_token.is_empty() || context_token.len() > 16384 {
            return Err(ApiError::Failed(
                "微信通知会话无效，请给 Bot 发一条消息".into(),
            ));
        }
        if text.trim().is_empty() || text.len() > 16384 {
            return Err(ApiError::Failed("微信通知内容为空或过长".into()));
        }
        let request = self
            .post(self.endpoint(base, "ilink/bot/sendmessage")?, Some(token))?
            .timeout(Duration::from_secs(15))
            .json(&json!({
                "msg": {
                    "from_user_id": "",
                    "to_user_id": target,
                    "client_id": format!("tickets-{}", Uuid::new_v4()),
                    "message_type": 2,
                    "message_state": 2,
                    "context_token": context_token,
                    "item_list": [{ "type": 1, "text_item": { "text": text } }]
                },
                "base_info": base_info()
            }));
        receive(request, true).await?;
        Ok(())
    }
}

fn base_info() -> Value {
    json!({ "channel_version": CHANNEL_VERSION, "bot_agent": concat!("Tickets/", env!("CARGO_PKG_VERSION")) })
}

fn network_error(error: reqwest::Error) -> ApiError {
    if error.is_timeout() {
        ApiError::Timeout
    } else {
        ApiError::Failed("微信连接失败，请检查网络后重试".into())
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|_| ApiError::Failed("微信响应格式异常，请稍后重试".into()))
}

fn check_result(value: &Value, require_ret: bool) -> Result<(), ApiError> {
    if !value.is_object() {
        return Err(ApiError::Failed("微信响应格式异常，请稍后重试".into()));
    }
    if ["ret", "errcode"]
        .iter()
        .any(|key| value.get(key).and_then(Value::as_i64) == Some(-14))
    {
        return Err(ApiError::Expired);
    }
    for key in ["ret", "errcode"] {
        if let Some(code) = value.get(key) {
            if code.is_null() {
                continue;
            }
            match code.as_i64() {
                Some(0) => (),
                Some(_) => {
                    return Err(ApiError::Failed(
                        "微信服务拒绝了请求，请检查绑定状态并给 Bot 发一条消息".into(),
                    ))
                }
                None => return Err(ApiError::Failed("微信响应格式异常，请稍后重试".into())),
            }
        }
    }
    if require_ret && value.get("ret").and_then(Value::as_i64) != Some(0) {
        return Err(ApiError::Failed(
            "微信未确认消息发送成功，请检查微信是否收到通知".into(),
        ));
    }
    Ok(())
}

async fn receive(request: RequestBuilder, require_ret: bool) -> Result<Value, ApiError> {
    let mut response = request.send().await.map_err(network_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 => ApiError::Expired,
            403 => ApiError::Failed("微信访问受限，请检查微信中的绑定状态".into()),
            429 => ApiError::Failed("微信请求过于频繁，请稍后重试".into()),
            _ => ApiError::Failed(format!("微信服务暂不可用（HTTP {}）", status.as_u16())),
        });
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(ApiError::Failed("微信响应过大，已停止读取".into()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(ApiError::Failed("微信响应过大，已停止读取".into()));
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::Failed("微信响应格式异常，请稍后重试".into()))?;
    check_result(&value, require_ret)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn server(
        responses: Vec<(u16, String, String)>,
    ) -> (WechatApi, tokio::task::JoinHandle<Vec<(String, Value)>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = WechatApi::with_test_base(&format!("http://{}", listener.local_addr().unwrap()));
        let handle = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, headers, body) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                let header_end = loop {
                    let size = socket.read(&mut buffer).await.unwrap();
                    assert!(size > 0);
                    request.extend_from_slice(&buffer[..size]);
                    if let Some(index) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        break index + 4;
                    }
                };
                let head = String::from_utf8(request[..header_end].to_vec()).unwrap();
                let length: usize = head
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .map(|(_, value)| value.trim().parse().unwrap())
                    .unwrap_or(0);
                while request.len() < header_end + length {
                    let size = socket.read(&mut buffer).await.unwrap();
                    assert!(size > 0);
                    request.extend_from_slice(&buffer[..size]);
                }
                let payload = if length > 0 {
                    serde_json::from_slice(&request[header_end..header_end + length]).unwrap()
                } else {
                    Value::Null
                };
                requests.push((head, payload));
                let _ = socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).as_bytes()).await;
            }
            requests
        });
        (api, handle)
    }

    fn ok(value: Value) -> (u16, String, String) {
        (200, String::new(), value.to_string())
    }

    #[test]
    fn only_wechat_https_origins_are_accepted() {
        assert_eq!(api_base(DEFAULT_API_BASE).unwrap(), DEFAULT_API_BASE);
        assert_eq!(
            api_base("https://sh.ilinkai.weixin.qq.com:443/").unwrap(),
            "https://sh.ilinkai.weixin.qq.com"
        );
        for invalid in [
            "http://ilinkai.weixin.qq.com",
            "https://ilinkai.weixin.qq.com.evil.test",
            "https://evil-ilinkai.weixin.qq.com",
            "https://127.0.0.1",
            "https://user:secret@ilinkai.weixin.qq.com",
            "https://ilinkai.weixin.qq.com:8443",
            "https://ilinkai.weixin.qq.com/api",
            "https://ilinkai.weixin.qq.com/?token=secret",
            "https://ilinkai.weixin.qq.com/#secret",
            "https://ilinkai.weixin.qq.com\\@evil.test",
            " https://ilinkai.weixin.qq.com",
        ] {
            assert!(api_base(invalid).is_err(), "accepted unsafe origin");
        }
    }

    #[tokio::test]
    async fn login_and_messages_use_distinct_headers_and_correct_payloads() {
        let (api, server) = server(vec![
            ok(json!({ "ret": null, "errcode": null, "qrcode": "scan&token=1", "qrcode_img_content": "https://weixin.qq.com/test" })),
            ok(json!({ "ret": null, "status": "need_verifycode" })),
            ok(json!({ "ret": 0, "msgs": [{ "from_user_id": "user", "to_user_id": "bot", "message_type": 1, "context_token": "context" }], "get_updates_buf": "next" })),
            ok(json!({ "ret": 0 })),
        ]).await;
        let qr = api
            .qr((0..12).map(|i| format!("token{i}")).collect())
            .await
            .unwrap();
        api.poll_qr(DEFAULT_API_BASE, &qr.qrcode, Some("12&34"))
            .await
            .unwrap();
        let update = api
            .updates(DEFAULT_API_BASE, "test-token", "previous")
            .await
            .unwrap();
        assert_eq!(update.get_updates_buf.as_deref(), Some("next"));
        assert_eq!(update.msgs[0].context_token.as_deref(), Some("context"));
        api.send(
            DEFAULT_API_BASE,
            "test-token",
            "user",
            "context",
            "发现余票",
        )
        .await
        .unwrap();
        let requests = server.await.unwrap();
        let header = |index: usize, key: &str| {
            requests[index]
                .0
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value.trim().to_string())
        };
        assert!(requests[0]
            .0
            .starts_with("POST /ilink/bot/get_bot_qrcode?bot_type=3 "));
        assert_eq!(
            requests[0].1["local_token_list"].as_array().unwrap().len(),
            10
        );
        assert!(requests[0].1.get("base_info").is_none());
        assert!(header(0, "authorization").is_none());
        assert!(requests[1].0.starts_with(
            "GET /ilink/bot/get_qrcode_status?qrcode=scan%26token%3D1&verify_code=12%2634 "
        ));
        assert!(header(1, "authorization").is_none());
        assert!(header(1, "authorizationtype").is_none());
        assert!(header(1, "x-wechat-uin").is_none());
        for index in [0, 2, 3] {
            assert_eq!(
                header(index, "authorizationtype").as_deref(),
                Some("ilink_bot_token")
            );
            let random = STANDARD
                .decode(header(index, "x-wechat-uin").unwrap())
                .unwrap();
            String::from_utf8(random).unwrap().parse::<u32>().unwrap();
        }
        for index in 0..4 {
            assert_eq!(header(index, "ilink-app-id").as_deref(), Some("bot"));
            assert_eq!(
                header(index, "ilink-app-clientversion").as_deref(),
                Some(CLIENT_VERSION)
            );
        }
        assert_eq!(
            header(2, "authorization").as_deref(),
            Some("Bearer test-token")
        );
        assert_eq!(requests[2].1["get_updates_buf"], "previous");
        assert_eq!(requests[3].1["msg"]["context_token"], "context");
        assert_eq!(requests[3].1["msg"]["to_user_id"], "user");
        assert_eq!(requests[3].1["msg"]["message_type"], 2);
        assert_eq!(requests[3].1["msg"]["message_state"], 2);
        assert_eq!(
            requests[3].1["msg"]["item_list"][0]["text_item"]["text"],
            "发现余票"
        );
    }

    #[tokio::test]
    async fn sends_require_positive_acknowledgement_and_redact_server_errors() {
        let (api, server) = server(vec![
            ok(json!({ "errmsg": "secret-token" })),
            ok(json!({ "ret": 0, "errcode": 9, "errmsg": "secret-token" })),
            ok(json!({ "ret": -14, "errmsg": "secret-token" })),
            (401, String::new(), "secret-token".into()),
        ])
        .await;
        for index in 0..4 {
            let error = api
                .send(DEFAULT_API_BASE, "secret-token", "user", "context", "test")
                .await
                .unwrap_err();
            assert!(!error.to_string().contains("secret-token"));
            if index >= 2 {
                assert!(matches!(error, ApiError::Expired));
            }
        }
        assert_eq!(server.await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn redirects_do_not_forward_authorization() {
        let (api, server) = server(vec![(
            302,
            "Location: https://example.com/collect\r\n".into(),
            String::new(),
        )])
        .await;
        assert!(matches!(
            api.send(DEFAULT_API_BASE, "secret-token", "user", "context", "test")
                .await,
            Err(ApiError::Failed(_))
        ));
        assert_eq!(server.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn oversized_and_malformed_responses_are_rejected_without_leaking_body() {
        let (api, server) = server(vec![
            (200, String::new(), "x".repeat(MAX_RESPONSE_BYTES + 1)),
            (200, String::new(), "secret-token invalid json".into()),
        ])
        .await;
        for _ in 0..2 {
            let error = match api.updates(DEFAULT_API_BASE, "test-token", "").await {
                Err(error) => error,
                Ok(_) => panic!("accepted invalid response"),
            };
            assert!(matches!(error, ApiError::Failed(_)));
            assert!(!error.to_string().contains("secret-token"));
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn response_body_timeout_is_distinct_and_does_not_retry() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = WechatApi::with_test_base(&format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            assert!(socket.read(&mut buffer).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{")
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let request = api
            .post(
                api.endpoint(DEFAULT_API_BASE, "ilink/bot/sendmessage")
                    .unwrap(),
                Some("test-token"),
            )
            .unwrap()
            .timeout(Duration::from_millis(30))
            .json(&json!({}));
        assert!(matches!(
            receive(request, true).await,
            Err(ApiError::Timeout)
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn updates_require_success_code_or_a_valid_message_list() {
        let (api, server) = server(vec![
            ok(json!({})),
            ok(json!({ "ret": null, "errcode": null })),
            ok(json!({ "ret": 0, "msgs": "invalid" })),
            ok(json!({ "ret": 0, "errcode": null })),
            ok(json!({ "ret": null, "errcode": null, "msgs": [] })),
            ok(json!({ "msgs": [{ "from_user_id": "user", "context_token": "context" }] })),
        ])
        .await;
        for _ in 0..3 {
            assert!(api
                .updates(DEFAULT_API_BASE, "test-token", "")
                .await
                .is_err());
        }
        for _ in 0..2 {
            assert!(api
                .updates(DEFAULT_API_BASE, "test-token", "")
                .await
                .unwrap()
                .msgs
                .is_empty());
        }
        assert_eq!(
            api.updates(DEFAULT_API_BASE, "test-token", "")
                .await
                .unwrap()
                .msgs
                .len(),
            1
        );
        server.await.unwrap();
    }

    #[test]
    fn business_errors_and_malformed_success_are_not_accepted() {
        for value in [
            json!({"ret": 1}),
            json!({"ret": 0, "errcode": 4}),
            json!({"ret": "0"}),
            json!({"ret": 0, "errcode": "0"}),
            json!({"ret": 0, "errcode": false}),
            json!({"ret": null}),
            json!([]),
        ] {
            assert!(check_result(&value, true).is_err());
        }
        assert!(matches!(
            check_result(&json!({"ret": 2, "errcode": -14}), false),
            Err(ApiError::Expired)
        ));
        assert!(check_result(&json!({"status": "wait"}), false).is_ok());
        assert!(check_result(
            &json!({"ret": null, "errcode": null, "status": "wait"}),
            false
        )
        .is_ok());
        assert!(check_result(&json!({"ret": 0, "errcode": null}), true).is_ok());
        assert!(check_result(&json!({"ret": 0}), true).is_ok());
    }
}
