use crate::{
    bilibili, clock, dm,
    http::{self, first, string, Account},
    notifications::WechatConfig,
    tasks::{Outcome, TaskContext},
};
use serde::Deserialize;
use serde_json::Value;
use std::{future::Future, time::Duration};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    account: Account,
    project_id: String,
    screen_id: String,
    sku_id: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    pub end_at: i64,
    #[serde(default)]
    wechat: WechatConfig,
}

pub fn validate(value: &Value, platform: &str) -> Result<(), String> {
    let config: Config = serde_json::from_value(value.clone()).map_err(|_| "余票监控配置不完整")?;
    if !matches!(platform, "dm" | "bilibili") {
        return Err("不支持的监控平台".into());
    }
    for id in [&config.project_id, &config.screen_id, &config.sku_id] {
        if !id
            .parse::<u64>()
            .is_ok_and(|id| id > 0 && id <= 9_007_199_254_740_991)
        {
            return Err("请选择有效的活动、场次和票档".into());
        }
    }
    if config.end_at < 0 || config.date.len() > 32 {
        return Err("监控结束时间或活动日期无效".into());
    }
    http::client(
        &config.account,
        if platform == "dm" {
            "https://m.damai.cn"
        } else {
            "https://show.bilibili.com"
        },
    )?;
    if platform == "dm" && http::cookie_value(&config.account.cookie, "_m_h5_tk").is_empty() {
        return Err("大麦 Cookie 缺少 _m_h5_tk，请重新获取".into());
    }
    config.wechat.validate()
}

#[derive(Debug, PartialEq)]
enum Availability {
    Available,
    Unavailable,
    Unknown,
}

fn boolean(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(v) => Some(*v),
        _ => match string(value).trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
    }
}

fn quantity(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| *n >= 0)
}

fn blocked_text(value: &str) -> bool {
    [
        "售罄",
        "售完",
        "缺货",
        "无票",
        "无货",
        "停售",
        "不可售",
        "不可购",
        "未开售",
        "未开始",
        "即将开售",
        "已结束",
        "已取消",
        "下架",
        "登记",
        "候补",
    ]
    .iter()
    .any(|word| value.contains(word))
}

fn future_sale(item: &Value, now: i64) -> bool {
    let value = quantity(first(item, &["sale_start", "saleStart"])).unwrap_or(0);
    let milliseconds = if value < 100_000_000_000 {
        value.saturating_mul(1000)
    } else {
        value
    };
    milliseconds > now
}

fn bili_state(item: &Value, now: i64) -> Availability {
    let flag = first(item, &["sale_flag", "saleFlag"]);
    let nested = first(flag, &["number", "sale_flag_number", "saleFlagNumber"]);
    let number = quantity(if nested.is_null() {
        first(item, &["sale_flag_number", "saleFlagNumber"])
    } else {
        nested
    });
    let label = if flag.is_string() {
        string(flag)
    } else {
        string(first(flag, &["display_name", "displayName"]))
    };
    let clickable = boolean(first(item, &["clickable", "canClick"]));
    let stock = quantity(first(item, &["num", "stock", "stock_num", "stockNum"]));
    if clickable == Some(false)
        || stock == Some(0)
        || future_sale(item, now)
        || blocked_text(&label)
        || matches!(
            number,
            Some(1 | 3 | 4 | 5 | 7 | 8 | 9 | 101 | 102 | 103 | 105 | 106)
        )
    {
        return Availability::Unavailable;
    }
    if stock.is_some_and(|n| n > 0)
        || matches!(number, Some(2 | 6))
        || ["立即购买", "预售中", "售票中"].contains(&label.as_str())
    {
        Availability::Available
    } else {
        Availability::Unknown
    }
}

fn bili_availability(
    raw: &Value,
    screen_id: &str,
    sku_id: &str,
    local_now: i64,
    offset_ms: i64,
) -> Result<Availability, String> {
    let now = local_now.saturating_add(offset_ms);
    let screens = first(raw, &["screen_list", "screenList"])
        .as_array()
        .ok_or("未返回有效场次列表")?;
    let Some(screen) = screens
        .iter()
        .find(|item| string(first(item, &["id", "screen_id", "screenId"])) == screen_id)
    else {
        return Ok(Availability::Unknown);
    };
    if bili_state(screen, now) == Availability::Unavailable {
        return Ok(Availability::Unavailable);
    }
    let tickets = first(screen, &["ticket_list", "ticketList"])
        .as_array()
        .ok_or("未返回有效票档列表")?;
    Ok(tickets
        .iter()
        .find(|item| string(first(item, &["id", "sku_id", "skuId"])) == sku_id)
        .map(|item| bili_state(item, now))
        .unwrap_or(Availability::Unknown))
}

fn damai_availability(raw: &Value, sku_id: &str) -> Result<Availability, String> {
    let tickets = raw["perform"]["skuList"]
        .as_array()
        .ok_or("未返回有效票档列表")?;
    let Some(ticket) = tickets.iter().find(|item| string(&item["skuId"]) == sku_id) else {
        return Ok(Availability::Unknown);
    };
    let tags = ticket["tags"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| string(&item["tagDesc"]))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let label = string(first(ticket, &["buyBtnText", "statusText"]));
    let stock = quantity(first(
        ticket,
        &["salableQuantity", "quantity", "skuQuantity"],
    ));
    if stock == Some(0)
        || boolean(&ticket["enable"]) == Some(false)
        || blocked_text(&tags)
        || blocked_text(&label)
        || boolean(&raw["perform"]["enable"]) == Some(false)
    {
        return Ok(Availability::Unavailable);
    }
    Ok(if stock.is_some_and(|n| n > 0) {
        Availability::Available
    } else {
        Availability::Unknown
    })
}

async fn query(config: &Config, platform: &str, offset_ms: i64) -> Result<Availability, String> {
    if platform == "dm" {
        let raw = dm::dm_tickets(
            config.account.clone(),
            config.project_id.clone(),
            config.screen_id.clone(),
        )
        .await?;
        damai_availability(&raw, &config.sku_id)
    } else {
        let id = config
            .project_id
            .parse::<i64>()
            .map_err(|_| "项目编号无效")?;
        let raw = if config.date.is_empty() {
            bilibili::bili_project(config.account.clone(), id).await?
        } else {
            bilibili::bili_screens(config.account.clone(), id, config.date.clone()).await?
        };
        bili_availability(
            &raw,
            &config.screen_id,
            &config.sku_id,
            clock::now_ms(),
            offset_ms,
        )
    }
}

async fn poll<Q, F, R>(
    interval: u64,
    max_attempts: u32,
    mut query: Q,
    mut report: R,
) -> Result<bool, String>
where
    Q: FnMut() -> F,
    F: Future<Output = Result<Availability, String>>,
    R: FnMut(u32, &str),
{
    let mut attempt: u32 = 0;
    let mut errors: u32 = 0;
    loop {
        attempt = attempt.saturating_add(1);
        let result = query().await;
        let message = match result {
            Ok(Availability::Available) => {
                report(attempt, "发现可购票档，正在处理通知");
                return Ok(true);
            }
            Ok(state) => {
                errors = 0;
                if state == Availability::Unavailable {
                    "暂无可购余票，继续监控"
                } else {
                    "票档状态未知，等待下一次查询"
                }
                .to_string()
            }
            Err(_) => {
                errors += 1;
                if errors >= 5 {
                    report(attempt, "连续 5 次查询失败，监控停止");
                    return Err(
                        "连续 5 次查询失败，监控已停止；请检查网络、Cookie 和官方页面是否需要验证"
                            .into(),
                    );
                }
                format!("查询失败（连续 {errors}/5 次），稍后重试；请检查网络和登录状态")
            }
        };
        report(attempt, &message);
        if max_attempts > 0 && attempt >= max_attempts {
            return Ok(false);
        }
        let delay = if errors > 0 {
            interval
                .saturating_mul(1 << errors)
                .min(300_000)
                .max(interval)
        } else {
            interval
        };
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

pub fn project_url(platform: &str, project_id: &str) -> String {
    if platform == "dm" {
        format!("https://m.damai.cn/damai/detail/item.html?itemId={project_id}")
    } else {
        format!("https://show.bilibili.com/platform/detail.html?id={project_id}")
    }
}

pub async fn run(context: &TaskContext) -> Result<Outcome, String> {
    let config: Config =
        serde_json::from_value(context.request.config.clone()).map_err(|_| "监控配置不完整")?;
    let polling = poll(
        context.request.interval_ms,
        context.request.max_attempts,
        || {
            query(
                &config,
                &context.request.platform,
                context.request.offset_ms,
            )
        },
        |attempt, message| context.report("running", message, attempt, None),
    );
    let found = if config.end_at > 0 {
        let delay = clock::delay_ms(config.end_at, context.request.offset_ms, clock::now_ms());
        if delay == 0 {
            false
        } else {
            tokio::time::timeout(Duration::from_millis(delay), polling)
                .await
                .unwrap_or(Ok(false))?
        }
    } else {
        polling.await?
    };
    if !found {
        return Ok(Outcome {
            status: "completed",
            message: "监控已结束，未发现可购余票".into(),
            order_url: None,
        });
    }
    let url = project_url(&context.request.platform, &config.project_id);
    Ok(Outcome {
        status: "found",
        message: "发现可购票档，监控已结束，请前往官方页面确认".into(),
        order_url: Some(url),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bili_requires_positive_evidence_and_honors_unavailable_states() {
        for ticket in [json!({}), json!({"clickable":true}), json!({"num":-1})] {
            assert_eq!(bili_state(&ticket, 1), Availability::Unknown);
        }
        for ticket in [
            json!({"num":2}),
            json!({"saleFlagNumber":6}),
            json!({"sale_flag":{"number":2}}),
        ] {
            assert_eq!(bili_state(&ticket, 1), Availability::Available);
        }
        for ticket in [
            json!({"num":0,"saleFlagNumber":2}),
            json!({"num":2,"clickable":false}),
            json!({"num":2,"saleFlagNumber":4}),
            json!({"num":2,"sale_start":999}),
        ] {
            assert_eq!(bili_state(&ticket, 1), Availability::Unavailable);
        }
    }

    #[test]
    fn bili_matches_both_ids_and_both_api_formats() {
        for raw in [
            json!({"screenList":[{"screenId":1,"ticketList":[{"skuId":2,"num":"3"}]}]}),
            json!({"screen_list":[{"id":1,"ticket_list":[{"id":2,"num":"3"}]}]}),
        ] {
            assert_eq!(
                bili_availability(&raw, "1", "2", 1, 0).unwrap(),
                Availability::Available
            );
            assert_eq!(
                bili_availability(&raw, "1", "3", 1, 0).unwrap(),
                Availability::Unknown
            );
            assert_eq!(
                bili_availability(&raw, "3", "2", 1, 0).unwrap(),
                Availability::Unknown
            );
        }
        assert!(bili_availability(&json!({}), "1", "2", 1, 0).is_err());
    }

    #[test]
    fn bili_sale_time_uses_the_same_clock_offset_as_the_schedule() {
        let sale_start = 1_800_000_000_000_i64;
        for (local_now, offset_ms) in [
            (sale_start - 90_000, 120_000),
            (sale_start + 90_000, -120_000),
        ] {
            for (field, value) in [("sale_start", sale_start / 1000), ("saleStart", sale_start)] {
                let raw = json!({"screen_list":[{"id":1,"ticket_list":[{
                    "id":2,"num":3,"sale_flag":{"number":2},field:value,
                }]}]});
                let expected = if clock::delay_ms(sale_start, offset_ms, local_now) == 0 {
                    Availability::Available
                } else {
                    Availability::Unavailable
                };
                assert_eq!(
                    bili_availability(&raw, "1", "2", local_now, offset_ms).unwrap(),
                    expected,
                );
                assert_ne!(
                    bili_availability(&raw, "1", "2", local_now, 0).unwrap(),
                    expected,
                );
            }
        }
    }

    #[test]
    fn damai_does_not_confuse_purchase_limits_or_missing_stock_with_inventory() {
        let check =
            |ticket| damai_availability(&json!({"perform":{"skuList":[ticket]}}), "2").unwrap();
        assert_eq!(
            check(json!({"skuId":2,"limitQuantity":6,"enable":true})),
            Availability::Unknown
        );
        assert_eq!(
            check(json!({"skuId":2,"quantity":"2"})),
            Availability::Available
        );
        assert_eq!(
            check(json!({"skuId":2,"quantity":"0"})),
            Availability::Unavailable
        );
        assert_eq!(
            check(json!({"skuId":2,"quantity":2,"tags":[{"tagDesc":"缺货登记"}]})),
            Availability::Unavailable
        );
        assert_eq!(
            check(json!({"skuId":2,"quantity":2,"enable":"false"})),
            Availability::Unavailable
        );
    }

    #[tokio::test]
    async fn polling_recovers_and_stops_on_first_available_result() {
        let mut results = vec![
            Ok(Availability::Unavailable),
            Err("secret upstream body".into()),
            Ok(Availability::Unknown),
            Ok(Availability::Available),
        ]
        .into_iter();
        let mut reports = Vec::new();
        assert!(poll(
            0,
            10,
            || std::future::ready(results.next().unwrap()),
            |attempt, message| reports.push((attempt, message.to_string()))
        )
        .await
        .unwrap());
        assert_eq!(reports.last().unwrap().0, 4);
        assert!(!format!("{reports:?}").contains("secret"));
    }

    #[tokio::test]
    async fn polling_limits_and_repeated_errors_terminate() {
        let mut count = 0;
        assert!(!poll(
            0,
            2,
            || std::future::ready(Ok(Availability::Unavailable)),
            |n, _| count = n
        )
        .await
        .unwrap());
        assert_eq!(count, 2);
        let error = poll(
            0,
            0,
            || std::future::ready(Err("private cookie".into())),
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(error.contains("连续 5 次"));
        assert!(!error.contains("private"));
    }

    #[tokio::test]
    async fn deadline_drops_polling_before_another_query() {
        let mut checks = 0;
        let result = tokio::time::timeout(
            Duration::from_millis(5),
            poll(
                1000,
                0,
                || {
                    checks += 1;
                    std::future::ready(Ok(Availability::Unavailable))
                },
                |_, _| {},
            ),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(checks, 1);
    }
}
