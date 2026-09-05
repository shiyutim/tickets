export const platforms = {
    dm: { name: "大麦", subtitle: "演唱会 · 音乐节 · 现场演出", mark: "麦", color: "#087f6d", home: "https://m.damai.cn", orders: "https://orders.damai.cn/orderList" },
    bilibili: { name: "Bilibili", subtitle: "会员购 · 漫展 · 演出活动", mark: "哔", color: "#db638b", home: "https://show.bilibili.com", orders: "https://show.bilibili.com/platform/orderList.html" },
};

export function projectId(input, platform) {
    const text = String(input || "").trim();
    if (/^[1-9]\d*$/.test(text)) return text;
    try {
        const url = new URL(text);
        const domain = platform === "dm" ? "damai.cn" : "bilibili.com";
        if (url.protocol !== "https:" || !(url.hostname === domain || url.hostname.endsWith(`.${domain}`))) return "";
        const keys = platform === "dm" ? ["itemId"] : ["id", "project_id", "projectId", "itemsId"];
        for (const key of keys) {
            const value = url.searchParams.get(key);
            if (/^[1-9]\d*$/.test(value || "")) return value;
        }
    } catch { /* 输入完成后统一校验 */ }
    return "";
}

export function timestamp(value) {
    if (!value) return 0;
    const numeric = Number(value);
    if (Number.isFinite(numeric) && numeric > 0) return numeric < 1e11 ? numeric * 1000 : numeric;
    const text = String(value).trim();
    const explicit = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(text);
    const parsed = Date.parse(explicit ? text : `${text.replace(" ", "T")}+08:00`);
    return Number.isFinite(parsed) ? parsed : 0;
}

export function beijingInput(value) {
    return value ? new Date(value + 8 * 3600_000).toISOString().slice(0, 19) : "";
}

export function formatTime(value) {
    return value ? new Intl.DateTimeFormat("zh-CN", { timeZone: "Asia/Shanghai", year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }).format(value) : "立即开始";
}

export function money(cents) {
    return (Number(cents || 0) / 100).toLocaleString("zh-CN", { minimumFractionDigits: 0, maximumFractionDigits: 2 });
}

export function maskId(value) {
    const text = String(value || "");
    return text.length > 7 ? `${text.slice(0, 3)}${"*".repeat(8)}${text.slice(-4)}` : text;
}

const first = (value, keys, fallback = "") => keys.map(key => value?.[key]).find(item => item !== undefined && item !== null && item !== "") ?? fallback;

const biliSaleStatuses = {
    1: "未开售", 2: "预售", 3: "停售", 4: "售罄", 5: "不可用", 6: "库存紧张", 7: "不可售",
    8: "暂时售罄", 9: "无购买资格", 101: "未开始", 102: "已结束", 103: "未完成", 105: "下架", 106: "已取消",
};
const biliUnavailableStatuses = new Set([1, 3, 4, 5, 7, 8, 9, 101, 102, 103, 105, 106]);

function biliAvailability(item) {
    const flag = first(item, ["sale_flag", "saleFlag"], {});
    const flagNumber = Number(first(flag, ["number", "sale_flag_number", "saleFlagNumber"], first(item, ["sale_flag_number", "saleFlagNumber"], 0)));
    const value = first(item, ["clickable", "canClick"], null);
    const clickable = typeof value === "string" ? value.trim().toLowerCase() : value;
    const disabled = [false, 0, "false", "0"].includes(clickable)
        || (![true, 1, "true", "1"].includes(clickable) && biliUnavailableStatuses.has(flagNumber));
    const status = (typeof flag === "string" ? flag : first(flag, ["display_name", "displayName"]))
        || biliSaleStatuses[flagNumber] || (disabled ? "暂不可购" : "");
    return { disabled, status, disabledReason: disabled ? status : "" };
}

export function biliScreens(raw = []) {
    return raw.map(screen => {
        const deliveryFee = Math.max(0, Number(first(screen, ["express_fee", "expressFee"], 0)));
        return {
            id: String(first(screen, ["id", "screen_id", "screenId"])),
            name: first(screen, ["name", "screen_name", "screenName", "start_time_str", "startTimeStr", "dateStr"]),
            requiresSeat: [true, 1, "1"].includes(first(screen, ["is_seat", "isSeat"], false)),
            deliveryFee,
            ...biliAvailability(screen),
            tickets: first(screen, ["ticket_list", "ticketList"], []).map(ticket => ({
                id: String(first(ticket, ["id", "sku_id", "skuId"])),
                name: first(ticket, ["desc", "ticket_desc", "ticketDesc", "sku_desc", "skuDesc", "sku_name", "skuName", "name"]),
                price: Number(first(ticket, ["price", "ticket_price", "ticketPrice"], 0)) + deliveryFee,
                limit: Number(first(ticket, ["static_limit", "staticLimit"], {}).num || 20),
                saleStart: timestamp(first(ticket, ["sale_start", "saleStart"])),
                ...biliAvailability(ticket),
            })),
        };
    });
}

export function normalizeBili(raw, id) {
    let image = first(raw, ["projectCover", "cover", "performance_image", "performanceImage"]);
    try { image = JSON.parse(image); } catch { /* 封面也可能是 URL */ }
    if (Array.isArray(image)) image = image[0]?.url || image[0];
    if (image && typeof image === "object") image = image.url || image.large || "";
    return {
        id: String(first(raw, ["id", "projectId", "itemsId"], id)),
        name: first(raw, ["name", "projectName", "project_name"], "会员购活动"),
        venue: first(raw, ["venue_info", "skuVenueInfo"], {}).name || first(raw, ["venue_info", "skuVenueInfo"], {}).venueName || "",
        image: typeof image === "string" ? image.replace(/^http:/, "https:") : "",
        screens: biliScreens(first(raw, ["screen_list", "screenList"], [])),
        dates: first(raw, ["sales_dates", "salesDates"], []).map(date => typeof date === "string" ? date : date.date).filter(Boolean),
        saleStart: 0,
        requiresDelivery: raw.has_eticket === false,
    };
}

export function normalizeDamai(raw, id) {
    const base = raw.staticData?.itemBase || {};
    const item = raw.item || {};
    return {
        id: String(base.itemId || id), name: base.itemName || "大麦演出", image: (base.itemPic || "").replace(/^http:/, "https:"),
        venue: [base.cityName, base.venueName].filter(Boolean).join(" · "),
        saleStart: timestamp(item.sellStartTime), dates: [],
        screens: (item.performBases || []).flatMap(group => (group.performs || []).map(perform => ({
            id: String(perform.performId), name: perform.performName || group.name,
            status: group.performBaseTagDesc || "", tickets: [],
        }))),
    };
}

export function normalizeDamaiTickets(raw) {
    return (raw.perform?.skuList || []).map(sku => ({
        id: String(sku.skuId), name: sku.priceName, price: Math.round(Number(sku.price) * 100),
        signKey: String(raw.itemBasicInfo?.t || ""), limit: Number(sku.limitQuantity || 20),
        status: (sku.tags || []).map(tag => tag.tagDesc).join(" · "),
    }));
}

export function validateSelection({ platform, project, screen, ticket, count, buyers, buyer, tel, address, scheduled, startAt }) {
    if (!project || !screen || !ticket) return "请先选择场次和票档";
    if (screen.disabled) return `该场次不可购买：${screen.disabledReason || "请重新选择场次"}`;
    if (ticket.disabled) return `该票档不可购买：${ticket.disabledReason || "请重新选择票档"}`;
    if (screen.requiresSeat) return "该场次需要选座，请在官方页面完成购票";
    if (!Number.isInteger(count) || count < 1 || count > Math.min(ticket.limit || 20, 20)) return "购买数量超出票档限购范围";
    if (buyers.length !== count) return "请选择与购买张数一致的观演人";
    if (new Set(buyers).size !== buyers.length) return "请勿重复选择同一观演人";
    if (platform === "bilibili") {
        if (!buyer?.trim() || !/^\+?\d{6,20}$/.test(tel || "")) return "请填写联系人姓名和有效电话";
        if ((project.requiresDelivery || screen.deliveryFee > 0) && !address) return "该票档需要配送，请选择收货地址";
    }
    if (scheduled && !timestamp(startAt)) return "请选择有效的预约时间（北京时间）";
    return "";
}
