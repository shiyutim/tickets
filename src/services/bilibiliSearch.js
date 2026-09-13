export const BILI_SEARCH_PAGE_SIZE = 20;

const text = value => typeof value === "string" ? value.trim() : "";
const nonnegativeInteger = value => Number.isSafeInteger(value) && value >= 0;

function searchId(value) {
    if (typeof value !== "string" && typeof value !== "number") return "";
    const id = String(value).trim();
    return /^[1-9]\d*$/.test(id) && Number.isSafeInteger(Number(id)) ? id : "";
}

function searchImage(value) {
    try {
        const url = new URL(text(value));
        if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) return "";
        url.protocol = "https:";
        return url.href;
    } catch {
        return "";
    }
}

function searchPrice(item) {
    if (item.is_free === true) return "免费";
    if (item.is_price === false || !nonnegativeInteger(item.price_low)) return "票价待公布";
    const amount = (item.price_low / 100).toFixed(2).replace(/\.?0+$/, "");
    const from = !nonnegativeInteger(item.price_high) || item.price_high > item.price_low;
    return `¥${amount}${from ? " 起" : ""}`;
}

export function normalizeBiliSearch(raw, page = 1) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error("搜索结果格式异常，请重试");
    if (!Number.isSafeInteger(page) || page < 1) throw new Error("搜索页码无效");

    const total = nonnegativeInteger(raw.total) ? raw.total : null;
    const result = raw.result === null && raw.total === 0 ? [] : raw.result;
    if (!Array.isArray(result)) throw new Error("搜索结果格式异常，请重试");

    const seen = new Set();
    const items = [];
    for (const item of result) {
        if (!item || typeof item !== "object" || Array.isArray(item)) continue;
        const id = searchId(item.id ?? item.ticket_id);
        if (!id || seen.has(id)) continue;
        seen.add(id);
        items.push({
            id,
            name: text(item.project_name) || text(item.title) || "会员购活动",
            image: searchImage(item.cover),
            venue: [...new Set([text(item.city), text(item.venue_name)].filter(Boolean))].join(" · "),
            date: text(item.tlabel),
            price: searchPrice(item),
            status: text(item.sale_flag),
        });
    }

    const hasMore = total !== null ? page * BILI_SEARCH_PAGE_SIZE < total
        : typeof raw.isLastBrush === "boolean" ? !raw.isLastBrush
            : result.length >= BILI_SEARCH_PAGE_SIZE;
    return { items, total, hasMore };
}
