use crate::{bilibili, clock, dm};
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
    pub id: String,
    pub platform: String,
    pub title: String,
    pub status: String,
    pub message: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub start_at: i64,
    pub offset_ms: i64,
    pub updated_at: i64,
    pub revision: u64,
    pub order_url: Option<String>,
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
            entry.snapshot.clone()
        };
        let _ = self.app.emit_all("ticket-task", snapshot);
    }

    pub async fn credentials(&self) -> Result<Value, String> {
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
    if !(1..=100).contains(&request.max_attempts) {
        return Err("尝试次数必须在 1–100 之间".into());
    }
    if !(300..=60_000).contains(&request.interval_ms) {
        return Err("重试间隔必须在 300–60000 毫秒之间".into());
    }
    if !(-86_400_000..=86_400_000).contains(&request.offset_ms) || request.start_at < 0 {
        return Err("定时时间或修正值无效".into());
    }
    if clock::delay_ms(request.start_at, request.offset_ms, clock::now_ms()) > 30 * 86_400_000 {
        return Err("仅支持预约未来 30 天内的任务".into());
    }
    match request.platform.as_str() {
        "dm" => dm::validate(&request.config),
        "bilibili" => bilibili::validate(&request.config),
        _ => Err("不支持的购票平台".into()),
    }
}

impl TaskManager {
    fn reserve(
        &self,
        request: &TaskRequest,
    ) -> Result<(TaskSnapshot, watch::Receiver<bool>), String> {
        let (cancel, receiver) = watch::channel(false);
        let snapshot = TaskSnapshot {
            id: request.id.clone(),
            platform: request.platform.clone(),
            title: request.title.clone(),
            status: "waiting".into(),
            message: "任务已就绪，等待开始".into(),
            attempt: 0,
            max_attempts: request.max_attempts,
            start_at: request.start_at,
            offset_ms: request.offset_ms,
            updated_at: clock::now_ms(),
            revision: 0,
            order_url: None,
        };
        {
            let mut tasks = self.tasks.lock().map_err(|_| "任务状态不可用")?;
            if tasks.contains_key(&request.id) {
                return Err("任务编号已存在".into());
            }
            if tasks
                .values()
                .any(|t| t.snapshot.platform == request.platform && t.snapshot.active())
            {
                return Err("该平台已有运行中的任务，请先停止".into());
            }
            if tasks.len() >= 100 {
                let oldest = tasks
                    .iter()
                    .filter(|(_, v)| !v.snapshot.active())
                    .min_by_key(|(_, v)| v.snapshot.updated_at)
                    .map(|(k, _)| k.clone());
                if let Some(id) = oldest {
                    tasks.remove(&id);
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

#[tauri::command]
pub fn start_ticket_task(
    app: AppHandle,
    state: State<'_, TaskManager>,
    request: TaskRequest,
) -> Result<TaskSnapshot, String> {
    validate(&request)?;
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
            context.report("running", "开始执行购票任务", 0, None);
            match context.request.platform.as_str() {
                "dm" => dm::run(&context).await,
                _ => bilibili::run(&context).await,
            }
        };
        let outcome = match run_cancellable(receiver, run).await {
            None => {
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
                    order_url: Some(
                        known_order
                            .unwrap_or_else(|| order_list_url(&context.request.platform).into()),
                    ),
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
        context.report(outcome.status, outcome.message, attempt, outcome.order_url);
        if let Ok(mut pending) = context.manager.credentials.lock() {
            pending.retain(|key, _| !key.starts_with(&format!("{}:", context.request.id)));
        };
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
}
