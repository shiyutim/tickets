use crate::{
    clock,
    http::{self, first, number, string, Account},
    tasks::{Outcome, TaskContext},
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

const SHOW: &str = "https://show.bilibili.com";
const MALL: &str = "https://mall.bilibili.com";
const ORDERS: &str = "https://show.bilibili.com/platform/orderList.html";

pub fn code(response: &Value) -> i64 {
    let code = first(response, &["errno", "code"]);
    code.as_i64()
        .or_else(|| code.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(-1)
}

fn message(response: &Value) -> String {
    let raw = string(first(response, &["msg", "message"]));
    let hint = match code(response) {
        -101 => "登录已过期，请重新填写 Cookie",
        100044 | -352 => "请在官方购票页完成人机验证，再重新启动任务",
        100003 | 100079 => "已存在购买订单或达到限购数量",
        100048 => "已有未完成订单，请先前往支付",
        100051 => "订单凭证已过期",
        100034 => "票价已变化，请重新加载票档并确认价格",
        100041 => "尚未到开售时间",
        100009 => "库存不足",
        100001 => "暂无可售票或登录状态异常",
        100016 | 100017 | 100039 => "当前项目或票档已停止销售",
        3 | 221 | 900001 | 900002 => "请求较多，稍后重试",
        _ => "购票请求未成功",
    };
    if raw.is_empty() {
        format!("{hint}（{}）", code(response))
    } else {
        format!("{hint}：{raw}（{}）", code(response))
    }
}

fn data(response: Value) -> Result<Value, String> {
    if code(&response) != 0 || response["success"] == false {
        return Err(message(&response));
    }
    if !response["data"].is_object() {
        return Err("接口未返回有效数据".into());
    }
    Ok(response["data"].clone())
}

fn project_data(response: Value) -> Result<Value, String> {
    if response.get("code").is_none()
        && response.get("errno").is_none()
        && response["success"] == true
        && response["data"].is_object()
    {
        Ok(response["data"].clone())
    } else {
        data(response)
    }
}

async fn project(client: &Client, project_id: i64) -> Result<Value, String> {
    let new = http::json(
        client
            .post(format!("{MALL}/mall-search-items/items_detail/info"))
            .header("origin", MALL)
            .header(
                "referer",
                format!("{MALL}/neul-next/ticket-renovation/detail.html?id={project_id}"),
            )
            .json(&json!({ "itemsId": project_id, "itemsDetailPageType": 3 })),
    )
    .await
    .and_then(project_data);
    match new {
        Ok(value)
            if first(&value, &["screenList", "screen_list"])
                .as_array()
                .is_some_and(|s| !s.is_empty()) =>
        {
            Ok(value)
        }
        _ => data(
            http::json(
                client
                    .get(format!("{SHOW}/api/ticket/project/getV2"))
                    .query(&[
                        ("version", "134".to_string()),
                        ("id", project_id.to_string()),
                        ("project_id", project_id.to_string()),
                    ]),
            )
            .await?,
        ),
    }
}

#[tauri::command]
pub async fn bili_project(account: Account, project_id: i64) -> Result<Value, String> {
    if project_id <= 0 {
        return Err("请输入有效的项目编号".into());
    }
    project(&http::client(&account, SHOW)?, project_id).await
}

#[tauri::command]
pub async fn bili_buyers(account: Account, project_id: i64) -> Result<Value, String> {
    let value = data(
        http::json(
            http::client(&account, SHOW)?
                .get(format!("{SHOW}/api/ticket/buyer/list"))
                .query(&[
                    ("is_default", "".into()),
                    ("projectId", project_id.to_string()),
                ]),
        )
        .await?,
    )?;
    value
        .get("list")
        .filter(|v| v.is_array())
        .cloned()
        .ok_or("未返回观演人列表，请先在会员购添加实名信息".into())
}

#[tauri::command]
pub async fn bili_addresses(account: Account) -> Result<Value, String> {
    let value = data(
        http::json(http::client(&account, SHOW)?.get(format!("{SHOW}/api/ticket/addr/list")))
            .await?,
    )?;
    Ok(value["addr_list"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into())
}

#[tauri::command]
pub async fn bili_screens(
    account: Account,
    project_id: i64,
    date: String,
) -> Result<Value, String> {
    data(
        http::json(
            http::client(&account, SHOW)?
                .get(format!("{SHOW}/api/ticket/project/infoByDate"))
                .query(&[("id", project_id.to_string()), ("date", date)]),
        )
        .await?,
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    account: Account,
    project_id: i64,
    screen_id: i64,
    sku_id: i64,
    count: u32,
    unit_price: i64,
    buyers: Vec<Value>,
    buyer: String,
    tel: String,
    #[serde(default)]
    deliver_info: Value,
    #[serde(default)]
    requires_delivery: bool,
    #[serde(default)]
    date: String,
}

pub fn validate(value: &Value) -> Result<(), String> {
    let config: Config =
        serde_json::from_value(value.clone()).map_err(|_| "Bilibili 任务配置不完整")?;
    if config.project_id <= 0 || config.screen_id <= 0 || config.sku_id <= 0 {
        return Err("请选择有效的项目、场次和票档".into());
    }
    if config.unit_price <= 0 || config.unit_price > 100_000_000 {
        return Err("票价无效，请重新加载商品".into());
    }
    if config.count == 0 || config.count > 20 || config.buyers.len() != config.count as usize {
        return Err("购买数量必须与观演人数一致，且最多 20 张".into());
    }
    let mut ids = std::collections::HashSet::new();
    for buyer in &config.buyers {
        let id = string(&buyer["id"]);
        if id.is_empty()
            || !ids.insert(id)
            || string(&buyer["name"]).is_empty()
            || string(&buyer["personal_id"]).is_empty()
        {
            return Err("所选观演人信息不完整或重复，请重新加载实名信息".into());
        }
    }
    if config.buyer.trim().is_empty()
        || config.tel.len() < 6
        || !config.tel.chars().all(|c| c.is_ascii_digit() || c == '+')
    {
        return Err("请填写联系人姓名和有效电话".into());
    }
    if config.requires_delivery
        && (number(&config.deliver_info["addr_id"]) <= 0
            || string(&config.deliver_info["addr"]).is_empty())
    {
        return Err("该票档需要配送，请选择收货地址".into());
    }
    if http::cookie_value(&config.account.cookie, "SESSDATA").is_empty()
        || http::cookie_value(&config.account.cookie, "bili_jct").is_empty()
    {
        return Err(
            "Cookie 缺少 SESSDATA 或 bili_jct，请从已登录的 Bilibili 页面重新获取完整 Cookie"
                .into(),
        );
    }
    http::client(&config.account, SHOW)?;
    Ok(())
}

fn selected_ticket<'a>(payload: &'a Value, config: &Config) -> Option<(&'a Value, &'a Value)> {
    let screens = first(payload, &["screenList", "screen_list"]).as_array()?;
    let screen = screens
        .iter()
        .find(|s| number(first(s, &["id", "screenId", "screen_id"])) == config.screen_id)?;
    let tickets = first(screen, &["ticketList", "ticket_list"]).as_array()?;
    let ticket = tickets
        .iter()
        .find(|s| number(first(s, &["id", "skuId", "sku_id"])) == config.sku_id)?;
    Some((screen, ticket))
}

fn ticket_price(screen: &Value, ticket: &Value) -> i64 {
    number(first(ticket, &["price", "ticketPrice", "ticket_price"]))
        + number(first(screen, &["expressFee", "express_fee"])).max(0)
}

fn unavailable(item: &Value) -> bool {
    let clickable = first(item, &["clickable", "canClick"]);
    let text = string(clickable).trim().to_ascii_lowercase();
    if *clickable == true || matches!(text.as_str(), "1" | "true") {
        return false;
    }
    if *clickable == false || matches!(text.as_str(), "0" | "false") {
        return true;
    }
    let flag = first(item, &["sale_flag", "saleFlag"]);
    let nested = first(flag, &["number", "sale_flag_number", "saleFlagNumber"]);
    let status = if nested.is_null() {
        first(item, &["sale_flag_number", "saleFlagNumber"])
    } else {
        nested
    };
    matches!(
        number(status),
        1 | 3 | 4 | 5 | 7 | 8 | 9 | 101 | 102 | 103 | 105 | 106
    )
}

fn is_terminal(errno: i64) -> bool {
    matches!(
        errno,
        -101 | -111 | -352 | 100044 | 100003 | 100048 | 100079 | 100034 | 100016 | 100017 | 100039
    )
}

pub fn created_order(response: &Value) -> Option<String> {
    if code(response) != 0 || string(first(response, &["msg", "message"])).contains("defaultBBR") {
        return None;
    }
    let id = string(first(&response["data"], &["orderId", "order_id"]));
    if id.is_empty() || id == "0" || !id.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        Some(id)
    }
}

pub async fn run(context: &TaskContext) -> Result<Outcome, String> {
    let config: Config =
        serde_json::from_value(context.request.config.clone()).map_err(|_| "任务参数无效")?;
    let client = http::client(&config.account, SHOW)?;
    let page = format!(
        "{MALL}/neul-next/ticket-renovation/detail.html?id={}",
        config.project_id
    );
    context.report("running", "正在复核可售状态、票档价格与限购信息", 0, None);
    let detail = if config.date.is_empty() {
        project(&client, config.project_id).await?
    } else {
        bili_screens(
            config.account.clone(),
            config.project_id,
            config.date.clone(),
        )
        .await?
    };
    let (screen, ticket) =
        selected_ticket(&detail, &config).ok_or("所选场次或票档已变化，请重新加载商品")?;
    for (item, label) in [(screen, "场次"), (ticket, "票档")] {
        if unavailable(item) {
            return Ok(Outcome::action(
                format!("所选{label}当前不可购买，请重新加载商品确认状态"),
                page,
            ));
        }
    }
    if ticket_price(screen, ticket) != config.unit_price {
        return Ok(Outcome::action(
            "票价或配送费已变化，请重新加载票档并确认价格",
            page,
        ));
    }
    let limit = number(&first(ticket, &["static_limit", "staticLimit"])["num"]);
    if limit > 0 && i64::from(config.count) > limit {
        return Err(format!("当前票档限购 {limit} 张"));
    }
    if first(screen, &["is_seat", "isSeat"]) == true
        || number(first(screen, &["is_seat", "isSeat"])) == 1
    {
        return Ok(Outcome::action(
            "该场次需要选座，请在官方页面完成购票",
            page,
        ));
    }
    let csrf = http::cookie_value(&config.account.cookie, "bili_jct");
    let device_id = format!(
        "{:x}",
        md5::compute(format!(
            "{}{}",
            http::cookie_value(&config.account.cookie, "buvid3"),
            http::USER_AGENT
        ))
    );
    let mut prepared = Value::Null;
    let mut last_error = String::new();
    for attempt in 1..=context.request.max_attempts {
        if prepared["token"].as_str().unwrap_or_default().is_empty() {
            context.report("running", "正在准备订单", attempt, None);
            let credentials = context.credentials().await?;
            let response = http::json(client.post(format!("{SHOW}/api/ticket/order/prepare"))
                .query(&[("project_id", config.project_id)])
                .json(&json!({
                    "project_id": config.project_id, "screen_id": config.screen_id, "sku_id": config.sku_id,
                    "count": config.count, "order_type": 1, "buyer_info": config.buyers,
                    "ignoreRequestLimit": true, "ticket_agent": "", "newRisk": true,
                    "requestSource": "neul-next", "token": credentials["ctoken"], "csrf": csrf,
                }))).await?;
            if code(&response) == 0 && !string(&response["data"]["token"]).is_empty() {
                prepared = response["data"].clone();
            } else {
                last_error = message(&response);
                if is_terminal(code(&response)) {
                    return Ok(Outcome::action(last_error, page));
                }
            }
        }
        if !prepared.is_null() {
            let credentials = context.credentials().await?;
            let ptoken = string(&prepared["ptoken"]).replace('=', "");
            context.report("running", "正在提交订单", attempt, None);
            let response = http::json(client.post(format!("{SHOW}/api/ticket/order/createV2"))
                .query(&[("project_id", config.project_id.to_string()), ("ptoken", ptoken.clone())])
                .json(&json!({
                    "project_id": config.project_id, "screen_id": config.screen_id, "sku_id": config.sku_id,
                    "count": config.count, "pay_money": config.unit_price * i64::from(config.count), "order_type": 1,
                    "buyer_info": serde_json::to_string(&config.buyers).map_err(|_| "观演人信息无效")?,
                    "buyer": config.buyer, "tel": config.tel, "deliver_info": config.deliver_info.to_string(),
                    "token": prepared["token"], "ptoken": ptoken, "ctoken": credentials["ctoken"],
                    "timestamp": clock::now_ms() + context.request.offset_ms, "device_id": device_id,
                    "again": 1, "newRisk": true, "requestSource": "neul-next", "csrf": csrf,
                    "orderCreateUrl": format!("{SHOW}/api/ticket/order/createV2"),
                }))).await;
            let response = match response {
                Ok(value) => value,
                Err(_) => {
                    return Ok(Outcome::action(
                        "订单请求结果未确认，请先检查官方订单页，避免重复下单",
                        ORDERS.into(),
                    ))
                }
            };
            if let Some(order_id) = created_order(&response) {
                let url = format!("{SHOW}/platform/orderDetail.html?order_id={order_id}");
                context.report(
                    "running",
                    "订单已创建，正在确认支付入口",
                    attempt,
                    Some(url.clone()),
                );
                let pay = http::json(
                    client
                        .get(format!("{SHOW}/api/ticket/order/getPayParam"))
                        .query(&[("order_id", &order_id)]),
                )
                .await;
                let message = if pay.as_ref().is_ok_and(|r| code(r) == 0) {
                    "订单已创建，请前往会员购完成支付"
                } else {
                    "订单已创建，支付信息暂不可用，请在官方订单详情中确认"
                };
                return Ok(Outcome {
                    status: "succeeded",
                    message: message.into(),
                    order_url: Some(url),
                });
            }
            let errno = code(&response);
            last_error = message(&response);
            if is_terminal(errno) {
                let order_id = string(first(&response["data"], &["orderId", "order_id"]));
                let url = if !order_id.is_empty() && order_id.bytes().all(|c| c.is_ascii_digit()) {
                    format!("{SHOW}/platform/orderDetail.html?order_id={order_id}")
                } else if matches!(errno, 100003 | 100048 | 100079) {
                    ORDERS.into()
                } else {
                    page
                };
                return Ok(Outcome::action(last_error, url));
            }
            if errno == 0 && !string(first(&response, &["msg", "message"])).contains("defaultBBR") {
                return Ok(Outcome::action(
                    "接口未返回有效订单编号，请先检查官方订单页",
                    ORDERS.into(),
                ));
            }
            if errno == 100051 {
                prepared = Value::Null;
            }
            let delay = if matches!(errno, 3 | 221 | 900001 | 900002) {
                context.request.interval_ms.max(3000)
            } else {
                context.request.interval_ms
            };
            context.report(
                "running",
                format!("{last_error}；等待下一次尝试"),
                attempt,
                None,
            );
            if attempt < context.request.max_attempts {
                context.pause(delay).await;
            }
        } else {
            context.report(
                "running",
                format!("{last_error}；等待重新准备订单"),
                attempt,
                None,
            );
            if attempt < context.request.max_attempts {
                context.pause(context.request.interval_ms).await;
            }
        }
    }
    Err(format!("已达到尝试次数上限：{last_error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_requires_a_real_order_id() {
        assert_eq!(
            created_order(&json!({"errno":0,"data":{"orderId":123}})),
            Some("123".into())
        );
        assert_eq!(
            created_order(&json!({"code":"0","data":{"order_id":"123"}})),
            Some("123".into())
        );
        assert_eq!(
            created_order(&json!({"errno":0,"msg":"defaultBBR","data":{"orderId":123}})),
            None
        );
        assert_eq!(created_order(&json!({"errno":0,"data":{}})), None);
        assert_eq!(created_order(&json!({"data":{"orderId":123}})), None);
        assert!(is_terminal(100044));
        assert!(is_terminal(100048));
        assert!(!is_terminal(100051));
    }

    #[test]
    fn price_includes_delivery_and_supports_both_api_shapes() {
        assert_eq!(
            ticket_price(&json!({"expressFee":1200}), &json!({"ticketPrice":8800})),
            10000
        );
        assert_eq!(
            ticket_price(&json!({"express_fee":0}), &json!({"price":8800})),
            8800
        );
    }

    #[test]
    fn availability_honors_explicit_flags_before_sale_status() {
        for key in ["clickable", "canClick"] {
            for value in [json!(false), json!(0), json!("0"), json!("false")] {
                assert!(unavailable(&json!({ key: value, "saleFlagNumber": 2 })));
            }
            for value in [json!(true), json!(1), json!("1"), json!("true")] {
                assert!(!unavailable(&json!({ key: value, "saleFlagNumber": 101 })));
            }
        }
        assert!(unavailable(&json!({ "sale_flag_number": 4 })));
        assert!(unavailable(&json!({ "saleFlag": { "number": 105 } })));
        assert!(!unavailable(&json!({ "saleFlagNumber": 6 })));
        assert!(!unavailable(&json!({})));
    }

    #[test]
    fn detail_accepts_the_new_success_envelope_but_orders_require_a_code() {
        assert!(project_data(json!({ "success": true, "data": { "screenList": [] } })).is_ok());
        assert!(project_data(json!({ "success": false, "code": 0, "data": {} })).is_err());
        assert!(data(json!({ "success": true, "data": {} })).is_err());
    }
}
