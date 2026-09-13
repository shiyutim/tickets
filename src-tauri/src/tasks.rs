use crate::{
    bilibili, clock, dm, monitor,
    notifications::{self, WechatConfig},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{oneshot, watch};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRequest {
    #[serde(default)]
    pub mode: String,
    pub id: String,
    pub platform: String,
    pub title: String,
    pub start_at: i64,
    pub offset_ms: i64,
    pub max_attempts: u32,
    pub interval_ms: u64,
    pub config: Value,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub mode: String,
    pub id: String,
    pub platform: String,
    pub title: String,
    pub status: String,
    pub message: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub interval_ms: u64,
    pub end_at: i64,
    pub start_at: i64,
    pub offset_ms: i64,
    pub updated_at: i64,
    pub revision: u64,
    pub order_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_status: Option<String>,
}

impl TaskSnapshot {
    pub fn active(&self) -> bool {
        matches!(self.status.as_str(), "waiting" | "running")
    }
}

struct TaskEntry {
    snapshot: TaskSnapshot,
    cancel: watch::Sender<bool>,
}

#[derive(Clone, Default)]
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    credentials: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
    sequence: Arc<AtomicU64>,
}

pub struct TaskContext {
    pub app: AppHandle,
    pub manager: TaskManager,
    pub request: TaskRequest,
}

pub struct Outcome {
    pub status: &'static str,
    pub message: String,
    pub order_url: Option<String>,
}

impl Outcome {
    pub fn action(message: impl Into<String>, order_url: String) -> Self {
        Self {
            status: "needs_action",
            message: message.into(),
            order_url: Some(order_url),
        }
    }
}

impl TaskContext {
    pub fn report(
        &self,
        status: &str,
        message: impl Into<String>,
        attempt: u32,
        order_url: Option<String>,
    ) {
        self.report_with_notification(status, message, attempt, order_url, None);
    }

    fn report_with_notification(
        &self,
        status: &str,
        message: impl Into<String>,
        attempt: u32,
        order_url: Option<String>,
        notification_status: Option<&str>,
    ) {
        let snapshot = {
            let Ok(mut tasks) = self.manager.tasks.lock() else {
                return;
            };
            let Some(entry) = tasks.get_mut(&self.request.id) else {
                return;
            };
            entry.snapshot.status = status.into();
            entry.snapshot.message = message.into();
            entry.snapshot.attempt = attempt;
            entry.snapshot.updated_at = clock::now_ms();
            entry.snapshot.revision += 1;
            entry.snapshot.order_url = order_url;
            entry.snapshot.notification_status = notification_status.map(str::to_string);
            entry.snapshot.clone()
        };
        let _ = self.app.emit_all("ticket-task", snapshot);
    }

    pub async fn credentials(&self) -> Result<Value, String> {
        self.credentials_with_user_agent("").await
    }

    pub async fn credentials_with_user_agent(&self, user_agent: &str) -> Result<Value, String> {
        let seq = self.manager.sequence.fetch_add(1, Ordering::Relaxed);
        let request_id = format!("{}:{seq}", self.request.id);
        let (sender, receiver) = oneshot::channel();
        self.manager
            .credentials
            .lock()
            .map_err(|_| "凭证状态不可用")?
            .insert(request_id.clone(), sender);
        let emitted = self.app.emit_all(
            "ticket-credentials",
            json!({
                "requestId": request_id, "platform": self.request.platform,
                "projectId": self.request.config["projectId"],
                "userAgent": user_agent,
            }),
        );
        let result = if emitted.is_err() {
            Err("无法请求页面凭证".to_string())
        } else {
            match tokio::time::timeout(Duration::from_secs(15), receiver).await {
                Ok(Ok(value)) if value["error"].is_string() => {
                    Err(value["error"].as_str().unwrap_or_default().into())
                }
                Ok(Ok(value)) => Ok(value),
                _ => Err("页面凭证获取超时，请保持应用运行后重试".into()),
            }
        };
        if let Ok(mut pending) = self.manager.credentials.lock() {
            pending.remove(&request_id);
        }
        result
    }

    pub async fn pause(&self, milliseconds: u64) {
        tokio::time::sleep(Duration::from_millis(milliseconds)).await;
    }
}

fn validate(request: &TaskRequest) -> Result<(), String> {
    if request.id.is_empty() || request.id.len() > 100 || request.id.contains(':') {
        return Err("任务编号无效".into());
    }
    let monitoring = request.mode == "monitor";
    if !matches!(request.mode.as_str(), "" | "purchase" | "monitor") {
        return Err("不支持的任务类型".into());
    }
    if monitoring && request.max_attempts > 100_000 {
        return Err("最多查询次数不能超过 100000".into());
    }
    if !monitoring && !(1..=100).contains(&request.max_attempts) {
        return Err("尝试次数必须在 1–100 之间".into());
    }
    if monitoring && !(5_000..=3_600_000).contains(&request.interval_ms) {
        return Err("监控间隔必须在 5–3600 秒之间".into());
    }
    if !monitoring && !(300..=60_000).contains(&request.interval_ms) {
        return Err("重试间隔必须在 300–60000 毫秒之间".into());
    }
    if !(-86_400_000..=86_400_000).contains(&request.offset_ms) || request.start_at < 0 {
        return Err("定时时间或修正值无效".into());
    }
    if clock::delay_ms(request.start_at, request.offset_ms, clock::now_ms()) > 30 * 86_400_000 {
        return Err("仅支持预约未来 30 天内的任务".into());
    }
    if monitoring {
        monitor::validate(&request.config, &request.platform)?;
        let end = request.config["endAt"].as_i64().unwrap_or(0);
        if end > 0
            && (end <= request.start_at
                || clock::delay_ms(end, request.offset_ms, clock::now_ms()) == 0)
        {
            return Err("结束时间必须晚于开始时间和当前时间".into());
        }
        return Ok(());
    }
    match request.platform.as_str() {
        "dm" => dm::validate(&request.config),
        "bilibili" => bilibili::validate(&request.config),
        _ => Err("不支持的购票平台".into()),
    }
}

fn pin_subscription_proxy(config: &mut Value) -> Result<(), String> {
    let account = config
        .get_mut("account")
        .and_then(Value::as_object_mut)
        .ok_or("任务账号配置不完整")?;
    let subscription_id = account
        .get("subscriptionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if subscription_id.is_empty() {
        return Ok(());
    }
    let node_id = account
        .get("subscriptionNodeId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let proxy = crate::subscriptions::resolve_proxy(subscription_id, node_id)?;
    account.insert("proxy".into(), json!(proxy));
    account.insert("subscriptionId".into(), json!(""));
    account.insert("subscriptionNodeId".into(), json!(""));
    Ok(())
}

impl TaskManager {
    fn reserve(
        &self,
        request: &TaskRequest,
    ) -> Result<(TaskSnapshot, watch::Receiver<bool>), String> {
        let (cancel, receiver) = watch::channel(false);
        let snapshot = TaskSnapshot {
            mode: request.mode.clone(),
            id: request.id.clone(),
            platform: request.platform.clone(),
            title: request.title.clone(),
            status: "waiting".into(),
            message: "任务已就绪，等待开始".into(),
            attempt: 0,
            max_attempts: request.max_attempts,
            interval_ms: request.interval_ms,
            end_at: if request.mode == "monitor" {
                request.config["endAt"].as_i64().unwrap_or(0)
            } else {
                0
            },
            start_at: request.start_at,
            offset_ms: request.offset_ms,
            updated_at: clock::now_ms(),
            revision: 0,
            order_url: None,
            notification_status: None,
        };
        {
            let mut tasks = self.tasks.lock().map_err(|_| "任务状态不可用")?;
            if tasks.contains_key(&request.id) {
                return Err("任务编号已存在".into());
            }
            if request.mode != "monitor"
                && tasks.values().any(|t| {
                    t.snapshot.mode != "monitor"
                        && t.snapshot.platform == request.platform
                        && t.snapshot.active()
                })
            {
                return Err("该平台已有运行中的购票任务，请先停止".into());
            }
            if tasks.len() >= 100 {
                let oldest = tasks
                    .iter()
                    .filter(|(_, v)| {
                        !v.snapshot.active()
                            && v.snapshot.notification_status.as_deref() != Some("pending")
                    })
                    .min_by_key(|(_, v)| v.snapshot.updated_at)
                    .map(|(k, _)| k.clone());
                if let Some(id) = oldest {
                    tasks.remove(&id);
                } else {
                    return Err("同时运行的任务已达 100 个，请先停止部分任务".into());
                }
            }
            tasks.insert(
                request.id.clone(),
                TaskEntry {
                    snapshot: snapshot.clone(),
                    cancel,
                },
            );
        }
        Ok((snapshot, receiver))
    }

    fn cancel(&self, id: &str) -> Result<(), String> {
        let tasks = self.tasks.lock().map_err(|_| "任务状态不可用")?;
        let entry = tasks.get(id).ok_or("任务不存在")?;
        if entry.snapshot.active() {
            let _ = entry.cancel.send(true);
        }
        Ok(())
    }
}

async fn run_cancellable(
    mut cancel: watch::Receiver<bool>,
    run: impl std::future::Future<Output = Result<Outcome, String>>,
) -> Option<Result<Outcome, String>> {
    if *cancel.borrow() {
        return None;
    }
    tokio::select! {
        biased;
        _ = cancel.changed() => None,
        result = run => Some(result),
    }
}

async fn finish_task<P, S, F>(request: &TaskRequest, mut outcome: Outcome, mut publish: P, send: S)
where
    P: FnMut(&Outcome, Option<&str>),
    S: FnOnce(WechatConfig, String) -> F,
    F: std::future::Future<Output = Result<(), String>>,
{
    let monitoring = request.mode == "monitor";
    if outcome.status != if monitoring { "found" } else { "succeeded" } {
        publish(&outcome, None);
        return;
    }
    if request.config["wechat"]["enabled"].as_bool() != Some(true) {
        outcome
            .message
            .push_str("；微信通知未发送：任务启动时未启用提醒");
        publish(&outcome, Some("disabled"));
        return;
    }
    let original_message = outcome.message.clone();
    outcome.message = format!("{original_message}；正在发送微信通知");
    publish(&outcome, Some("pending"));
    let config = serde_json::from_value::<WechatConfig>(request.config["wechat"].clone())
        .map_err(|_| "微信通知配置无效，请检查通知设置".to_string());
    let result = match config {
        Ok(config) => match config.validate() {
            Ok(()) => {
                let platform = if request.platform == "dm" {
                    "大麦"
                } else {
                    "哔哩哔哩会员购"
                };
                let url = outcome.order_url.clone().unwrap_or_else(|| {
                    if monitoring {
                        monitor::project_url(
                            &request.platform,
                            &crate::http::string(&request.config["projectId"]),
                        )
                    } else {
                        order_list_url(&request.platform).into()
                    }
                });
                let message = if monitoring {
                    format!("【Tickets 余票提醒】\n{platform}\n{}\n检测到可购票档，请及时到官方页面确认。库存随时变化。\n{url}", request.title)
                } else {
                    format!("【Tickets 购票提醒】\n{platform}\n{}\n订单已创建，尚未支付，请及时前往官方页面完成支付。\n{url}", request.title)
                };
                // 先公布任务结果；发送结果未知时不重试，避免重复通知。
                send(config, message).await
            }
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    };
    let status = match result {
        Ok(()) => {
            outcome.message = format!("{original_message}；微信通知已发送");
            "sent"
        }
        Err(error) => {
            outcome.message = format!("{original_message}；微信通知未确认送达：{error}");
            "failed"
        }
    };
    publish(&outcome, Some(status));
}

#[tauri::command]
pub fn start_ticket_task(
    app: AppHandle,
    state: State<'_, TaskManager>,
    mut request: TaskRequest,
) -> Result<TaskSnapshot, String> {
    pin_subscription_proxy(&mut request.config)?;
    validate(&request)?;
    if request.mode == "monitor" {
        let config: crate::notifications::WechatConfig = serde_json::from_value(
            request
                .config
                .get("wechat")
                .cloned()
                .unwrap_or_else(|| json!({})),
        )
        .map_err(|_| "微信通知配置无效")?;
        app.state::<crate::notifications::WechatManager>()
            .check(&config)?;
    }
    let manager = state.inner().clone();
    let (snapshot, receiver) = manager.reserve(&request)?;
    tauri::async_runtime::spawn(async move {
        let context = TaskContext {
            app,
            manager,
            request,
        };
        let run = async {
            let delay = clock::delay_ms(
                context.request.start_at,
                context.request.offset_ms,
                clock::now_ms(),
            );
            context.pause(delay).await;
            if context.request.mode == "monitor" {
                context.report("running", "开始查询余票", 0, None);
                return monitor::run(&context).await;
            }
            context.report("running", "开始执行购票任务", 0, None);
            match context.request.platform.as_str() {
                "dm" => dm::run(&context).await,
                _ => bilibili::run(&context).await,
            }
        };
        let outcome =
            match run_cancellable(receiver, run).await {
                None => {
                    if context.request.mode == "monitor" {
                        Outcome {
                            status: "cancelled",
                            message: "余票监控已停止".into(),
                            order_url: None,
                        }
                    } else {
                        let known_order = context.manager.tasks.lock().ok().and_then(|tasks| {
                            tasks
                                .get(&context.request.id)
                                .and_then(|entry| entry.snapshot.order_url.clone())
                        });
                        Outcome {
                            status: if known_order.is_some() {
                                "succeeded"
                            } else {
                                "cancelled"
                            },
                            message: if known_order.is_some() {
                                "订单已创建，已停止后续查询，请前往支付"
                            } else {
                                "任务已停止；已发出的订单请求请在官方订单页确认"
                            }
                            .into(),
                            order_url: Some(known_order.unwrap_or_else(|| {
                                order_list_url(&context.request.platform).into()
                            })),
                        }
                    }
                }
                Some(result) => result.unwrap_or_else(|message| Outcome {
                    status: "failed",
                    message,
                    order_url: None,
                }),
            };
        let attempt = context
            .manager
            .tasks
            .lock()
            .ok()
            .and_then(|t| t.get(&context.request.id).map(|t| t.snapshot.attempt))
            .unwrap_or(0);
        if let Ok(mut pending) = context.manager.credentials.lock() {
            pending.retain(|key, _| !key.starts_with(&format!("{}:", context.request.id)));
        };
        let app = context.app.clone();
        finish_task(
            &context.request,
            outcome,
            |outcome, notification_status| {
                context.report_with_notification(
                    outcome.status,
                    &outcome.message,
                    attempt,
                    outcome.order_url.clone(),
                    notification_status,
                )
            },
            |config, message| async move { notifications::send(&app, &config, &message).await },
        )
        .await;
    });
    Ok(snapshot)
}

pub fn order_list_url(platform: &str) -> &'static str {
    if platform == "dm" {
        "https://orders.damai.cn/orderList"
    } else {
        "https://show.bilibili.com/platform/orderList.html"
    }
}

#[tauri::command]
pub fn cancel_ticket_task(state: State<'_, TaskManager>, id: String) -> Result<(), String> {
    state.cancel(&id)
}

#[tauri::command]
pub fn list_ticket_tasks(state: State<'_, TaskManager>) -> Result<Vec<TaskSnapshot>, String> {
    Ok(state
        .tasks
        .lock()
        .map_err(|_| "任务状态不可用")?
        .values()
        .map(|t| t.snapshot.clone())
        .collect())
}

#[tauri::command]
pub fn provide_ticket_credentials(
    state: State<'_, TaskManager>,
    request_id: String,
    credentials: Value,
) -> Result<(), String> {
    if let Some(sender) = state
        .credentials
        .lock()
        .map_err(|_| "凭证状态不可用")?
        .remove(&request_id)
    {
        let _ = sender.send(credentials);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn request(platform: &str, id: &str) -> TaskRequest {
        TaskRequest {
            mode: "purchase".into(),
            id: id.into(),
            platform: platform.into(),
            title: "测试任务".into(),
            start_at: 0,
            offset_ms: 0,
            max_attempts: 3,
            interval_ms: 1000,
            config: Value::Null,
        }
    }

    #[tokio::test]
    async fn purchase_success_is_published_before_sending_one_notification() {
        for platform in ["dm", "bilibili"] {
            let mut task = request(platform, "purchase-notice");
            task.title = "测试演出 · 周六场 · 看台票".into();
            task.config = json!({"wechat":{"enabled":true,"accountId":"bot-1","target":"owner@im.wechat"},
                "account":{"cookie":"synthetic-cookie"},"buyers":[{"id":"synthetic-personal-id"}]});
            if platform == "dm" {
                task.mode.clear();
            }
            let published = Arc::new(Mutex::new(Vec::new()));
            let during_send = published.clone();
            let expected_url = format!("{}/test-order", order_list_url(platform));
            let outcome = Outcome {
                status: "succeeded",
                message: "订单已创建，请前往支付".into(),
                order_url: Some(expected_url.clone()),
            };
            finish_task(
                &task,
                outcome,
                |outcome, notification| {
                    published.lock().unwrap().push((
                        outcome.status.to_string(),
                        outcome.message.clone(),
                        outcome.order_url.clone(),
                        notification.map(str::to_string),
                    ));
                },
                |config, text| async move {
                    let snapshots = during_send.lock().unwrap();
                    assert_eq!(snapshots.len(), 1);
                    assert_eq!(snapshots[0].0, "succeeded");
                    assert_eq!(snapshots[0].2.as_deref(), Some(expected_url.as_str()));
                    assert_eq!(snapshots[0].3.as_deref(), Some("pending"));
                    assert_eq!(config.account_id, "bot-1");
                    assert_eq!(config.target, "owner@im.wechat");
                    assert!(text.contains("测试演出 · 周六场 · 看台票"));
                    assert!(text.contains(if platform == "dm" {
                        "大麦"
                    } else {
                        "哔哩哔哩会员购"
                    }));
                    assert!(text.contains("尚未支付"));
                    assert!(text.contains(&expected_url));
                    assert!(!text.contains("synthetic-cookie"));
                    assert!(!text.contains("synthetic-personal-id"));
                    Ok(())
                },
            )
            .await;
            let snapshots = published.lock().unwrap();
            assert_eq!(snapshots.len(), 2);
            assert_eq!(snapshots[1].0, "succeeded");
            assert_eq!(snapshots[1].3.as_deref(), Some("sent"));
            assert!(snapshots[1].1.contains("微信通知已发送"));
        }
    }

    #[tokio::test]
    async fn found_tickets_are_published_before_wechat_finishes_and_survive_notification_failure() {
        for platform in ["dm", "bilibili"] {
            for send_error in [None, Some("微信发送超时，送达状态未知")] {
                let mut task = request(platform, "monitor-notice");
                task.mode = "monitor".into();
                task.title = "测试演出 · 周六场 · 看台票".into();
                task.config = json!({"projectId":"123","wechat":{
                    "enabled":true,"accountId":"bot-1","target":"owner@im.wechat"
                },"account":{"cookie":"synthetic-cookie"}});
                let manager = TaskManager::default();
                let (_, cancel) = manager.reserve(&task).unwrap();
                let publishing_manager = manager.clone();
                let published = Arc::new(Mutex::new(Vec::new()));
                let published_by_task = published.clone();
                let calls = Arc::new(AtomicU64::new(0));
                let calls_by_task = calls.clone();
                let url = monitor::project_url(platform, "123");
                let expected_url = url.clone();
                let (sending, send_started) = oneshot::channel();
                let (release, released) = oneshot::channel();
                let finishing = tokio::spawn(async move {
                    finish_task(
                        &task,
                        Outcome {
                            status: "found",
                            message: "发现可购票档，监控已结束，请前往官方页面确认".into(),
                            order_url: Some(url),
                        },
                        |outcome, notification| {
                            let mut tasks = publishing_manager.tasks.lock().unwrap();
                            let snapshot = &mut tasks.get_mut(&task.id).unwrap().snapshot;
                            snapshot.status = outcome.status.into();
                            snapshot.order_url = outcome.order_url.clone();
                            snapshot.notification_status = notification.map(str::to_string);
                            published_by_task.lock().unwrap().push((
                                outcome.status.to_string(),
                                outcome.message.clone(),
                                outcome.order_url.clone(),
                                notification.map(str::to_string),
                            ));
                        },
                        |config, text| async move {
                            calls_by_task.fetch_add(1, Ordering::SeqCst);
                            assert_eq!(config.account_id, "bot-1");
                            assert_eq!(config.target, "owner@im.wechat");
                            assert!(text.contains("【Tickets 余票提醒】"));
                            assert!(text.contains("测试演出 · 周六场 · 看台票"));
                            assert!(text.contains(&expected_url));
                            assert!(!text.contains("订单已创建"));
                            assert!(!text.contains("synthetic-cookie"));
                            sending.send(()).unwrap();
                            released.await.unwrap();
                            send_error.map_or(Ok(()), |error| Err(error.into()))
                        },
                    )
                    .await;
                });
                send_started.await.unwrap();
                assert!(!finishing.is_finished());
                {
                    let snapshots = published.lock().unwrap();
                    assert_eq!(snapshots.len(), 1);
                    assert_eq!(snapshots[0].0, "found");
                    assert_eq!(snapshots[0].3.as_deref(), Some("pending"));
                }
                // 发现票档即完成监控；旧的停止操作不能中断通知或覆盖已发现结果。
                manager.cancel("monitor-notice").unwrap();
                assert!(!*cancel.borrow());
                release.send(()).unwrap();
                finishing.await.unwrap();
                let snapshots = published.lock().unwrap();
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert_eq!(snapshots.len(), 2);
                assert_eq!(snapshots[1].0, "found");
                assert_eq!(snapshots[1].2, snapshots[0].2);
                assert_eq!(
                    snapshots[1].3.as_deref(),
                    Some(if send_error.is_some() {
                        "failed"
                    } else {
                        "sent"
                    }),
                );
                assert!(snapshots[1].1.starts_with("发现可购票档，监控已结束"));
                if let Some(error) = send_error {
                    assert!(snapshots[1].1.contains(error));
                }
            }
        }
    }

    #[tokio::test]
    async fn notification_errors_preserve_the_order_and_never_retry() {
        for (wechat, error, expected_calls) in [
            (
                json!({"enabled":true,"accountId":"bot-1","target":"owner@im.wechat"}),
                "微信登录已失效",
                1,
            ),
            (
                json!({"enabled":true,"accountId":"bot-1","target":"owner@im.wechat"}),
                "微信发送超时，送达状态未知",
                1,
            ),
            (json!({"enabled":true,"accountId":"","target":""}), "", 0),
        ] {
            let mut task = request("bilibili", "failed-notice");
            task.config = json!({"wechat":wechat});
            let calls = Arc::new(AtomicU64::new(0));
            let count = calls.clone();
            let mut states = Vec::new();
            let url = "https://show.bilibili.com/platform/orderDetail.html?order_id=123";
            finish_task(
                &task,
                Outcome {
                    status: "succeeded",
                    message: "订单已创建，请前往支付".into(),
                    order_url: Some(url.into()),
                },
                |outcome, status| {
                    states.push((
                        outcome.status.to_string(),
                        outcome.message.clone(),
                        outcome.order_url.clone(),
                        status.map(str::to_string),
                    ))
                },
                |_, _| async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Err(error.into())
                },
            )
            .await;
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
            assert_eq!(states.len(), 2);
            assert_eq!(states[1].0, "succeeded");
            assert_eq!(states[1].2.as_deref(), Some(url));
            assert_eq!(states[1].3.as_deref(), Some("failed"));
            assert!(states[1].1.starts_with("订单已创建，请前往支付"));
            if !error.is_empty() {
                assert!(states[1].1.contains(error));
            }
        }
    }

    #[tokio::test]
    async fn disabled_notifications_keep_the_successful_result_without_sending() {
        for platform in ["dm", "bilibili"] {
            for (mode, status) in [("purchase", "succeeded"), ("monitor", "found")] {
                for wechat in [Value::Null, json!({}), json!({"enabled":false})] {
                    let mut task = request(platform, "disabled-notice");
                    task.mode = mode.into();
                    task.config = json!({"wechat":wechat});
                    let mut reports = 0;
                    let url = order_list_url(platform);
                    finish_task(
                        &task,
                        Outcome {
                            status,
                            message: "任务结果已确认".into(),
                            order_url: Some(url.into()),
                        },
                        |outcome, notification| {
                            reports += 1;
                            assert_eq!(outcome.status, status);
                            assert_eq!(outcome.order_url.as_deref(), Some(url));
                            assert_eq!(notification, Some("disabled"));
                            assert!(outcome.message.contains("任务启动时未启用提醒"));
                        },
                        |_, _| async {
                            panic!("未启用提醒的任务不应发送消息");
                            #[allow(unreachable_code)]
                            Ok(())
                        },
                    )
                    .await;
                    assert_eq!(reports, 1);
                }
            }
        }
    }

    #[tokio::test]
    async fn unsuccessful_or_cancelled_tasks_do_not_send_notifications() {
        for (mode, status, enabled) in [
            ("purchase", "failed", true),
            ("purchase", "cancelled", true),
            ("purchase", "needs_action", true),
            ("purchase", "found", true),
            ("monitor", "completed", true),
            ("monitor", "cancelled", true),
            ("monitor", "failed", true),
            ("monitor", "needs_action", true),
            ("monitor", "succeeded", true),
        ] {
            let mut task = request("dm", "not-notified");
            task.mode = mode.into();
            task.config = json!({"wechat":{"enabled":enabled}});
            let mut reports = 0;
            finish_task(
                &task,
                Outcome {
                    status,
                    message: "原始结果".into(),
                    order_url: None,
                },
                |outcome, notification| {
                    reports += 1;
                    assert_eq!(outcome.status, status);
                    assert_eq!(outcome.message, "原始结果");
                    assert!(notification.is_none());
                },
                |_, _| async {
                    panic!("不应发送微信通知");
                    #[allow(unreachable_code)]
                    Ok(())
                },
            )
            .await;
            assert_eq!(reports, 1);
        }
    }

    #[tokio::test]
    async fn scheduled_tasks_keep_their_proxy_after_subscription_removal() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 2048];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
            }
            let body = "http://local-user:local-password@127.0.0.1:8123#test-node";
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let id = "scheduled-task-proxy-test";
        crate::subscriptions::refresh_subscription(id.into(), url)
            .await
            .unwrap();
        let mut task = request("dm", "pinned-proxy");
        task.config = json!({ "account": {
            "cookie": "session=private-cookie", "proxy": "http://127.0.0.1:9000",
            "subscriptionId": id, "subscriptionNodeId": "",
        }});
        pin_subscription_proxy(&mut task.config).unwrap();
        crate::subscriptions::remove_subscription(id.into()).unwrap();
        assert!(crate::subscriptions::resolve_proxy(id, "").is_err());
        let account: crate::http::Account =
            serde_json::from_value(task.config["account"].clone()).unwrap();
        assert_eq!(
            account.proxy,
            "http://local-user:local-password@127.0.0.1:8123/"
        );
        assert!(account.subscription_id.is_empty());
        assert!(account.subscription_node_id.is_empty());
        assert!(crate::http::client(&account, "https://m.damai.cn").is_ok());
        let (snapshot, _) = TaskManager::default().reserve(&task).unwrap();
        let serialized = serde_json::to_string(&snapshot).unwrap();
        for secret in [
            "local-user",
            "local-password",
            "private-cookie",
            "127.0.0.1:8123",
        ] {
            assert!(!serialized.contains(secret));
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn only_one_active_task_per_platform_and_cancel_targets_one_task() {
        let manager = TaskManager::default();
        let (_, first) = manager.reserve(&request("dm", "first")).unwrap();
        assert!(manager.reserve(&request("dm", "duplicate")).is_err());
        let (_, other) = manager.reserve(&request("bilibili", "other")).unwrap();
        manager.cancel("first").unwrap();
        assert!(*first.borrow());
        assert!(!*other.borrow());
        assert!(manager.cancel("missing").is_err());
    }

    #[tokio::test]
    async fn monitors_coexist_on_same_platform_with_a_purchase_and_cancel_individually() {
        let manager = TaskManager::default();
        let mut first = request("dm", "monitor-one");
        first.mode = "monitor".into();
        first.config = json!({"endAt":123456789});
        let (snapshot, first_cancel) = manager.reserve(&first).unwrap();
        assert_eq!(snapshot.interval_ms, first.interval_ms);
        assert_eq!(snapshot.end_at, 123456789);
        let (_, purchase_cancel) = manager.reserve(&request("dm", "purchase")).unwrap();
        first.id = "monitor-two".into();
        let (_, second_cancel) = manager.reserve(&first).unwrap();
        assert!(manager.reserve(&request("dm", "second-purchase")).is_err());
        manager.cancel("monitor-one").unwrap();
        assert!(*first_cancel.borrow());
        assert!(!*second_cancel.borrow());
        assert!(!*purchase_cancel.borrow());
        assert_eq!(manager.tasks.lock().unwrap().len(), 3);
    }

    #[test]
    fn active_task_capacity_is_bounded_and_completed_entries_can_be_replaced() {
        let manager = TaskManager::default();
        for n in 0..100 {
            let mut task = request("bilibili", &format!("monitor-{n}"));
            task.mode = "monitor".into();
            manager.reserve(&task).unwrap();
        }
        let mut next = request("bilibili", "monitor-next");
        next.mode = "monitor".into();
        assert!(manager.reserve(&next).err().unwrap().contains("100"));
        manager
            .tasks
            .lock()
            .unwrap()
            .get_mut("monitor-0")
            .unwrap()
            .snapshot
            .status = "found".into();
        manager
            .tasks
            .lock()
            .unwrap()
            .get_mut("monitor-0")
            .unwrap()
            .snapshot
            .notification_status = Some("pending".into());
        assert!(manager.reserve(&next).is_err());
        manager
            .tasks
            .lock()
            .unwrap()
            .get_mut("monitor-0")
            .unwrap()
            .snapshot
            .notification_status = Some("sent".into());
        assert!(manager.reserve(&next).is_ok());
        let tasks = manager.tasks.lock().unwrap();
        assert_eq!(tasks.len(), 100);
        assert!(!tasks.contains_key("monitor-0"));
    }

    #[tokio::test]
    async fn cancellation_prevents_a_scheduled_request_from_running() {
        let (sender, receiver) = watch::channel(false);
        let requested = Arc::new(AtomicBool::new(false));
        let flag = requested.clone();
        let task = tokio::spawn(async move {
            run_cancellable(receiver, async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                flag.store(true, Ordering::SeqCst);
                Err("不应执行".into())
            })
            .await
        });
        tokio::task::yield_now().await;
        sender.send(true).unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(100), task)
            .await
            .unwrap()
            .unwrap()
            .is_none());
        assert!(!requested.load(Ordering::SeqCst));
    }

    #[test]
    fn invalid_timing_and_retry_values_are_rejected_without_overflow() {
        let mut config = request("dm", "invalid");
        config.offset_ms = i64::MIN;
        assert!(validate(&config).unwrap_err().contains("修正值"));
        config.offset_ms = 0;
        config.max_attempts = 0;
        assert!(validate(&config).unwrap_err().contains("尝试次数"));
        config.max_attempts = 3;
        config.interval_ms = 0;
        assert!(validate(&config).unwrap_err().contains("重试间隔"));
    }

    #[test]
    fn monitor_validation_is_independent_of_purchase_credentials() {
        let mut task = request("dm", "monitor-validation");
        task.mode = "monitor".into();
        task.interval_ms = 60_000;
        task.max_attempts = 0;
        task.config = json!({"account":{"cookie":"_m_h5_tk=mock_999"},"projectId":"1","screenId":"2","skuId":"3","endAt":0});
        assert!(validate(&task).is_ok());
        task.platform = "bilibili".into();
        assert!(validate(&task).is_ok());
        task.interval_ms = 4999;
        assert!(validate(&task).unwrap_err().contains("监控间隔"));
        task.interval_ms = 5000;
        task.config["endAt"] = json!(clock::now_ms() - 1000);
        assert!(validate(&task).unwrap_err().contains("结束时间"));
        task.config["endAt"] = json!(clock::now_ms() + 60_000);
        task.start_at = clock::now_ms() + 120_000;
        assert!(validate(&task).unwrap_err().contains("结束时间"));
        task.config["endAt"] = json!(0);
        task.config["wechat"] = json!({"enabled":true,"executable":"openclaw"});
        assert!(validate(&task).unwrap_err().contains("接收人"));
        task.config["wechat"] = json!({"enabled":false});
        task.mode = "purchase".into();
        task.max_attempts = 1;
        assert!(validate(&task).is_err());
    }
}
