export function normalizeProxySettings(settings = {}) {
    const subscriptions = (Array.isArray(settings.subscriptions) ? settings.subscriptions : [])
        .filter(item => item && typeof item.id === "string")
        .map(item => ({ id: item.id, name: String(item.name || ""), url: String(item.url || "").trim() }));
    return {
        proxy: typeof settings.proxy === "string" ? settings.proxy.trim() : "",
        proxyMode: settings.proxyMode === "subscription" ? "subscription" : "manual",
        subscriptions,
        subscriptionId: typeof settings.subscriptionId === "string" ? settings.subscriptionId : "",
        subscriptionNodeId: typeof settings.subscriptionNodeId === "string" ? settings.subscriptionNodeId : "",
    };
}

function validUrl(value, protocols) {
    try {
        const url = new URL(value);
        return !/\s/.test(value) && protocols.includes(url.protocol) && Boolean(url.hostname);
    } catch { return false; }
}

export function validateProxySettings(settings) {
    if (settings.proxy && !validUrl(settings.proxy, ["http:", "https:", "socks5:", "socks5h:"])) throw new Error("请输入有效的 HTTP、HTTPS 或 SOCKS5 代理地址");
    if (settings.subscriptions.length > 20) throw new Error("最多保存 20 个订阅");
    const ids = new Set();
    for (const source of settings.subscriptions) {
        if (!source.id || ids.has(source.id)) throw new Error("订阅标识重复，请删除后重新添加");
        ids.add(source.id);
        if (!source.name.trim()) throw new Error("请填写订阅名称");
        if (!validUrl(source.url, ["http:", "https:"])) throw new Error(`订阅「${source.name}」需要有效的 HTTP 或 HTTPS 链接`);
    }
    if (settings.proxyMode === "subscription" && !ids.has(settings.subscriptionId)) throw new Error("请选择要使用的订阅");
}

export function proxyAccount(settings, enabled) {
    if (!enabled) return { proxy: "", subscriptionId: "", subscriptionNodeId: "" };
    if (settings.proxyMode === "subscription") return { proxy: "", subscriptionId: settings.subscriptionId, subscriptionNodeId: settings.subscriptionNodeId };
    return { proxy: settings.proxy.trim(), subscriptionId: "", subscriptionNodeId: "" };
}

export function proxyProblem(settings, snapshots) {
    if (settings.proxyMode !== "subscription") return settings.proxy ? "" : "请先在设置中填写代理地址";
    const source = settings.subscriptions.find(item => item.id === settings.subscriptionId);
    if (!source) return "请先在设置中添加并选择订阅";
    const snapshot = snapshots[source.id];
    if (!snapshot || snapshot.url !== source.url || !snapshot.nodes?.length) return "订阅节点尚未就绪，请在设置中刷新订阅";
    if (settings.subscriptionNodeId && !snapshot.nodes.some(node => node.id === settings.subscriptionNodeId)) return "所选节点已不在订阅中，请在设置中重新选择";
    return "";
}

export function proxyNodeName(node) {
    const name = String(node.name || "").replace(/(?:%[\da-f]{2})+/gi, encoded => {
        try { return decodeURIComponent(encoded); } catch { return encoded; }
    }).trim();
    if (name && name !== "proxy") return name;
    const address = [node.host, node.port].filter(Boolean).join(":");
    return address ? `未命名节点 · ${address}` : "未命名节点";
}
