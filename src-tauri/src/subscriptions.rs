use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use subscription_proxy_pool::{
    ProxyNode, ProxyPool, SubscriptionSource, SubscriptionUpdate, SubscriptionValidators,
    DEFAULT_MAX_SUBSCRIPTION_BYTES,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionNode {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionSnapshot {
    pub id: String,
    pub nodes: Vec<SubscriptionNode>,
    pub skipped: usize,
    pub updated_at: i64,
}

#[derive(Clone)]
struct LoadedSubscription {
    pool: ProxyPool,
    validators: SubscriptionValidators,
    skipped: usize,
}

struct SubscriptionEntry {
    source: SubscriptionSource,
    loaded: Mutex<Option<LoadedSubscription>>,
    refresh: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct SubscriptionManager {
    entries: Mutex<HashMap<String, Arc<SubscriptionEntry>>>,
}

impl SubscriptionManager {
    async fn refresh(&self, id: &str, url: &str) -> Result<SubscriptionSnapshot, String> {
        if id.trim().is_empty() || id.len() > 100 {
            return Err("订阅编号无效".into());
        }
        let source = SubscriptionSource::new(url.trim())
            .map_err(|_| "订阅地址需使用有效的 HTTP 或 HTTPS 链接")?;
        let entry = {
            let mut entries = self.entries.lock().map_err(|_| "订阅状态不可用")?;
            if let Some(entry) = entries
                .get(id)
                .filter(|entry| entry.source.key() == source.key())
            {
                entry.clone()
            } else {
                let entry = Arc::new(SubscriptionEntry {
                    source,
                    loaded: Mutex::new(None),
                    refresh: tokio::sync::Mutex::new(()),
                });
                entries.insert(id.to_owned(), entry.clone());
                entry
            }
        };
        let _refresh = entry.refresh.lock().await;
        self.ensure_current(id, &entry)?;
        let previous = entry.loaded.lock().map_err(|_| "订阅状态不可用")?.clone();
        let client = subscription_proxy_pool::reqwest::Client::builder()
            .no_proxy()
            .user_agent(ua_generator::ua::spoof_ua())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .redirect(subscription_proxy_pool::reqwest::redirect::Policy::limited(
                5,
            ))
            .build()
            .map_err(|_| "无法初始化订阅连接")?;
        let update = entry
            .source
            .fetch_update(
                &client,
                DEFAULT_MAX_SUBSCRIPTION_BYTES,
                previous.as_ref().map(|loaded| &loaded.validators),
            )
            .await
            .map_err(subscription_error)?;
        let loaded = match update {
            SubscriptionUpdate::Modified { report, validators } => LoadedSubscription {
                pool: ProxyPool::builder()
                    .nodes(report.nodes)
                    .connect_timeout(Duration::from_secs(5))
                    .request_timeout(Duration::from_secs(15))
                    .build()
                    .await
                    .map_err(|_| "无法创建订阅代理池")?,
                validators,
                skipped: report.skipped,
            },
            SubscriptionUpdate::NotModified => previous.ok_or("订阅缓存已失效，请重新刷新")?,
        };
        let snapshot = SubscriptionSnapshot {
            id: id.to_owned(),
            nodes: loaded.pool.nodes().iter().map(node_snapshot).collect(),
            skipped: loaded.skipped,
            updated_at: crate::clock::now_ms(),
        };
        let entries = self.entries.lock().map_err(|_| "订阅状态不可用")?;
        if !entries
            .get(id)
            .is_some_and(|current| Arc::ptr_eq(current, &entry))
        {
            return Err("订阅已被修改或删除，请重新刷新".into());
        }
        *entry.loaded.lock().map_err(|_| "订阅状态不可用")? = Some(loaded);
        Ok(snapshot)
    }

    fn ensure_current(&self, id: &str, entry: &Arc<SubscriptionEntry>) -> Result<(), String> {
        let entries = self.entries.lock().map_err(|_| "订阅状态不可用")?;
        if entries
            .get(id)
            .is_some_and(|current| Arc::ptr_eq(current, entry))
        {
            Ok(())
        } else {
            Err("订阅已被修改或删除，请重新刷新".into())
        }
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        self.entries
            .lock()
            .map_err(|_| "订阅状态不可用")?
            .remove(id);
        Ok(())
    }

    fn resolve(&self, id: &str, node_id: &str) -> Result<String, String> {
        let entry = self
            .entries
            .lock()
            .map_err(|_| "订阅状态不可用")?
            .get(id)
            .cloned()
            .ok_or("所选订阅尚未加载，请先刷新订阅")?;
        let loaded = entry.loaded.lock().map_err(|_| "订阅状态不可用")?;
        let loaded = loaded.as_ref().ok_or("所选订阅尚未加载，请先刷新订阅")?;
        if node_id.is_empty() {
            return loaded
                .pool
                .acquire()
                .map(|lease| lease.node().url().to_owned())
                .map_err(|_| "订阅暂无可用代理，请刷新或选择其他节点".into());
        }
        loaded
            .pool
            .nodes()
            .into_iter()
            .find(|node| node.id() == node_id)
            .map(|node| node.url().to_owned())
            .ok_or_else(|| "所选代理节点已失效，请重新选择节点".into())
    }
}

fn node_snapshot(node: &ProxyNode) -> SubscriptionNode {
    let url = reqwest::Url::parse(node.url()).expect("validated proxy node");
    SubscriptionNode {
        id: node.id(),
        name: node.name().to_owned(),
        protocol: url.scheme().to_owned(),
        host: url.host_str().unwrap_or_default().to_owned(),
        port: url.port_or_known_default().unwrap_or_default(),
    }
}

fn subscription_error(error: subscription_proxy_pool::Error) -> String {
    match error {
        subscription_proxy_pool::Error::Transport(error) if error.is_timeout() => {
            "订阅请求超时，请检查网络后重试".into()
        }
        subscription_proxy_pool::Error::Transport(error) => match error.status() {
            Some(status) => format!("订阅服务器返回 HTTP {}，请检查订阅地址和有效期", status.as_u16()),
            None => "无法下载订阅，请检查地址和网络连接".into(),
        },
        subscription_proxy_pool::Error::Subscription("subscription exceeds size limit") => {
            "订阅内容超过 4 MiB 限制".into()
        }
        subscription_proxy_pool::Error::Subscription(
            "subscription contains no supported valid proxy nodes",
        ) => "订阅中没有可用的 HTTP、HTTPS 或 SOCKS5 节点；不支持 SS、VMess、VLESS、Trojan 等协议".into(),
        _ => "订阅内容无法解析，请使用 Clash YAML/JSON 或 HTTP、SOCKS5 地址列表（可使用 Base64 编码）".into(),
    }
}

fn manager() -> &'static SubscriptionManager {
    static MANAGER: OnceLock<SubscriptionManager> = OnceLock::new();
    MANAGER.get_or_init(SubscriptionManager::default)
}

#[tauri::command]
pub async fn refresh_subscription(id: String, url: String) -> Result<SubscriptionSnapshot, String> {
    manager().refresh(&id, &url).await
}

#[tauri::command]
pub fn remove_subscription(id: String) -> Result<(), String> {
    manager().remove(&id)
}

pub fn resolve_proxy(subscription_id: &str, node_id: &str) -> Result<String, String> {
    manager().resolve(subscription_id, node_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
    };

    fn response(status: &str, headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
            body.len()
        )
    }

    async fn server(responses: Vec<String>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/subscription?token=private-token",
            listener.local_addr().unwrap()
        );
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut socket, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 2048];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&chunk[..count]);
                }
                requests.push(String::from_utf8(request).unwrap());
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        (url, task)
    }

    #[tokio::test]
    async fn subscriptions_hide_credentials_and_rotate_or_select_nodes() {
        let body = "proxies:\n  - {name: first, type: http, server: localhost, port: 8001, username: user, password: node-secret}\n  - {name: second, type: socks5, server: localhost, port: 8002}\n  - {type: ss, server: localhost, port: 8003, password: unsupported-secret}\n";
        let (url, server) = server(vec![response("200 OK", "", body)]).await;
        let manager = SubscriptionManager::default();
        let snapshot = manager.refresh("one", &url).await.unwrap();
        assert_eq!(snapshot.nodes.len(), 2);
        assert_eq!(snapshot.skipped, 1);
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("node-secret"));
        assert!(!serialized.contains("private-token"));
        assert!(!serialized.contains("username"));
        let first = manager.resolve("one", "").unwrap();
        let second = manager.resolve("one", "").unwrap();
        assert_ne!(first, second);
        assert_eq!(manager.resolve("one", "").unwrap(), first);
        assert_eq!(
            manager.resolve("one", &snapshot.nodes[1].id).unwrap(),
            second
        );
        assert!(manager.resolve("one", "unknown").is_err());
        let requests = server.await.unwrap();
        assert!(requests[0].to_lowercase().contains("user-agent: mozilla/"));
    }

    #[tokio::test]
    async fn conditional_refresh_and_failures_preserve_existing_nodes() {
        let (url, server) = server(vec![
            response(
                "200 OK",
                "ETag: \"revision-1\"\r\n",
                "http://localhost:8001#first",
            ),
            response("304 Not Modified", "", ""),
            response("503 Service Unavailable", "", "private error body"),
        ])
        .await;
        let manager = SubscriptionManager::default();
        let first = manager.refresh("one", &url).await.unwrap();
        let second = manager.refresh("one", &url).await.unwrap();
        assert_eq!(first.nodes[0].id, second.nodes[0].id);
        let error = manager.refresh("one", &url).await.err().unwrap();
        assert!(error.contains("503"));
        assert!(!error.contains("private"));
        assert!(manager.resolve("one", &first.nodes[0].id).is_ok());
        let requests = server.await.unwrap();
        assert!(!requests[0].to_lowercase().contains("if-none-match"));
        for request in &requests[1..] {
            assert!(request
                .to_lowercase()
                .contains("if-none-match: \"revision-1\""));
            assert!(request.to_lowercase().contains("user-agent: mozilla/"));
        }
    }

    #[tokio::test]
    async fn changing_source_does_not_reuse_old_credentials_or_validators() {
        let (first_url, first_server) = server(vec![response(
            "200 OK",
            "ETag: old\r\n",
            "http://user:old-secret@localhost:8001",
        )])
        .await;
        let (second_url, second_server) =
            server(vec![response("500 Internal Server Error", "", "")]).await;
        let manager = SubscriptionManager::default();
        manager.refresh("one", &first_url).await.unwrap();
        assert!(manager.refresh("one", &second_url).await.is_err());
        assert!(manager.resolve("one", "").is_err());
        first_server.await.unwrap();
        let requests = second_server.await.unwrap();
        assert!(!requests[0].to_lowercase().contains("if-none-match"));
    }

    #[tokio::test]
    async fn deleting_a_subscription_prevents_an_inflight_refresh_from_restoring_it() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let manager = Arc::new(SubscriptionManager::default());
        let refreshing = manager.clone();
        let task = tokio::spawn(async move { refreshing.refresh("one", &url).await });
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut request = [0; 2048];
        socket.read(&mut request).await.unwrap();
        manager.remove("one").unwrap();
        socket
            .write_all(response("200 OK", "", "http://localhost:8001").as_bytes())
            .await
            .unwrap();
        assert!(task.await.unwrap().is_err());
        assert!(manager.resolve("one", "").is_err());
    }

    #[tokio::test]
    async fn rejects_unsupported_subscriptions_without_leaking_content() {
        let (url, server) = server(vec![response(
            "200 OK",
            "",
            "ss://private-secret@localhost:8001",
        )])
        .await;
        let manager = SubscriptionManager::default();
        let error = manager.refresh("one", &url).await.err().unwrap();
        assert!(error.contains("没有可用"));
        assert!(!error.contains("private-secret"));
        assert!(manager.resolve("one", "").is_err());
        server.await.unwrap();
        assert!(manager
            .refresh("one", "file:///tmp/subscription")
            .await
            .is_err());
    }
}
