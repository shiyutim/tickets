export const accountStorageKey = "tickets.accounts";
export const emptyAccounts = () => ({ items: [], defaults: {} });

export function cleanAccountDraft(value) {
    const { cookie, remember, ...draft } = value && typeof value === "object" && !Array.isArray(value) ? value : {};
    return draft;
}

export function validateAccount(input) {
    if (!["dm", "bilibili"].includes(input.platform)) throw new Error("请选择大麦或 Bilibili 平台");
    if (!input.name?.trim() || input.name.trim().length > 60) throw new Error("账号名称须为 1–60 个字符");
    if (typeof input.cookie !== "string" || !input.cookie.trim()) throw new Error("请填写 Cookie");
    if (/[\x00-\x1f\x7f]/.test(input.cookie) || input.cookie.length > 32768) throw new Error("Cookie 不能包含换行、控制字符，且不能超过 32 KB");
    if (!input.cookie.split(';').some(pair => /^[^=\s;]+=.*/.test(pair.trim()))) throw new Error("请粘贴请求头中完整的 Cookie 内容");
}

export function upsertAccount(state, input, createId = () => crypto.randomUUID(), now = Date.now()) {
    validateAccount(input);
    const previous = input.id ? state.items.find(item => item.id === input.id) : null;
    if (input.id && !previous) throw new Error("该账号已被删除，请重新添加");
    if (previous && previous.platform !== input.platform) throw new Error("已有账号不能更换平台，请新建账号");
    const account = { id: previous?.id || createId(), platform: input.platform, name: input.name.trim(), cookie: input.cookie.trim(), updatedAt: now };
    const items = previous ? state.items.map(item => item.id === previous.id ? account : item) : [...state.items, account];
    const defaults = { ...state.defaults };
    if (!items.some(item => item.id === defaults[account.platform] && item.platform === account.platform)) defaults[account.platform] = account.id;
    return { items, defaults };
}

export function removeAccount(state, id) {
    const items = state.items.filter(item => item.id !== id);
    const defaults = { ...state.defaults };
    for (const platform of ["dm", "bilibili"]) {
        if (defaults[platform] === id) defaults[platform] = items.find(item => item.platform === platform)?.id || "";
    }
    return { items, defaults };
}

export function defaultAccount(state, id) {
    const account = state.items.find(item => item.id === id);
    if (!account) throw new Error("账号不存在");
    return { items: state.items, defaults: { ...state.defaults, [account.platform]: id } };
}

export function resolveAccount(state, platform, id = "") {
    const items = state.items.filter(item => item.platform === platform);
    if (id) return items.find(item => item.id === id);
    return items.find(item => item.id === state.defaults[platform]) || items[0];
}

export function loadAccounts(storage, createId = () => crypto.randomUUID(), now = Date.now()) {
    const raw = storage.getItem(accountStorageKey);
    let state = raw === null ? emptyAccounts() : JSON.parse(raw);
    if (!state || !Array.isArray(state.items) || !state.defaults || typeof state.defaults !== "object"
        || state.items.some(item => !item || typeof item.id !== "string" || !item.id || !["dm", "bilibili"].includes(item.platform) || typeof item.cookie !== "string" || typeof item.name !== "string")
        || new Set(state.items.map(item => item.id)).size !== state.items.length) throw new Error("账号存储格式无效");
    const updates = [];
    for (const platform of ["dm", "bilibili"]) {
        for (const prefix of ["tickets.draft", "tickets.monitor-draft"]) {
            const key = `${prefix}.${platform}`;
            const draftRaw = storage.getItem(key);
            if (!draftRaw) continue;
            let legacy;
            try { legacy = JSON.parse(draftRaw); } catch { continue; }
            if (!legacy || typeof legacy !== "object" || Array.isArray(legacy)) continue;
            if (!("cookie" in legacy) && !("remember" in legacy)) continue;
            const draft = cleanAccountDraft(legacy);
            if (legacy.remember === true && typeof legacy.cookie === "string" && legacy.cookie.trim()) {
                const cookie = legacy.cookie.trim();
                let account = state.items.find(item => item.platform === platform && item.cookie === cookie);
                if (!account) {
                    account = { id: createId(), platform, name: `${platform === "dm" ? "大麦" : "Bilibili"}${prefix.includes("monitor") ? "监控" : "购票"}账号（已迁入）`, cookie, updatedAt: now };
                    state = { items: [...state.items, account], defaults: { ...state.defaults, [platform]: state.defaults[platform] || account.id } };
                }
                draft.accountId = account.id;
            }
            updates.push([key, draft]);
        }
    }
    if (updates.length) {
        // 先保存账号，再清理旧表单；中途失败时可安全重试。
        storage.setItem(accountStorageKey, JSON.stringify(state));
        for (const [key, draft] of updates) storage.setItem(key, JSON.stringify(draft));
    }
    return state;
}
