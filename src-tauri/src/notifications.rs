use crate::{
    clock,
    wechat_api::{self, ApiError, UpdatesResponse, WechatApi},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{watch, Mutex as AsyncMutex};
use uuid::Uuid;

const LOGIN_TTL: i64 = 5 * 60_000;
const INITIAL_BASE: &str = "https://ilinkai.weixin.qq.com";

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WechatConfig {
    pub enabled: bool,
    pub target: String,
    pub account_id: String,
}

impl WechatConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if !valid_id(&self.target)
            || !self.target.ends_with("@im.wechat")
            || self.target == "@im.wechat"
        {
            return Err("请在微信通知设置中扫码绑定接收人".into());
        }
        if !valid_id(&self.account_id) {
            return Err("请在微信通知设置中扫码绑定 Bot 账号".into());
        }
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Binding {
    id: String,
    account_id: String,
    target: String,
    token: String,
    base_url: String,
    #[serde(default)]
    context_token: String,
    #[serde(default)]
    cursor: String,
    #[serde(default)]
    expired: bool,
    updated_at: i64,
}

impl Binding {
    fn validate(&self) -> Result<(), String> {
        WechatConfig {
            enabled: true,
            target: self.target.clone(),
            account_id: self.account_id.clone(),
        }
        .validate()?;
        wechat_api::api_base(&self.base_url)?;
        if !valid_id(&self.id)
            || self.token.is_empty()
            || self.token.len() > 8192
            || self.token.chars().any(char::is_control)
            || self.context_token.len() > 16_384
            || self.cursor.len() > 1_048_576
        {
            return Err("微信绑定数据无效，请重新绑定".into());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct StoredBinding {
    version: u32,
    account: Binding,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WechatStatus {
    status: String,
    account_id: String,
    target: String,
    message: String,
    updated_at: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStatus {
    session_id: String,
    qr_content: String,
    expires_at: i64,
    status: String,
    message: String,
}

#[derive(Clone)]
struct Login {
    view: LoginStatus,
    qrcode: String,
    base: String,
    polling: bool,
    redirects: u8,
    verify_code: Option<String>,
}

struct Connection {
    account: Option<Binding>,
    login: Option<Login>,
    stop: Option<watch::Sender<bool>>,
    status: String,
    message: String,
    load_error: bool,
}

struct Inner {
    api: WechatApi,
    path: PathBuf,
    connection: Mutex<Connection>,
    send_lock: AsyncMutex<()>,
}

#[derive(Clone)]
pub struct WechatManager {
    inner: Arc<Inner>,
}

impl WechatManager {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        let (account, load_error) = match read_binding(&path) {
            Ok(value) => (value, false),
            Err(_) => (None, true),
        };
        let (status, message) = if load_error {
            (
                "error",
                "微信绑定数据读取失败，原文件已保留；请检查本机存储，或解除绑定后重新扫码",
            )
        } else if account.as_ref().is_some_and(|a| a.expired) {
            ("expired", "微信登录已失效，请重新扫码绑定")
        } else if account.is_some() {
            ("reconnecting", "正在恢复微信连接…")
        } else {
            ("unbound", "尚未绑定微信")
        };
        Ok(Self {
            inner: Arc::new(Inner {
                api: WechatApi::new()?,
                path,
                connection: Mutex::new(Connection {
                    account,
                    login: None,
                    stop: None,
                    status: status.into(),
                    message: message.into(),
                    load_error,
                }),
                send_lock: AsyncMutex::new(()),
            }),
        })
    }

    pub fn status(&self) -> Result<WechatStatus, String> {
        let state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
        Ok(WechatStatus {
            status: state.status.clone(),
            message: state.message.clone(),
            account_id: state
                .account
                .as_ref()
                .map(|a| a.account_id.clone())
                .unwrap_or_default(),
            target: state
                .account
                .as_ref()
                .map(|a| a.target.clone())
                .unwrap_or_default(),
            updated_at: state.account.as_ref().map(|a| a.updated_at).unwrap_or(0),
        })
    }

    pub fn check(&self, config: &WechatConfig) -> Result<(), String> {
        if !config.enabled {
            return Ok(());
        }
        self.bound_account(config).map(|_| ())
    }

    fn bound_account(&self, config: &WechatConfig) -> Result<Binding, String> {
        config.validate()?;
        let state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
        let account = state
            .account
            .as_ref()
            .ok_or("请先在设置中扫码绑定微信；旧 OpenClaw 设置需要重新绑定")?;
        if account.account_id != config.account_id || account.target != config.target {
            return Err(
                "微信绑定已变更，本任务仍使用原接收人；请检查通知设置，新任务将使用新的接收人"
                    .into(),
            );
        }
        if account.expired {
            return Err("微信登录已失效，请重新扫码绑定".into());
        }
        if account.context_token.is_empty() {
            return Err("请先在微信中向 Bot 发一条消息，建立通知会话".into());
        }
        if state.load_error {
            return Err("微信绑定数据无法读取，请检查本机存储".into());
        }
        Ok(account.clone())
    }

    pub fn resume(&self) {
        let Ok(mut state) = self.inner.connection.lock() else {
            return;
        };
        if state.stop.is_some() {
            return;
        }
        let Some(account) = state.account.as_ref().filter(|a| !a.expired) else {
            return;
        };
        let id = account.id.clone();
        let (stop, receiver) = watch::channel(false);
        state.stop = Some(stop);
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            manager.receive(id, receiver).await;
        });
    }

    async fn receive(&self, id: String, mut stop: watch::Receiver<bool>) {
        let mut failures = 0u32;
        loop {
            if *stop.borrow() {
                return;
            }
            let account = {
                let Ok(state) = self.inner.connection.lock() else {
                    return;
                };
                let Some(account) = state.account.as_ref().filter(|a| a.id == id && !a.expired)
                else {
                    return;
                };
                account.clone()
            };
            let result = tokio::select! {
                biased;
                _ = stop.changed() => return,
                result = self.inner.api.updates(&account.base_url, &account.token, &account.cursor) => result,
            };
            let delay = match result {
                Ok(updates) => match self.accept_updates(&id, updates) {
                    Ok(true) => {
                        failures = 0;
                        500
                    }
                    Ok(false) => return,
                    Err(_) => {
                        self.connection_message(
                            &id,
                            "error",
                            "微信会话无法保存，请检查本机存储空间和权限",
                        );
                        5000
                    }
                },
                Err(ApiError::Timeout) => {
                    failures = 0;
                    1000
                }
                Err(ApiError::Expired) => {
                    self.expire(&id);
                    return;
                }
                Err(_) => {
                    failures = failures.saturating_add(1);
                    self.connection_message(&id, "reconnecting", "微信连接暂时中断，正在自动重连");
                    (2000u64 << failures.min(5)).min(60_000)
                }
            };
            tokio::select! { biased; _ = stop.changed() => return, _ = tokio::time::sleep(Duration::from_millis(delay)) => {} }
        }
    }

    fn accept_updates(&self, id: &str, updates: UpdatesResponse) -> Result<bool, String> {
        let mut state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
        let Some(previous) = state.account.as_ref().filter(|a| a.id == id && !a.expired) else {
            return Ok(false);
        };
        let mut account = previous.clone();
        let mut changed = false;
        for msg in updates.msgs {
            if msg.message_type != Some(1)
                || msg.from_user_id.as_deref() != Some(account.target.as_str())
                || msg
                    .to_user_id
                    .as_deref()
                    .is_some_and(|target| !target.is_empty() && target != account.account_id)
            {
                continue;
            }
            if let Some(token) = msg.context_token.filter(|token| {
                !token.is_empty() && token.len() <= 16_384 && !token.chars().any(char::is_control)
            }) {
                account.context_token = token;
                account.updated_at = clock::now_ms();
                changed = true;
            }
        }
        if let Some(cursor) = updates
            .get_updates_buf
            .filter(|v| !v.is_empty() && v.len() <= 1_048_576)
        {
            if cursor != account.cursor {
                account.cursor = cursor;
                changed = true;
            }
        }
        if changed {
            write_binding(&self.inner.path, &account)?;
        }
        let ready = !account.context_token.is_empty();
        state.account = Some(account);
        state.status = if ready { "ready" } else { "waiting_message" }.into();
        state.message = if ready {
            "微信已连接，可以发送购票与余票提醒"
        } else {
            "绑定成功，请在微信中向 Bot 发一条消息以启用通知"
        }
        .into();
        Ok(true)
    }

    fn connection_message(&self, id: &str, status: &str, message: &str) {
        if let Ok(mut state) = self.inner.connection.lock() {
            if state
                .account
                .as_ref()
                .is_some_and(|a| a.id == id && !a.expired)
            {
                state.status = status.into();
                state.message = message.into();
            }
        }
    }

    fn expire(&self, id: &str) {
        if let Ok(mut state) = self.inner.connection.lock() {
            if let Some(account) = state.account.as_mut().filter(|a| a.id == id) {
                account.expired = true;
                account.context_token.clear();
                let stored = write_binding(&self.inner.path, account).is_ok();
                state.status = "expired".into();
                state.message = if stored {
                    "微信登录已失效，请重新扫码绑定"
                } else {
                    "微信登录已失效，且失效状态无法保存；请检查本机存储后重新绑定"
                }
                .into();
                if let Some(stop) = state.stop.take() {
                    let _ = stop.send(true);
                }
            }
        }
    }

    async fn start_login(&self) -> Result<LoginStatus, String> {
        let id = Uuid::new_v4().to_string();
        let tokens = {
            let mut state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
            if state.load_error {
                return Err(state.message.clone());
            }
            let tokens = state
                .account
                .as_ref()
                .filter(|a| !a.expired)
                .map(|a| vec![a.token.clone()])
                .unwrap_or_default();
            state.login = Some(Login {
                view: LoginStatus {
                    session_id: id.clone(),
                    qr_content: String::new(),
                    expires_at: clock::now_ms() + LOGIN_TTL,
                    status: "wait".into(),
                    message: "请使用微信扫一扫，并确认绑定".into(),
                },
                qrcode: String::new(),
                base: INITIAL_BASE.into(),
                polling: false,
                redirects: 0,
                verify_code: None,
            });
            tokens
        };
        let response = self.inner.api.qr(tokens).await;
        let mut state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
        let login = state
            .login
            .as_mut()
            .filter(|l| l.view.session_id == id)
            .ok_or("本次扫码已取消，请重新获取二维码")?;
        match response {
            Ok(qr)
                if !qr.qrcode.is_empty()
                    && qr.qrcode.len() <= 8192
                    && !qr.qrcode_img_content.is_empty()
                    && qr.qrcode_img_content.len() <= 16_384 =>
            {
                login.qrcode = qr.qrcode;
                login.view.qr_content = qr.qrcode_img_content;
                Ok(login.view.clone())
            }
            Ok(_) => {
                state.login = None;
                Err("微信未返回有效的登录二维码，请重试".into())
            }
            Err(error) => {
                state.login = None;
                Err(error.to_string())
            }
        }
    }

    fn cancel_login(&self, id: &str) -> Result<(), String> {
        let mut state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
        if state
            .login
            .as_ref()
            .is_some_and(|l| l.view.session_id == id)
        {
            state.login = None;
        }
        Ok(())
    }

    async fn poll_login(&self, id: &str, verify_code: Option<&str>) -> Result<LoginStatus, String> {
        if verify_code
            .is_some_and(|v| v.is_empty() || v.len() > 64 || !v.bytes().all(|c| c.is_ascii_digit()))
        {
            return Err("请填写微信显示的数字验证码".into());
        }
        let login = {
            let mut state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
            let login = state
                .login
                .as_mut()
                .filter(|l| l.view.session_id == id)
                .ok_or("二维码已取消，请重新获取")?;
            if clock::now_ms() >= login.view.expires_at && login.view.status != "confirmed" {
                login.view.status = "expired".into();
                login.view.message = "二维码已过期，请重新获取".into();
            }
            if matches!(
                login.view.status.as_str(),
                "confirmed" | "expired" | "verify_code_blocked"
            ) || (login.view.status == "need_verifycode" && verify_code.is_none())
            {
                return Ok(login.view.clone());
            }
            if login.polling {
                return Err("正在等待扫码结果，请稍后".into());
            }
            if let Some(code) = verify_code {
                login.verify_code = Some(code.into());
                login.view.status = "scaned".into();
                login.view.message = "正在核对验证码…".into();
            }
            login.polling = true;
            login.clone()
        };
        let response = self
            .inner
            .api
            .poll_qr(&login.base, &login.qrcode, login.verify_code.as_deref())
            .await;
        let _sending = self.inner.send_lock.lock().await;
        let confirmed = {
            let mut state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
            let already_bound = state.account.as_ref().is_some_and(|a| !a.expired);
            let current = state
                .login
                .as_mut()
                .filter(|l| l.view.session_id == id)
                .ok_or("本次扫码已取消")?;
            current.polling = false;
            if clock::now_ms() >= current.view.expires_at {
                current.view.status = "expired".into();
                current.view.message = "二维码已过期，请重新获取".into();
                return Ok(current.view.clone());
            }
            let response = match response {
                Ok(response) => response,
                Err(ApiError::Timeout) => return Ok(current.view.clone()),
                Err(error) => return Err(error.to_string()),
            };
            match response.status.as_str() {
                "confirmed" => {
                    let account = Binding {
                        id: Uuid::new_v4().to_string(),
                        account_id: response
                            .ilink_bot_id
                            .ok_or("微信未返回 Bot 账号，请重新扫码")?,
                        target: response
                            .ilink_user_id
                            .ok_or("微信未返回扫码接收人，请重新扫码")?,
                        token: response.bot_token.ok_or("微信未返回登录凭证，请重新扫码")?,
                        base_url: wechat_api::api_base(
                            response.baseurl.as_deref().unwrap_or(INITIAL_BASE),
                        )?,
                        context_token: String::new(),
                        cursor: String::new(),
                        expired: false,
                        updated_at: clock::now_ms(),
                    };
                    account.validate()?;
                    write_binding(&self.inner.path, &account)?;
                    current.view.status = "confirmed".into();
                    current.view.qr_content.clear();
                    current.verify_code = None;
                    current.view.message = "绑定成功，请在微信中向 Bot 发一条消息".into();
                    let view = current.view.clone();
                    if let Some(stop) = state.stop.take() {
                        let _ = stop.send(true);
                    }
                    state.account = Some(account);
                    state.status = "waiting_message".into();
                    state.message = view.message.clone();
                    view
                }
                "scaned_but_redirect" => {
                    if current.redirects >= 3 {
                        return Err("微信登录跳转次数过多，请重新扫码".into());
                    }
                    let host = response
                        .redirect_host
                        .ok_or("微信登录跳转信息缺失，请重新扫码")?;
                    let value = if host.starts_with("https://") {
                        host
                    } else {
                        format!("https://{host}")
                    };
                    current.base = wechat_api::api_base(&value)?;
                    current.redirects += 1;
                    current.view.status = "scaned".into();
                    current.view.message = "已扫码，正在完成绑定…".into();
                    return Ok(current.view.clone());
                }
                "binded_redirect" => {
                    current.view.status = if already_bound {
                        "confirmed"
                    } else {
                        "expired"
                    }
                    .into();
                    current.view.qr_content.clear();
                    current.view.message = if already_bound {
                        "此微信已绑定本应用，可以继续使用"
                    } else {
                        "微信提示已有绑定，但本机凭证不可用，请在微信中移除旧绑定后重新扫码"
                    }
                    .into();
                    return Ok(current.view.clone());
                }
                "wait" | "scaned" | "expired" | "need_verifycode" | "verify_code_blocked" => {
                    current.view.message = match response.status.as_str() {
                        "scaned" => "已扫码，请在微信中确认授权",
                        "expired" => "二维码已过期，请重新获取",
                        "need_verifycode" if current.verify_code.is_some() => {
                            "验证码不匹配，请重新填写微信显示的数字"
                        }
                        "need_verifycode" => "请填写微信显示的验证码以完成绑定",
                        "verify_code_blocked" => "验证码尝试次数过多，请重新获取二维码",
                        _ => "请使用微信扫一扫，并确认绑定",
                    }
                    .into();
                    if response.status != "wait" {
                        current.verify_code = None;
                    }
                    current.view.status = response.status;
                    return Ok(current.view.clone());
                }
                _ => return Err("微信返回了暂不支持的登录状态，请重新扫码或更新应用".into()),
            }
        };
        self.resume();
        Ok(confirmed)
    }

    async fn disconnect(&self) -> Result<WechatStatus, String> {
        let _sending = self.inner.send_lock.lock().await;
        {
            let mut state = self.inner.connection.lock().map_err(|_| "微信状态不可用")?;
            match fs::remove_file(&self.inner.path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("微信绑定无法删除，解绑尚未生效，请检查本机存储权限".into()),
            }
            if let Some(stop) = state.stop.take() {
                let _ = stop.send(true);
            }
            state.account = None;
            state.login = None;
            state.load_error = false;
            state.status = "unbound".into();
            state.message =
                "已解除此 App 的微信绑定；如需撤销授权，可在微信 ClawBot 中移除该绑定".into();
        }
        self.status()
    }

    async fn send_message(&self, config: &WechatConfig, text: &str) -> Result<(), String> {
        if !config.enabled {
            return Err("尚未启用微信通知".into());
        }
        let _sending = self.inner.send_lock.lock().await;
        let account = self.bound_account(config)?;
        match self
            .inner
            .api
            .send(
                &account.base_url,
                &account.token,
                &account.target,
                &account.context_token,
                text,
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(ApiError::Expired) => {
                self.expire(&account.id);
                Err("微信登录已失效，请重新扫码绑定".into())
            }
            Err(ApiError::Timeout) => {
                Err("微信发送超时，送达状态未知，请检查微信；应用不会自动重发".into())
            }
            Err(error) => Err(format!(
                "{error}；请在微信中向 Bot 发一条消息更新会话后测试通知"
            )),
        }
    }
}

fn read_binding(path: &Path) -> Result<Option<Binding>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("微信绑定数据读取失败".into()),
    };
    let mut bytes = Vec::new();
    file.take(2_097_153)
        .read_to_end(&mut bytes)
        .map_err(|_| "微信绑定数据读取失败")?;
    if bytes.len() > 2_097_152 {
        return Err("微信绑定数据过大".into());
    }
    let stored: StoredBinding = serde_json::from_slice(&bytes).map_err(|_| "微信绑定数据损坏")?;
    if stored.version != 1 {
        return Err("暂不支持此版本的微信绑定数据".into());
    }
    stored.account.validate()?;
    Ok(Some(stored.account))
}

fn write_binding(path: &Path, account: &Binding) -> Result<(), String> {
    account.validate()?;
    let parent = path.parent().ok_or("微信存储目录不可用")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建微信凭证目录")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| "无法设置微信凭证目录权限")?;
    }
    let temp = parent.join(format!(".binding-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|_| "无法创建微信凭证文件")?;
        let bytes = serde_json::to_vec(&StoredBinding {
            version: 1,
            account: account.clone(),
        })
        .map_err(|_| "微信凭证无法保存")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "微信凭证写入失败，请检查磁盘空间")?;
        drop(file);
        fs::rename(&temp, path).map_err(|_| "微信凭证保存失败，原绑定已保留")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub async fn send(app: &AppHandle, config: &WechatConfig, text: &str) -> Result<(), String> {
    app.state::<WechatManager>()
        .send_message(config, text)
        .await
}

#[tauri::command]
pub fn get_wechat_status(state: State<'_, WechatManager>) -> Result<WechatStatus, String> {
    state.status()
}

#[tauri::command]
pub async fn start_wechat_login(state: State<'_, WechatManager>) -> Result<LoginStatus, String> {
    state.start_login().await
}

#[tauri::command]
pub async fn poll_wechat_login(
    state: State<'_, WechatManager>,
    session_id: String,
    verify_code: Option<String>,
) -> Result<LoginStatus, String> {
    state.poll_login(&session_id, verify_code.as_deref()).await
}

#[tauri::command]
pub fn cancel_wechat_login(
    state: State<'_, WechatManager>,
    session_id: String,
) -> Result<(), String> {
    state.cancel_login(&session_id)
}

#[tauri::command]
pub async fn disconnect_wechat(state: State<'_, WechatManager>) -> Result<WechatStatus, String> {
    state.disconnect().await
}

#[tauri::command]
pub async fn test_wechat_notification(
    state: State<'_, WechatManager>,
    config: WechatConfig,
) -> Result<(), String> {
    state
        .send_message(
            &config,
            "Tickets 测试通知：微信购票与余票提醒通道已连接。自动提醒是否启用请以应用设置为准。",
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wechat_api::InboundMessage;
    use serde_json::{json, Value};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Fixture {
        directory: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                directory: std::env::temp_dir().join(format!("tickets-wechat-{}", Uuid::new_v4())),
            }
        }
        fn path(&self) -> PathBuf {
            self.directory.join("wechat").join("binding.json")
        }
        fn manager(&self, account: Option<&Binding>) -> WechatManager {
            if let Some(account) = account {
                write_binding(&self.path(), account).unwrap();
            }
            WechatManager::new(self.path()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
    fn binding() -> Binding {
        Binding {
            id: "binding-1".into(),
            account_id: "bot-1".into(),
            target: "owner@im.wechat".into(),
            token: "SYNTHETIC_BOT_SECRET".into(),
            base_url: INITIAL_BASE.into(),
            context_token: String::new(),
            cursor: String::new(),
            expired: false,
            updated_at: 1,
        }
    }
    fn config() -> WechatConfig {
        WechatConfig {
            enabled: true,
            account_id: "bot-1".into(),
            target: "owner@im.wechat".into(),
        }
    }
    fn message(sender: &str, bot: &str, token: &str) -> InboundMessage {
        InboundMessage {
            from_user_id: Some(sender.into()),
            to_user_id: Some(bot.into()),
            message_type: Some(1),
            context_token: Some(token.into()),
        }
    }
    fn login(id: &str) -> Login {
        Login {
            view: LoginStatus {
                session_id: id.into(),
                qr_content: "mock-qr-content".into(),
                expires_at: clock::now_ms() + LOGIN_TTL,
                status: "wait".into(),
                message: String::new(),
            },
            qrcode: "mock-qr".into(),
            base: INITIAL_BASE.into(),
            polling: false,
            redirects: 0,
            verify_code: None,
        }
    }

    #[test]
    fn only_scanning_users_context_is_saved_and_public_status_has_no_credentials() {
        let fixture = Fixture::new();
        let manager = fixture.manager(Some(&binding()));
        assert!(manager.check(&config()).unwrap_err().contains("发一条消息"));
        manager
            .accept_updates(
                "binding-1",
                UpdatesResponse {
                    msgs: vec![
                        message("stranger@im.wechat", "bot-1", "wrong-user"),
                        message("owner@im.wechat", "bot-2", "wrong-bot"),
                    ],
                    get_updates_buf: Some("cursor-1".into()),
                },
            )
            .unwrap();
        assert_eq!(manager.status().unwrap().status, "waiting_message");
        manager
            .accept_updates(
                "binding-1",
                UpdatesResponse {
                    msgs: vec![message(
                        "owner@im.wechat",
                        "bot-1",
                        "SYNTHETIC_CONTEXT_SECRET",
                    )],
                    get_updates_buf: Some("cursor-2".into()),
                },
            )
            .unwrap();
        assert_eq!(manager.status().unwrap().status, "ready");
        assert!(manager.check(&config()).is_ok());
        let restored = read_binding(&fixture.path()).unwrap().unwrap();
        assert_eq!(restored.context_token, "SYNTHETIC_CONTEXT_SECRET");
        assert_eq!(restored.cursor, "cursor-2");
        let view = serde_json::to_string(&manager.status().unwrap()).unwrap();
        assert!(!view.contains("SECRET"));
        assert!(!view.contains("cursor"));
        assert!(!view.contains("baseUrl"));
        assert!(fixture.manager(None).check(&config()).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(fixture.path()).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(fixture.path().parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn stale_receivers_and_wrong_task_targets_cannot_change_binding() {
        let fixture = Fixture::new();
        let mut account = binding();
        account.context_token = "context".into();
        let manager = fixture.manager(Some(&account));
        assert!(!manager
            .accept_updates(
                "old-binding",
                UpdatesResponse {
                    msgs: vec![message("owner@im.wechat", "bot-1", "old")],
                    get_updates_buf: Some("old".into())
                }
            )
            .unwrap());
        manager.expire("old-binding");
        assert_eq!(
            read_binding(&fixture.path())
                .unwrap()
                .unwrap()
                .context_token,
            "context"
        );
        let mut wrong = config();
        wrong.target = "other@im.wechat".into();
        assert!(manager.check(&wrong).unwrap_err().contains("绑定已变更"));
        manager.expire("binding-1");
        assert!(manager.check(&config()).unwrap_err().contains("失效"));
        assert_eq!(fixture.manager(None).status().unwrap().status, "expired");
    }

    #[test]
    fn failed_storage_write_keeps_previous_cursor_and_context() {
        let fixture = Fixture::new();
        let manager = fixture.manager(Some(&binding()));
        fs::remove_file(fixture.path()).unwrap();
        fs::create_dir(fixture.path()).unwrap();
        assert!(manager
            .accept_updates(
                "binding-1",
                UpdatesResponse {
                    msgs: vec![message("owner@im.wechat", "bot-1", "unsaved")],
                    get_updates_buf: Some("unsaved".into())
                }
            )
            .is_err());
        let state = manager.inner.connection.lock().unwrap();
        assert!(state.account.as_ref().unwrap().context_token.is_empty());
        assert!(state.account.as_ref().unwrap().cursor.is_empty());
        assert_eq!(
            fs::read_dir(fixture.path().parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn corrupted_storage_is_preserved_until_explicit_disconnect() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.path().parent().unwrap()).unwrap();
        fs::write(fixture.path(), b"broken original data").unwrap();
        let manager = fixture.manager(None);
        assert_eq!(manager.status().unwrap().status, "error");
        assert!(manager.start_login().await.is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), b"broken original data");
        assert_eq!(manager.disconnect().await.unwrap().status, "unbound");
        assert!(!fixture.path().exists());
    }

    #[tokio::test]
    async fn disconnect_failure_preserves_binding_and_success_blocks_old_tasks() {
        let fixture = Fixture::new();
        let mut account = binding();
        account.context_token = "context".into();
        let manager = fixture.manager(Some(&account));
        fs::remove_file(fixture.path()).unwrap();
        fs::create_dir(fixture.path()).unwrap();
        assert!(manager.disconnect().await.is_err());
        assert!(manager.check(&config()).is_ok());
        fs::remove_dir(fixture.path()).unwrap();
        assert_eq!(manager.disconnect().await.unwrap().status, "unbound");
        assert!(manager.check(&config()).is_err());
        assert!(manager.check(&WechatConfig::default()).is_ok());
    }

    #[tokio::test]
    async fn cancelled_and_expired_qr_sessions_do_not_poll() {
        let fixture = Fixture::new();
        let manager = fixture.manager(None);
        manager.inner.connection.lock().unwrap().login = Some(login("current"));
        manager.cancel_login("old").unwrap();
        assert!(manager.inner.connection.lock().unwrap().login.is_some());
        manager
            .inner
            .connection
            .lock()
            .unwrap()
            .login
            .as_mut()
            .unwrap()
            .view
            .expires_at = 1;
        assert_eq!(
            manager.poll_login("current", None).await.unwrap().status,
            "expired"
        );
        manager.cancel_login("current").unwrap();
        assert!(manager.poll_login("current", None).await.is_err());
    }

    async fn mock_server() -> (
        WechatApi,
        Arc<Mutex<Vec<(String, Value)>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = WechatApi::with_test_base(&format!("http://{}", listener.local_addr().unwrap()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let handle = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 4096];
                let end = loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let length = headers
                    .lines()
                    .filter_map(|s| s.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    .unwrap_or(0);
                while bytes.len() < end + length {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_string();
                let payload = if length > 0 {
                    serde_json::from_slice(&bytes[end..end + length]).unwrap()
                } else {
                    Value::Null
                };
                recorded.lock().unwrap().push((path.clone(), payload));
                let response = if path.contains("get_bot_qrcode") { json!({"qrcode":"mock-qr", "qrcode_img_content":"https://weixin.qq.com/mock-login"}) }
                else if path.contains("get_qrcode_status") {
                    if path.contains("verify_code=123456") { json!({"status":"confirmed","bot_token":"SYNTHETIC_BOT_SECRET","ilink_bot_id":"bot-1","ilink_user_id":"owner@im.wechat","baseurl":INITIAL_BASE}) }
                    else { json!({"status":"need_verifycode"}) }
                } else if path.contains("getupdates") { json!({"ret":0,"msgs":[{"from_user_id":"owner@im.wechat","to_user_id":"bot-1","message_type":1,"context_token":"context-from-inbound"}],"get_updates_buf":"cursor-inbound"}) }
                else { json!({"ret":0}) }.to_string();
                let _ = socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await;
            }
        });
        (api, requests, handle)
    }

    #[tokio::test]
    async fn late_login_confirmation_cannot_replace_a_new_qr_session() {
        let fixture = Fixture::new();
        let mut manager = fixture.manager(None);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        Arc::get_mut(&mut manager.inner).unwrap().api =
            WechatApi::with_test_base(&format!("http://{}", listener.local_addr().unwrap()));
        manager.inner.connection.lock().unwrap().login = Some(login("old-session"));
        let background = manager.clone();
        let pending = tokio::spawn(async move { background.poll_login("old-session", None).await });
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        socket.read(&mut request).await.unwrap();
        manager.cancel_login("old-session").unwrap();
        manager.inner.connection.lock().unwrap().login = Some(login("new-session"));
        let body = json!({"status":"confirmed","bot_token":"SYNTHETIC_BOT_SECRET","ilink_bot_id":"bot-1","ilink_user_id":"owner@im.wechat","baseurl":INITIAL_BASE}).to_string();
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
        assert!(pending.await.unwrap().is_err());
        assert_eq!(manager.status().unwrap().status, "unbound");
        assert_eq!(
            manager
                .inner
                .connection
                .lock()
                .unwrap()
                .login
                .as_ref()
                .unwrap()
                .view
                .session_id,
            "new-session"
        );
        assert!(!fixture.path().exists());
    }

    #[tokio::test]
    async fn already_bound_response_preserves_local_credentials() {
        let fixture = Fixture::new();
        let mut account = binding();
        account.context_token = "retained-context".into();
        let mut manager = fixture.manager(Some(&account));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        Arc::get_mut(&mut manager.inner).unwrap().api =
            WechatApi::with_test_base(&format!("http://{}", listener.local_addr().unwrap()));
        manager.inner.connection.lock().unwrap().login = Some(login("same-account"));
        let response = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            socket.read(&mut request).await.unwrap();
            let body = json!({"status":"binded_redirect"}).to_string();
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
        assert_eq!(
            manager
                .poll_login("same-account", None)
                .await
                .unwrap()
                .status,
            "confirmed"
        );
        assert_eq!(
            manager.bound_account(&config()).unwrap().context_token,
            "retained-context"
        );
        assert_eq!(
            read_binding(&fixture.path()).unwrap().unwrap().id,
            "binding-1"
        );
        response.await.unwrap();
    }

    #[tokio::test]
    async fn qr_verification_inbound_context_send_and_disconnect_work_together() {
        let fixture = Fixture::new();
        let mut manager = fixture.manager(None);
        let (api, requests, server) = mock_server().await;
        Arc::get_mut(&mut manager.inner).unwrap().api = api;
        let qr = manager.start_login().await.unwrap();
        assert!(!qr.qr_content.is_empty());
        let verify = manager.poll_login(&qr.session_id, None).await.unwrap();
        assert_eq!(verify.status, "need_verifycode");
        let count = requests.lock().unwrap().len();
        assert_eq!(
            manager
                .poll_login(&qr.session_id, None)
                .await
                .unwrap()
                .status,
            "need_verifycode"
        );
        assert_eq!(requests.lock().unwrap().len(), count);
        assert_eq!(
            manager
                .poll_login(&qr.session_id, Some("123456"))
                .await
                .unwrap()
                .status,
            "confirmed"
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            while manager.status().unwrap().status != "ready" {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        manager
            .send_message(&config(), "mock ticket found")
            .await
            .unwrap();
        let sent: Vec<_> = requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(path, _)| path.contains("sendmessage"))
            .cloned()
            .collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1["msg"]["to_user_id"], "owner@im.wechat");
        assert_eq!(sent[0].1["msg"]["context_token"], "context-from-inbound");
        assert!(fixture.manager(None).check(&config()).is_ok());
        manager.disconnect().await.unwrap();
        assert!(manager
            .send_message(&config(), "must not send")
            .await
            .is_err());
        assert!(!fixture.path().exists());
        server.abort();
    }
}
