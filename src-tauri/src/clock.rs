use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSample {
    pub offset_ms: i64,
    pub round_trip_ms: i64,
    pub uncertainty_ms: i64,
    pub sampled_at: i64,
    pub source: String,
}

pub fn sample_offset(server_ms: i64, sent_ms: i64, round_trip_ms: i64) -> i64 {
    server_ms
        .saturating_sub(sent_ms)
        .saturating_sub(round_trip_ms / 2)
}

pub fn delay_ms(target_ms: i64, offset_ms: i64, local_ms: i64) -> u64 {
    target_ms
        .saturating_sub(offset_ms)
        .saturating_sub(local_ms)
        .max(0) as u64
}

#[tauri::command]
pub async fn sync_clock() -> Result<ClockSample, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| "无法初始化校时连接")?;
    for (url, pointer, scale, resolution, source) in [
        (
            "https://api.m.taobao.com/rest/api3.do?api=mtop.common.getTimestamp",
            "/data/t",
            1,
            1,
            "淘宝公共时间",
        ),
        (
            "https://api.bilibili.com/x/report/click/now",
            "/data/now",
            1000,
            1000,
            "Bilibili 公共时间",
        ),
    ] {
        let mut samples = Vec::new();
        for _ in 0..3 {
            let sent = now_ms();
            let timer = Instant::now();
            let response = client
                .get(url)
                .query(&[("_", sent)])
                .header("Cache-Control", "no-cache, no-store")
                .send()
                .await;
            if response.is_err() && samples.is_empty() {
                break;
            }
            if let Ok(response) = response {
                if !response.status().is_success() {
                    break;
                }
                if let Ok(value) = response.json::<Value>().await {
                    let rtt = timer.elapsed().as_millis() as i64;
                    let raw = value
                        .pointer(pointer)
                        .map(crate::http::number)
                        .unwrap_or_default();
                    let server = raw.saturating_mul(scale);
                    let offset = sample_offset(server, sent, rtt);
                    if server > 1_600_000_000_000 && (-86_400_000..=86_400_000).contains(&offset) {
                        samples.push(ClockSample {
                            offset_ms: offset,
                            round_trip_ms: rtt,
                            uncertainty_ms: rtt / 2 + resolution,
                            sampled_at: now_ms(),
                            source: source.into(),
                        });
                    }
                }
            }
        }
        if let Some(sample) = samples.into_iter().min_by_key(|s| s.round_trip_ms) {
            return Ok(sample);
        }
    }
    Err("公共时间接口暂不可用，已保留原修正值；可以稍后重试或手动填写".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compensates_round_trip_and_offset_direction() {
        assert_eq!(sample_offset(10_200, 10_000, 100), 150);
        assert_eq!(delay_ms(11_000, 150, 10_000), 850);
        assert_eq!(delay_ms(11_000, -150, 10_000), 1150);
        assert_eq!(delay_ms(9_000, 150, 10_000), 0);
    }
}
