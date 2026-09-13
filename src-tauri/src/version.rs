use std::time::Duration;

#[tauri::command]
pub async fn get_repo_release() -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "无法初始化更新连接")?;
    let release = crate::http::json(
        client
            .get("https://api.github.com/repos/shiyutim/tickets/releases/latest")
            .header(reqwest::header::ACCEPT, "application/vnd.github+json"),
    )
    .await
    .map_err(|error| format!("检查更新失败：{error}"))?;
    if release["tag_name"].as_str().unwrap_or_default().is_empty() {
        return Err("发布信息缺少版本号".into());
    }
    Ok(release)
}
