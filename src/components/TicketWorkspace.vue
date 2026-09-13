<script setup>
import { reactive, ref, computed, watch, nextTick } from "vue";
import { Message } from "@arco-design/web-vue";
import UiIcon from "./common/UiIcon.vue";
import BiliProjectSearch from "./BiliProjectSearch.vue";
import MonitorControls from "./MonitorControls.vue";
import TaskStatus from "./TaskStatus.vue";
import { platforms, projectId, normalizeBili, normalizeDamai, normalizeDamaiTickets, biliScreens, maskId, money, timestamp, beijingInput, validateSelection } from "../services/platforms";
import { runtime, readLocal, saveLocal, call, record, currentTask, isActive, startTask, syncClock, openExternal, errorText } from "../services/runtime";
import { damaiCredentials } from "../services/credentials";
import { cleanAccountDraft, resolveAccount } from "../services/accounts";
import { proxyAccount, proxyProblem } from "../services/proxies";
import { normalizeWechat } from "../services/monitoring";

const props = defineProps({ platform: { type: String, required: true }, monitoring: Boolean });
const emit = defineEmits(["created", "busy"]);
const monitoring = computed(() => props.monitoring);
const meta = platforms[props.platform];
const isBili = props.platform === "bilibili";
const draftKey = `tickets.${props.monitoring ? "monitor-draft" : "draft"}.${props.platform}`;
const saved = cleanAccountDraft(readLocal(draftKey, {}));
const form = reactive({
    url: "", accountId: "", useProxy: false, count: 1, maxAttempts: 5, intervalMs: 1000,
    buyer: "", tel: "", scheduled: false, startAt: "", offsetMs: runtime.clock?.offsetMs || 0,
    ...saved, buyers: [], address: "", date: "",
    offsetMs: runtime.clock?.offsetMs ?? saved.offsetMs ?? 0,
});
watch(form, value => {
    if (runtime.accountError) return;
    const { buyers, address, date, mode, ...draft } = value;
    saveLocal(draftKey, cleanAccountDraft(draft));
}, { deep: true });
const project = ref(null);
const buyers = ref([]);
const addresses = ref([]);
const screenId = ref("");
const ticketId = ref("");
const loading = ref(false);
const searchLoading = ref(false);
const connectionEditing = ref(false);
const buyerLoading = ref(false);
const ticketLoading = ref(false);
const addressLoading = ref(false);
const starting = ref(false);
const error = ref("");
const buyerError = ref("");
const addressError = ref("");
const clockError = ref("");
const accounts = computed(() => runtime.accounts.items.filter(item => item.platform === props.platform));
const selectedAccount = computed(() => resolveAccount(runtime.accounts, props.platform, form.accountId));
const task = computed(() => currentTask(props.platform));
const running = computed(() => !monitoring.value && isActive(task.value));
const locked = computed(() => running.value || starting.value);
const screen = computed(() => project.value?.screens.find(item => item.id === screenId.value));
const ticket = computed(() => screen.value?.tickets.find(item => item.id === ticketId.value));
const maxCount = computed(() => Math.min(ticket.value?.limit || 20, 20));
const needsDelivery = computed(() => isBili && (project.value?.requiresDelivery || screen.value?.deliveryFee > 0));
const id = computed(() => projectId(form.url, props.platform));
const account = () => ({ cookie: selectedAccount.value?.cookie || "", ...proxyAccount(runtime.settings, form.useProxy) });
const searchProblem = computed(() => runtime.accountError
    || (!selectedAccount.value ? "请先在设置的账号管理中添加并选择账号" : "")
    || (form.useProxy ? proxyProblem(runtime.settings, runtime.subscriptions) : ""));
let generation = 0;
let loadedAccount = "";
watch(() => starting.value || loading.value || ticketLoading.value || searchLoading.value, value => emit("busy", value));

watch(() => [form.url, selectedAccount.value?.id, selectedAccount.value?.cookie, form.useProxy, JSON.stringify(proxyAccount(runtime.settings, form.useProxy)), form.useProxy ? runtime.subscriptions[runtime.settings.subscriptionId]?.updatedAt : 0], () => {
    generation++;
    if (running.value) return;
    project.value = null; buyers.value = []; addresses.value = [];
    screenId.value = ""; ticketId.value = ""; form.buyers = []; form.address = ""; form.date = "";
    error.value = ""; buyerError.value = ""; addressError.value = "";
});
watch(() => runtime.clock, sample => { if (sample && !locked.value) form.offsetMs = sample.offsetMs; });

function fail(value, title = "加载失败") {
    error.value = errorText(value);
    record(props.platform, title, "error", error.value);
}

async function loadProject() {
    if (locked.value || loading.value || ticketLoading.value) return;
    error.value = "";
    if (runtime.accountError) { error.value = runtime.accountError; return; }
    if (!selectedAccount.value) { error.value = "请先在设置的账号管理中添加并选择账号"; return; }
    if (!id.value || (isBili && !Number.isSafeInteger(Number(id.value)))) { error.value = "请输入有效的官方商品链接或项目编号"; return; }
    if (form.useProxy) {
        const problem = proxyProblem(runtime.settings, runtime.subscriptions);
        if (problem) { error.value = problem; return; }
    }
    const version = ++generation;
    const credentials = account();
    const projectNumber = id.value;
    loading.value = true;
    project.value = null; screenId.value = ""; ticketId.value = ""; buyers.value = []; addresses.value = [];
    form.buyers = []; form.address = ""; form.date = "";
    try {
        const raw = await call(isBili ? "bili_project" : "dm_project", { account: credentials, projectId: isBili ? Number(projectNumber) : projectNumber, ...(!isBili ? { allowUnavailable: monitoring.value } : {}) });
        if (version !== generation) return;
        project.value = isBili ? normalizeBili(raw, projectNumber) : normalizeDamai(raw, projectNumber);
        loadedAccount = JSON.stringify(credentials);
        if (project.value.saleStart > Date.now() + form.offsetMs) {
            form.startAt = beijingInput(project.value.saleStart); form.scheduled = true;
        }
        record(props.platform, `已加载 ${project.value.name}`, "success");
        const pending = monitoring.value ? [] : [loadBuyers(version)];
        if (isBili && !monitoring.value) pending.push(loadAddresses(version));
        if (project.value.screens.length === 1) pending.push(selectScreen(project.value.screens[0]));
        await Promise.allSettled(pending);
    } catch (value) { if (version === generation) fail(value); }
    finally { loading.value = false; }
}

async function selectSearchProject(projectNumber) {
    if (locked.value || loading.value || ticketLoading.value) return;
    form.url = projectNumber;
    await nextTick();
    await loadProject();
}

async function loadBuyers(version = generation) {
    if (!project.value || locked.value) return;
    buyerLoading.value = true; buyerError.value = ""; form.buyers = [];
    try {
        const raw = await call(isBili ? "bili_buyers" : "dm_buyers", { account: account(), ...(isBili ? { projectId: Number(project.value.id) } : {}) });
        if (version !== generation) return;
        if (!Array.isArray(raw)) throw new Error("观演人数据格式无效");
        buyers.value = raw.map(item => ({
            key: String(isBili ? item.id : item.maskedIdentityNo),
            name: isBili ? item.name : item.maskedName,
            identity: isBili ? maskId(item.personal_id) : item.maskedIdentityNo,
            raw: item,
        }));
        if (!raw.length) buyerError.value = "尚无观演人，请先在官方平台添加实名信息，再点击刷新。";
    } catch (value) { if (version === generation) { buyers.value = []; buyerError.value = errorText(value); } }
    finally { buyerLoading.value = false; }
}

async function loadAddresses(version = generation) {
    if (!project.value || locked.value) return;
    addressLoading.value = true; addressError.value = "";
    try {
        const raw = await call("bili_addresses", { account: account() });
        if (version !== generation) return;
        addresses.value = Array.isArray(raw) ? raw : [];
        form.address = "";
    } catch (value) { if (version === generation) addressError.value = errorText(value); }
    finally { addressLoading.value = false; }
}

async function selectScreen(item) {
    if (locked.value || ticketLoading.value || (!monitoring.value && item.disabled)) return;
    screenId.value = item.id; ticketId.value = "";
    if (isBili || item.tickets.length) return;
    ticketLoading.value = true; error.value = "";
    const version = generation;
    try {
        const raw = await call("dm_tickets", { account: account(), projectId: project.value.id, screenId: item.id });
        if (version !== generation) return;
        item.tickets = normalizeDamaiTickets(raw);
    } catch (value) { if (version === generation) fail(value, "票档加载失败"); }
    finally { ticketLoading.value = false; }
}

async function changeDate() {
    if (!form.date || locked.value) return;
    const version = ++generation;
    screenId.value = ""; ticketId.value = ""; ticketLoading.value = true;
    try {
        const raw = await call("bili_screens", { account: account(), projectId: Number(project.value.id), date: form.date });
        if (version === generation) project.value.screens = biliScreens(raw.screen_list || raw.screenList || []);
    } catch (value) { if (version === generation) { project.value.screens = []; fail(value, "场次加载失败"); } }
    finally { ticketLoading.value = false; }
}

function selectTicket(item) {
    if (locked.value || ticketLoading.value || !screen.value || (!monitoring.value && (screen.value.disabled || item.disabled))) return;
    ticketId.value = item.id;
    if (form.count > maxCount.value) { form.count = maxCount.value; form.buyers = []; }
    const start = item.saleStart || project.value.saleStart;
    if (start > Date.now() + form.offsetMs) { form.scheduled = true; form.startAt = beijingInput(start); }
}

async function calibrate() {
    clockError.value = "";
    try { const sample = await syncClock(); if (sample) form.offsetMs = sample.offsetMs; }
    catch (value) { clockError.value = errorText(value); }
}

async function startMonitor(options) {
    if (locked.value || !project.value || !screen.value || !ticket.value) return;
    error.value = "";
    if (form.useProxy) {
        const problem = proxyProblem(runtime.settings, runtime.subscriptions);
        if (problem) { error.value = problem; return; }
    }
    if (loadedAccount !== JSON.stringify(account())) { error.value = "账号或代理设置已变化，请重新加载商品"; return; }
    starting.value = true;
    try {
        const { endAt, wechat, ...timing } = options;
        const created = await startTask({
            id: `${props.platform}-${crypto.randomUUID()}`, mode: "monitor", platform: props.platform,
            title: `${project.value.name} · ${screen.value.name} · ${ticket.value.name}`,
            ...timing,
            config: { account: account(), projectId: String(project.value.id), screenId: String(screen.value.id), skuId: String(ticket.value.id), date: form.date, endAt, wechat },
        });
        Message.success("监控已添加，可以继续添加其他活动或票档");
        emit("created", created);
    } catch (value) { fail(value, "监控启动失败"); }
    finally { starting.value = false; }
}

async function start() {
    if (locked.value) return;
    if (form.useProxy) {
        const problem = proxyProblem(runtime.settings, runtime.subscriptions);
        if (problem) { error.value = problem; return; }
    }
    const address = addresses.value.find(item => String(item.id) === form.address);
    const problem = validateSelection({ platform: props.platform, project: project.value, screen: screen.value, ticket: ticket.value,
        count: form.count, buyers: form.buyers, buyer: form.buyer, tel: form.tel, address, scheduled: form.scheduled, startAt: form.startAt });
    if (problem) { error.value = problem; Message.warning(problem); return; }
    if (loadedAccount !== JSON.stringify(account())) { error.value = "账号或代理设置已变化，请重新加载商品"; return; }
    const wechat = normalizeWechat(runtime.settings.wechat);
    starting.value = true; error.value = "";
    try {
        if (!isBili) await damaiCredentials();
        if (runtime.settings.autoSync && (!runtime.clock || Date.now() - runtime.clock.sampledAt > 300_000)) await calibrate();
        const common = { account: account(), projectId: project.value.id, skuId: ticket.value.id, count: form.count, wechat };
        const config = isBili ? {
            ...common, projectId: Number(project.value.id), screenId: Number(screen.value.id), skuId: Number(ticket.value.id),
            unitPrice: ticket.value.price, buyers: buyers.value.filter(item => form.buyers.includes(item.key)).map(item => item.raw),
            buyer: form.buyer.trim(), tel: form.tel.trim(), requiresDelivery: needsDelivery.value, date: form.date,
            deliverInfo: address ? { name: address.name, tel: address.phone, addr_id: address.id, addr: `${address.prov || ''}${address.city || ''}${address.area || ''}${address.addr || ''}` } : {},
        } : { ...common, signKey: ticket.value.signKey, buyers: [...form.buyers] };
        await startTask({
            id: `${props.platform}-${crypto.randomUUID()}`, platform: props.platform,
            title: `${project.value.name} · ${screen.value.name} · ${ticket.value.name}`,
            startAt: form.scheduled ? timestamp(form.startAt) : 0, offsetMs: Number(form.offsetMs || 0),
            maxAttempts: form.maxAttempts, intervalMs: form.intervalMs, config,
        });
    } catch (value) { fail(value, "任务启动失败"); }
    finally { starting.value = false; }
}
</script>

<template>
    <div class="workspace" :class="{ 'bili-workspace': isBili }">
        <div v-if="!monitoring" class="page-heading"><div><div v-if="!isBili" class="eyebrow">TICKET WORKSPACE <span>/ 01</span></div><h1>{{ meta.name }}<span v-if="isBili" class="heading-suffix">会员购</span><span v-else class="heading-suffix">购票工作台</span></h1><p v-if="!isBili">{{ meta.subtitle }}，为下一场期待做好准备。</p></div><button class="button secondary" @click="openExternal(meta.orders)">我的订单<UiIcon name="launch" /></button></div>
        <div v-if="!isBili" class="workflow"><span :class="{ done: project }"><b><UiIcon v-if="project" name="check" /><template v-else>01</template></b>连接账号与活动</span><i></i><span :class="{ current: project, done: ticket }"><b><UiIcon v-if="ticket" name="check" /><template v-else>02</template></b>选择场次与票档</span><i></i><span :class="{ current: ticket, done: running }"><b>03</b>{{ monitoring ? '设置并开始监控' : '确认并开始购票' }}</span></div>
        <div v-if="error" class="notice error" role="alert"><UiIcon name="info" /><span>{{ error }}</span><button aria-label="关闭提示" class="icon-button" @click="error = ''"><UiIcon name="close" /></button></div>
        <div class="workbench-grid">
            <div class="connection-column">
                <section class="panel connection-panel" aria-label="连接活动">
                    <div class="section-heading"><span class="section-icon"><UiIcon name="link" /></span><div><h2>连接活动</h2><p v-if="!isBili">从一个心仪的活动开始</p></div><span class="tiny-label">01</span></div>
                    <template v-if="isBili">
                        <div class="connection-account-row">
                            <label class="field-label" :for="`${platform}-account`">账号</label>
                            <select :id="`${platform}-account`" class="text-input" :value="selectedAccount?.id || ''" @change="form.accountId = $event.target.value" :disabled="locked || loading || ticketLoading">
                                <option value="" disabled>{{ accounts.length ? '请选择账号' : '请先添加账号' }}</option>
                                <option v-for="item in accounts" :key="item.id" :value="item.id">{{ item.name }}{{ runtime.accounts.defaults[platform] === item.id ? '（默认）' : '' }}</option>
                            </select>
                            <router-link to="/settings" class="text-button">管理账号</router-link>
                        </div>
                        <p v-if="form.accountId && !selectedAccount" class="inline-error">所选账号已删除，请选择其他账号。</p>
                        <BiliProjectSearch :account="account()" :problem="searchProblem" :disabled="locked || loading || ticketLoading" :ready="runtime.ready" :value="form.url" :project="project" :project-loading="loading" @select="selectSearchProject" @busy="searchLoading = $event" @editing="connectionEditing = $event" />
                        <div class="connection-tools">
                            <label class="check-label"><input type="checkbox" v-model="form.useProxy" :disabled="locked || loading || ticketLoading" />使用代理</label>
                            <router-link v-if="form.useProxy" to="/settings" class="text-button">代理设置</router-link>
                            <button type="button" class="text-button clock-tool" :disabled="runtime.syncing || locked" :title="runtime.clock ? `${runtime.clock.source} · 修正 ${form.offsetMs} ms · 往返 ${runtime.clock.roundTripMs} ms` : '同步服务器时间'" @click="calibrate"><UiIcon :name="runtime.syncing ? 'refresh' : 'clock'" :class="{ spinning: runtime.syncing }" />{{ runtime.syncing ? '正在校准…' : runtime.clock ? '时间已校准' : '校准时间' }}</button>
                            <button type="button" class="text-button official-link" @click="openExternal(meta.home)">会员购官网<UiIcon name="launch" /></button>
                        </div>
                        <p v-if="clockError" class="inline-error" role="alert">{{ clockError }}</p>
                    </template>
                    <form v-else @submit.prevent="loadProject" class="stack-form">
                        <label class="field-label" :for="`${platform}-url`">活动链接或编号 <span>*</span></label>
                        <input :id="`${platform}-url`" v-model="form.url" :disabled="locked || loading" class="text-input" :placeholder="isBili ? '粘贴会员购链接或项目 ID' : '粘贴大麦链接或 itemId'" autocomplete="off" />
                        <small class="field-hint" :class="{ 'accent-text': id }">{{ id ? `已识别项目 ${id}` : '自动识别官方商品链接中的项目编号' }}</small>
                        <div class="label-row"><label class="field-label" :for="`${platform}-account`">使用账号 <span>*</span></label><router-link to="/settings" class="text-button">管理账号</router-link></div>
                        <select :id="`${platform}-account`" class="text-input" :value="selectedAccount?.id || ''" @change="form.accountId = $event.target.value" :disabled="locked || loading || ticketLoading">
                            <option value="" disabled>{{ accounts.length ? '请选择账号' : '请先添加账号' }}</option>
                            <option v-for="item in accounts" :key="item.id" :value="item.id">{{ item.name }}{{ runtime.accounts.defaults[platform] === item.id ? '（默认）' : '' }}</option>
                        </select>
                        <small v-if="form.accountId && !selectedAccount" class="inline-error">所选账号已删除，请选择其他账号。</small>
                        <div class="privacy-note"><UiIcon name="lock" />{{ selectedAccount ? '使用统一账号库中的 Cookie，过期后可在账号管理中更新。' : '在设置中保存一次 Cookie，购票和监控即可共用。' }}</div>
                        <label class="check-label proxy-row"><input type="checkbox" v-model="form.useProxy" :disabled="locked || loading" />使用全局代理 <router-link to="/settings">设置<UiIcon name="chevron" /></router-link></label>
                        <button class="button primary full" :disabled="locked || loading || !runtime.ready" type="submit"><UiIcon :name="loading ? 'refresh' : 'search'" :class="{ spinning: loading }" />{{ loading ? '正在加载活动…' : project ? '重新加载活动' : '加载活动' }}</button>
                    </form>
                    <button v-if="!isBili" class="connection-help text-button" @click="openExternal(meta.home)">打开大麦官网<UiIcon name="launch" /></button>
                </section>
                <section v-if="!isBili" class="panel clock-panel"><div class="compact-heading"><UiIcon name="clock" /><h3>时间校准</h3><span class="tiny-label" v-if="runtime.clock">已同步</span></div><p>同步公共服务器时间，让预约更准确。</p><div class="clock-number">{{ form.offsetMs > 0 ? '+' : '' }}{{ form.offsetMs || 0 }}<span>ms</span></div><small v-if="runtime.clock">{{ runtime.clock.source }} · 往返 {{ runtime.clock.roundTripMs }} ms<br />估计误差 ±{{ runtime.clock.uncertaintyMs }} ms</small><small v-else>尚未同步 · 当前使用本机时间</small><button class="button secondary full small" :disabled="runtime.syncing || locked" @click="calibrate"><UiIcon name="refresh" :class="{ spinning: runtime.syncing }" />{{ runtime.syncing ? '正在校准…' : '同步服务器时间' }}</button><p v-if="clockError" class="inline-error" role="alert">{{ clockError }}</p></section>
                <TaskStatus v-if="!monitoring && task" :task="task" />
            </div>
            <div v-if="!isBili || project" v-show="!isBili || !connectionEditing" class="event-column">
                <section v-if="!project" class="panel empty-project" :aria-busy="loading">
                    <div class="empty-ticket-scene" aria-hidden="true"><div class="scene-orbit"></div><div class="decor-star star-one">✦</div><div class="decor-star star-two">✧</div><div class="ticket-illustration"><div class="ticket-illustration-top"><span>ADMIT ONE</span><UiIcon name="ticket" /></div><div class="ticket-illustration-title">下一场<br />值得期待。</div><div class="ticket-illustration-bottom"><span>LET’S GO LIVE</span><div class="barcode"></div></div></div><div class="scene-tag"><span class="live-dot"></span>READY FOR YOUR NEXT SHOW</div></div>
                    <h2>{{ loading ? '正在寻找你的下一场期待…' : '你的下一场，在哪里？' }}</h2><p>{{ isBili ? '搜索并选择活动，或填写左侧的活动链接与账号信息，' : '填写左侧的活动链接与账号信息，' }}<br />加载活动后，即可选择场次、票档与观演人。</p><div class="empty-features"><span><UiIcon name="calendar" />定时预约</span><span><UiIcon name="user" />实名观演</span><span><UiIcon name="activity" />实时进度</span></div>
                </section>
                <template v-if="project">
                    <section class="panel event-panel"><div v-if="!isBili" class="event-summary"><img v-if="project.image" :src="project.image" class="event-cover" alt="活动海报" referrerpolicy="no-referrer" @error="project.image = ''" /><div v-else class="event-cover placeholder-cover"><UiIcon name="ticket" /></div><div class="event-summary-text"><span class="pill">{{ meta.name }}{{ isBili ? ' 会员购' : ' 官方活动' }}</span><h2>{{ project.name }}</h2><p v-if="project.venue"><UiIcon name="location" />{{ project.venue }}</p><small>项目编号 {{ project.id }}</small></div></div>
                        <div v-if="!isBili" class="section-rule"></div>
                        <div class="section-heading"><span class="section-icon"><UiIcon name="calendar" /></span><div><h2>选择场次与票档</h2><p v-if="!isBili">选择你想去的那一场</p></div><span class="tiny-label">02</span></div>
                        <div v-if="project.dates.length" class="field"><label class="field-label" :for="`${platform}-date`">活动日期</label><select :id="`${platform}-date`" class="text-input" v-model="form.date" @change="changeDate" :disabled="locked || ticketLoading"><option value="" disabled>选择日期</option><option v-for="date in project.dates" :value="date" :key="date">{{ date }}</option></select></div>
                        <div class="field-label">场次</div><div class="option-grid"><button v-for="item in project.screens" :key="item.id" class="option-card" :class="{ chosen: screenId === item.id, unavailable: item.disabled }" :aria-pressed="screenId === item.id" :disabled="locked || ticketLoading || (!monitoring && item.disabled)" :title="item.disabledReason" @click="selectScreen(item)"><span>{{ item.name }}</span><small v-if="item.status">{{ item.status }}</small><UiIcon v-if="screenId === item.id" name="check" /></button></div>
                        <p v-if="!project.screens.length" class="field-hint">{{ project.dates.length ? '请选择活动日期以加载场次' : '暂无可选场次，请稍后重新加载' }}</p>
                        <template v-if="screen"><div class="field-label space-top">票档 <span v-if="ticketLoading" class="muted-text">加载中…</span></div><div class="option-grid ticket-options"><button v-for="item in screen.tickets" :key="item.id" :aria-pressed="ticketId === item.id" class="option-card" :class="{ chosen: ticketId === item.id, unavailable: screen.disabled || item.disabled }" :disabled="locked || ticketLoading || (!monitoring && (screen.disabled || item.disabled))" :title="screen.disabledReason || item.disabledReason" @click="selectTicket(item)"><span>{{ item.name }}</span><strong>¥ {{ money(item.price) }}</strong><small v-if="item.status">{{ item.status }}</small><UiIcon v-if="ticketId === item.id" name="check" /></button></div><p v-if="!ticketLoading && !screen.tickets.length" class="field-hint">暂无票档，可以重新选择场次刷新。</p><p v-if="screen.deliveryFee" class="field-hint">以上价格已包含配送费 ¥{{ money(screen.deliveryFee) }} / 张。</p><p v-if="screen.requiresSeat" class="inline-error">该场次需要选座，请前往官方页面购票。</p></template>
                    </section>
                    <MonitorControls v-if="monitoring" v-show="!isBili || ticket" :platform="platform" :locked="locked" :ready="!!ticket && !ticketLoading" @start="startMonitor" />
                    <section v-else v-show="!isBili || ticket" class="panel purchase-panel"><div class="section-heading"><span class="section-icon"><UiIcon name="user" /></span><div><h2>确认购票信息</h2><p v-if="!isBili">准备好，就出发</p></div><span class="tiny-label">03</span></div>
                        <div class="purchase-row"><label class="field-label" :for="`${platform}-count`">购买张数</label><div class="quantity-control"><button aria-label="减少张数" :disabled="locked || form.count <= 1" @click="form.count--">−</button><input :id="`${platform}-count`" type="number" min="1" :max="maxCount" v-model.number="form.count" :disabled="locked" /><button aria-label="增加张数" :disabled="locked || form.count >= maxCount" @click="form.count++">＋</button></div><small class="muted-text">最多 {{ maxCount }} 张</small></div>
                        <div class="label-row"><span class="field-label">观演人 <span class="muted-text">已选 {{ form.buyers.length }} / {{ form.count }} 位</span></span><button class="text-button" :disabled="buyerLoading || locked" @click="loadBuyers()"><UiIcon name="refresh" :class="{ spinning: buyerLoading }" />{{ buyerLoading ? '加载中' : '刷新' }}</button></div>
                        <div class="buyer-grid"><label v-for="item in buyers" :key="item.key" class="buyer-card" :class="{ chosen: form.buyers.includes(item.key) }"><input type="checkbox" :value="item.key" v-model="form.buyers" :disabled="locked || (!form.buyers.includes(item.key) && form.buyers.length >= form.count)" /><div><strong>{{ item.name }}</strong><small>{{ item.identity }}</small></div></label></div><p v-if="buyerError" class="inline-error" role="alert">{{ buyerError }}</p>
                        <div v-if="isBili" class="two-fields space-top"><div class="field"><label class="field-label" for="bili-contact">联系人</label><input id="bili-contact" class="text-input" v-model="form.buyer" :disabled="locked" placeholder="联系人姓名" /></div><div class="field"><label class="field-label" for="bili-tel">联系电话</label><input id="bili-tel" class="text-input" v-model="form.tel" :disabled="locked" type="tel" placeholder="接收订单通知的手机号" /></div></div>
                        <div v-if="needsDelivery" class="field space-top"><div class="label-row"><label class="field-label" for="bili-address">收货地址</label><button class="text-button" :disabled="addressLoading || locked" @click="loadAddresses()">刷新地址</button></div><select id="bili-address" v-model="form.address" :disabled="locked || addressLoading" class="text-input"><option value="">请选择收货地址</option><option v-for="address in addresses" :key="address.id" :value="String(address.id)">{{ address.name }} · {{ address.prov }}{{ address.city }}{{ address.area }}{{ address.addr }}</option></select><p v-if="addressError" class="inline-error">{{ addressError }}</p><small v-if="!addresses.length" class="field-hint">请先在会员购添加收货地址，再刷新列表。</small></div>
                        <div class="section-rule"></div>
                        <div class="label-row"><div class="compact-heading"><UiIcon name="clock" /><h3>预约与重试</h3></div><label class="check-label"><input type="checkbox" v-model="form.scheduled" :disabled="locked" />定时开始</label></div>
                        <div v-if="form.scheduled" class="field space-top"><label class="field-label" :for="`${platform}-start`">开始时间（北京时间）</label><input :id="`${platform}-start`" class="text-input" type="datetime-local" step="1" v-model="form.startAt" :disabled="locked" /><small class="field-hint">开售时间自动带入，也可以手动调整。</small></div>
                        <div class="three-fields space-top"><div class="field"><label class="field-label" :for="`${platform}-attempts`">最多尝试 / 次</label><input :id="`${platform}-attempts`" class="text-input" type="number" v-model.number="form.maxAttempts" min="1" max="100" :disabled="locked" /></div><div class="field"><label class="field-label" :for="`${platform}-interval`">重试间隔 / ms</label><input :id="`${platform}-interval`" class="text-input" type="number" v-model.number="form.intervalMs" min="300" max="60000" step="100" :disabled="locked" /></div><div class="field"><label class="field-label" :for="`${platform}-offset`">修正时间 / ms</label><input :id="`${platform}-offset`" class="text-input" type="number" v-model.number="form.offsetMs" min="-86400000" max="86400000" :disabled="locked" /></div></div>
                        <p class="field-hint">修正值 = 服务器时间 − 本机时间。任务开始后可切换平台，请保持电脑唤醒。</p>
                        <p class="field-hint">{{ runtime.settings.wechat.enabled ? '已启用微信提醒：订单创建成功后发送待支付通知。新任务使用启动时已保存的通知设置。' : '当前仅在应用内提醒。可在设置中启用微信购票与余票提醒。' }} <router-link to="/settings">通知设置 →</router-link></p>
                        <div class="purchase-footer"><div><small>预计总额</small><strong><span>¥</span> {{ money((ticket?.price || 0) * form.count) }}</strong></div><button class="button primary" :disabled="locked || !ticket || screen?.disabled || ticket.disabled || !runtime.ready || ticketLoading" @click="start"><UiIcon :name="starting ? 'refresh' : 'play'" :class="{ spinning: starting }" />{{ starting ? '正在准备…' : running ? '任务进行中' : form.scheduled ? '创建预约任务' : '开始购票' }}</button></div><small class="field-hint">创建订单后请在官方页面及时支付。</small>
                    </section>
                </template>
            </div>
        </div>
    </div>
</template>

<style scoped>
.bili-workspace .workbench-grid { display: flex; flex-direction: column; align-items: stretch; gap: 18px; }
.bili-workspace .connection-column { display: contents; }
.bili-workspace .connection-panel { order: 0; padding: 24px; }
.bili-workspace .event-column { order: 1; gap: 18px; }
.bili-workspace .connection-column > .task-status { order: 2; }
.bili-workspace .section-heading { margin-bottom: 20px; }
.connection-account-row { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; margin-bottom: 20px; }
.connection-account-row .field-label { margin: 0; }
.connection-account-row .text-input { width: 220px; min-width: 0; max-width: 100%; min-height: 34px; padding-block: 6px; }
.connection-tools { display: flex; align-items: center; flex-wrap: wrap; gap: 18px; padding-top: 18px; margin-top: 18px; border-top: 1px solid var(--border); }
.connection-tools .text-button, .connection-tools .check-label { color: var(--muted); font-size: 10px; }
.connection-tools .official-link { margin-left: auto; }
@media (max-width: 700px) {
    .bili-workspace .connection-panel { padding: 18px; }
    .connection-account-row { gap: 10px; }
    .connection-account-row .text-input { width: 170px; }
    .connection-tools { gap: 12px; }
}
</style>
