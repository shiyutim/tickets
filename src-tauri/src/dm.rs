use crate::{
    clock,
    http::{self, Account},
    tasks::{Outcome, TaskContext},
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const ORIGIN: &str = "https://m.damai.cn";
const ORDERS: &str = "https://orders.damai.cn/orderList";

struct Damai {
    client: Client,
    token: String,
}

impl Damai {
    fn new(account: &Account) -> Result<Self, String> {
        let token = http::cookie_value(&account.cookie, "_m_h5_tk")
            .split('_')
            .next()
            .unwrap_or_default()
            .to_string();
        if token.is_empty() {
            return Err("Cookie 缺少 _m_h5_tk，请从大麦 H5 页面重新获取".into());
        }
        Ok(Self {
            client: http::client(account, ORIGIN)?,
            token,
        })
    }

    async fn request(
        &self,
        api: &str,
        version: &str,
        data: &Value,
        credentials: Option<&Value>,
        secret: Option<(&str, &str)>,
    ) -> Result<Value, String> {
        let data = data.to_string();
        let t = clock::now_ms().to_string();
        let signature = format!(
            "{:x}",
            md5::compute(format!("{}&{t}&12574478&{data}", self.token))
        );
        let mut query = vec![
            ("jsv", "2.7.2"),
            ("appKey", "12574478"),
            ("t", &t),
            ("sign", &signature),
            ("type", "originaljson"),
            ("dataType", "json"),
            ("v", version),
            ("api", api),
            ("H5Request", "true"),
            ("AntiCreep", "true"),
            ("AntiFlood", "true"),
            ("globalCode", "ali.china.damai"),
            ("ttid", "#t#ip##_h5_2014"),
        ];
        if let Some(pair) = secret {
            query.push(pair);
        }
        let url = format!("https://mtop.damai.cn/h5/{api}/{version}/");
        let request = if let Some(credentials) = credentials {
            let ua = credentials["ua"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("大麦凭证尚未就绪")?;
            let umid = credentials["umidtoken"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("大麦设备凭证尚未就绪")?;
            self.client.post(url).query(&query).form(&[
                ("data", data.as_str()),
                ("bx-ua", ua),
                ("bx-umidtoken", umid),
            ])
        } else {
            query.push(("data", &data));
            self.client.get(url).query(&query)
        };
        http::json(request).await
    }
}

fn result(response: &Value) -> Result<Value, String> {
    let message = response["ret"]
        .as_array()
        .map(|a| a.iter().map(http::string).collect::<Vec<_>>().join("；"))
        .unwrap_or_default();
    if !message.starts_with("SUCCESS::") {
        return Err(if message.is_empty() {
            "大麦返回的数据格式无效".into()
        } else {
            message
        });
    }
    let data = &response["data"];
    match &data["result"] {
        Value::String(raw) => serde_json::from_str(raw).map_err(|_| "无法解析大麦商品数据".into()),
        Value::Null => Ok(data.clone()),
        value => Ok(value.clone()),
    }
}

fn detail_payload(item_id: &str, perform_id: Option<&str>) -> Value {
    json!({
        "itemId": item_id, "bizCode": "ali.china.damai", "scenario": "itemsku",
        "exParams": json!({ "dataType": if perform_id.is_some() { 2 } else { 4 }, "dataId": perform_id.unwrap_or(""), "privilegeActId": "" }).to_string(),
        "dmChannel": "damai@damaih5_h5",
    })
}

fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        Err("请输入有效的商品或场次编号".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn dm_project(account: Account, project_id: String) -> Result<Value, String> {
    valid_id(&project_id)?;
    let api = Damai::new(&account)?;
    let data = result(
        &api.request(
            "mtop.alibaba.damai.detail.getdetail",
            "1.2",
            &detail_payload(&project_id, None),
            None,
            None,
        )
        .await?,
    )?;
    let item = &data["detailViewComponentMap"]["item"];
    if !item.is_object() {
        return Err("未找到商品信息，请检查商品链接".into());
    }
    if matches!(item["item"]["buyBtnStatus"].as_str(), Some("303" | "100")) {
        return Err(format!(
            "{} {}",
            http::string(&item["item"]["buyBtnText"]),
            http::string(&item["item"]["buyBtnTips"])
        ));
    }
    Ok(item.clone())
}

#[tauri::command]
pub async fn dm_tickets(
    account: Account,
    project_id: String,
    screen_id: String,
) -> Result<Value, String> {
    valid_id(&project_id)?;
    valid_id(&screen_id)?;
    result(
        &Damai::new(&account)?
            .request(
                "mtop.alibaba.detail.subpage.getdetail",
                "2.0",
                &detail_payload(&project_id, Some(&screen_id)),
                None,
                None,
            )
            .await?,
    )
}

#[tauri::command]
pub async fn dm_buyers(account: Account) -> Result<Value, String> {
    result(&Damai::new(&account)?.request("mtop.damai.wireless.user.customerlist.get", "2.0", &json!({
        "customerType": "default", "platform": "8", "comboChannel": "2", "dmChannel": "damai@damaih5_h5",
    }), None, None).await?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    account: Account,
    project_id: String,
    sku_id: String,
    sign_key: String,
    count: u32,
    buyers: Vec<String>,
}

pub fn validate(value: &Value) -> Result<(), String> {
    let config: Config = serde_json::from_value(value.clone()).map_err(|_| "大麦任务配置不完整")?;
    valid_id(&config.project_id)?;
    valid_id(&config.sku_id)?;
    if config.sign_key.is_empty() {
        return Err("请重新加载票档信息".into());
    }
    if config.count == 0 || config.count > 20 || config.buyers.len() != config.count as usize {
        return Err("购买数量必须与观演人数一致，且最多 20 张".into());
    }
    let unique: std::collections::HashSet<_> =
        config.buyers.iter().filter(|s| !s.is_empty()).collect();
    if unique.len() != config.buyers.len() {
        return Err("观演人不能重复或为空".into());
    }
    Damai::new(&config.account)?;
    Ok(())
}

fn order_payload(detail: &Value, buyers: &[String], count: u32) -> Result<Value, String> {
    let blocks = detail["data"].as_object().ok_or("订单确认信息缺少 data")?;
    let tags = [
        "dmPayType",
        "dmEttributesHiddenBlock",
        "dmContactEmail",
        "dmViewer",
        "dmDeliverySelectCard",
        "dmContactPhone",
        "confirmOrder",
        "dmDeliveryAddress",
        "dmContactName",
        "item",
    ];
    let mut selected = Map::new();
    for (key, value) in blocks {
        if !tags.contains(&value["tag"].as_str().unwrap_or_default()) {
            continue;
        }
        let mut block = value.clone();
        if block["tag"] == "dmViewer" {
            if let Some(viewers) = block["fields"]["viewerList"].as_array_mut() {
                let mut matched = 0;
                for viewer in viewers {
                    let used = buyers.contains(&http::string(&viewer["maskedIdentityNo"]));
                    if used && (viewer["isDisabled"] == true || viewer["disabled"] == true) {
                        return Err("所选观演人暂不可用，请在官方页面检查实名信息".into());
                    }
                    viewer["isUsed"] = json!(used);
                    if used {
                        matched += 1;
                    }
                }
                if matched != count {
                    return Err("订单中的观演人与所选名单不一致，请重新加载观演人".into());
                }
                block["fields"]["selectedNum"] = json!(matched);
            }
        }
        selected.insert(key.clone(), block);
    }
    if selected.is_empty()
        || !detail["hierarchy"]["structure"].is_object()
        || !detail["linkage"]["common"].is_object()
    {
        return Err("订单确认信息不完整，请重新加载商品".into());
    }
    let prefixes = [
        "dmPayDetailPopupWindowBlock_",
        "dmViewerBlock_",
        "dmContactBlock_",
        "dmItemBlock_",
        "dmDeliveryWayBlock_",
        "deliveryMethodOptions_",
        "confirmOrder_",
        "dmOrderSubmitBlock_",
        "order_",
        "dmPayTypeBlock_",
        "dmTopNotificationBlock_",
    ];
    let structure: Map<String, Value> = detail["hierarchy"]["structure"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| prefixes.iter().any(|prefix| key.starts_with(prefix)))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let params = json!({
        "data": Value::Object(selected).to_string(),
        "linkage": json!({ "common": {
            "compress": detail["linkage"]["common"]["compress"].as_bool().unwrap_or(false),
            "submitParams": detail["linkage"]["common"]["submitParams"],
            "validateParams": detail["linkage"]["common"]["validateParams"],
        }, "signature": detail["linkage"]["signature"] }).to_string(),
        "hierarchy": json!({ "structure": structure }).to_string(),
    });
    Ok(json!({ "params": params.to_string(), "feature": json!({
        "subChannel": "damai@damaih5_h5", "returnUrl": "https://m.damai.cn/damai/pay-success/index.html",
        "serviceVersion": "2.0.0", "dataTags": "sqm:dianying.h5.unknown.value",
    }).to_string() }))
}

fn terminal(message: &str) -> bool {
    [
        "VALIDATE",
        "TOKEN",
        "SESSION",
        "登录",
        "令牌",
        "未支付",
        "限购",
        "实名",
    ]
    .iter()
    .any(|s| message.contains(s))
}

pub async fn run(context: &TaskContext) -> Result<Outcome, String> {
    let config: Config =
        serde_json::from_value(context.request.config.clone()).map_err(|_| "任务参数无效")?;
    let api = Damai::new(&config.account)?;
    let build = json!({
        "buyNow": true, "exParams": json!({
            "channel": "damai_app", "damai": "1", "umpChannel": "100031004", "subChannel": "damai@damaih5_h5",
            "atomSplit": 1, "signKey": config.sign_key, "rtc": 1, "serviceVersion": "2.0.0", "customerType": "default",
        }).to_string(),
        "buyParam": format!("{}_{}_{}", config.project_id, config.count, config.sku_id), "dmChannel": "damai@damaih5_h5",
    });
    let mut last_error = String::new();
    for attempt in 1..=context.request.max_attempts {
        context.report("running", "正在确认订单信息", attempt, None);
        let credentials = context.credentials().await?;
        let detail = api
            .request(
                "mtop.trade.order.build.h5",
                "4.0",
                &build,
                Some(&credentials),
                None,
            )
            .await
            .and_then(|r| result(&r));
        match detail {
            Ok(detail) => {
                let payload = order_payload(&detail, &config.buyers, config.count)?;
                let key = http::string(&detail["global"]["secretKey"]);
                let value = http::string(&detail["global"]["secretValue"]);
                let secret = if key.is_empty() {
                    None
                } else {
                    Some((key.as_str(), value.as_str()))
                };
                let credentials = context.credentials().await?;
                context.report("running", "正在提交订单", attempt, None);
                let response = match api
                    .request(
                        "mtop.trade.order.create.h5",
                        "4.0",
                        &payload,
                        Some(&credentials),
                        secret,
                    )
                    .await
                {
                    Ok(response) => response,
                    Err(_) => {
                        return Ok(Outcome::action(
                            "订单请求结果未确认，请先检查官方订单页，避免重复下单",
                            ORDERS.into(),
                        ))
                    }
                };
                match result(&response) {
                    Ok(_) => {
                        return Ok(Outcome {
                            status: "succeeded",
                            message: "订单已创建，请前往大麦完成支付".into(),
                            order_url: Some(ORDERS.into()),
                        })
                    }
                    Err(error) => last_error = error,
                }
            }
            Err(error) => last_error = error,
        }
        if terminal(&last_error) {
            return Ok(Outcome::action(last_error, ORDERS.into()));
        }
        context.report(
            "running",
            format!("{last_error}；等待下一次尝试"),
            attempt,
            None,
        );
        if attempt < context.request.max_attempts {
            context.pause(context.request.interval_ms).await;
        }
    }
    Err(format!("已达到尝试次数上限：{last_error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_viewer_count_and_hierarchy_are_preserved() {
        let detail = json!({
            "data": { "viewer": { "tag": "dmViewer", "fields": { "viewerList": [
                {"maskedIdentityNo": "a"}, {"maskedIdentityNo": "b"}
            ] } } },
            "linkage": { "common": {"compress": false}, "signature": "sig" },
            "hierarchy": { "structure": { "order_1": ["viewer"], "unrelated": [] } },
        });
        let payload = order_payload(&detail, &["b".into()], 1).unwrap();
        let params: Value = serde_json::from_str(payload["params"].as_str().unwrap()).unwrap();
        let data: Value = serde_json::from_str(params["data"].as_str().unwrap()).unwrap();
        assert_eq!(data["viewer"]["fields"]["selectedNum"], 1);
        assert_eq!(data["viewer"]["fields"]["viewerList"][0]["isUsed"], false);
        let hierarchy: Value = serde_json::from_str(params["hierarchy"].as_str().unwrap()).unwrap();
        assert!(hierarchy["structure"].get("order_1").is_some());
        assert!(hierarchy["structure"].get("unrelated").is_none());
        assert!(order_payload(&detail, &["missing".into()], 1).is_err());
    }
}
