<script setup>
import { computed } from "vue";
import { Message } from "@arco-design/web-vue";
import UiIcon from "./common/UiIcon.vue";
import { runtime, desktop, refreshSubscription, errorText } from "../services/runtime";
import { proxyNodeName } from "../services/proxies";

const props = defineProps({ modelValue: { type: Object, required: true } });
const emit = defineEmits(["update:modelValue"]);
const form = computed(() => props.modelValue);
const busy = computed(() => Object.values(runtime.subscriptions).some(item => item.loading));
const selected = computed(() => form.value.subscriptions.find(item => item.id === form.value.subscriptionId));
const snapshot = source => {
    const state = runtime.subscriptions[source?.id];
    return state?.url === source?.url ? state : null;
};
const nodes = computed(() => snapshot(selected.value)?.nodes || []);
const savedSource = source => runtime.settings.subscriptions.some(item => item.id === source.id && item.url === source.url);
function update(values) { emit("update:modelValue", { ...form.value, ...values }); }
function add() {
    const source = { id: crypto.randomUUID(), name: `订阅 ${form.value.subscriptions.length + 1}`, url: "" };
    update({ subscriptions: [...form.value.subscriptions, source], subscriptionId: form.value.subscriptionId || source.id });
}
function edit(id, values) {
    update({ subscriptions: form.value.subscriptions.map(source => source.id === id ? { ...source, ...values } : source) });
}
function remove(id) {
    const subscriptions = form.value.subscriptions.filter(source => source.id !== id);
    update({ subscriptions, ...(form.value.subscriptionId === id ? { subscriptionId: subscriptions[0]?.id || "", subscriptionNodeId: "" } : {}) });
}
async function refresh(source) {
    try {
        const result = await refreshSubscription(source);
        if (result) Message.success(`订阅已更新，共 ${result.nodes.length} 个节点`);
    } catch (error) { Message.error(errorText(error)); }
}
</script>

<template>
    <section class="panel settings-panel proxy-settings">
        <div class="section-heading"><span class="section-icon"><UiIcon name="link" /></span><div><h2>代理与订阅</h2><p>在平台页勾选「使用全局代理」后生效</p></div></div>
        <label class="field-label" for="proxy-mode">代理来源</label>
        <select id="proxy-mode" class="text-input" :value="form.proxyMode" :disabled="busy" @change="update({ proxyMode: $event.target.value })"><option value="manual">手动代理地址</option><option value="subscription">订阅节点</option></select>
        <div v-if="form.proxyMode === 'manual'" class="field space-top"><label class="field-label" for="global-proxy">全局代理地址</label><input id="global-proxy" :value="form.proxy" @input="update({ proxy: $event.target.value.trim() })" class="text-input" placeholder="http://127.0.0.1:7890" autocomplete="off" /><small class="field-hint">支持 HTTP、HTTPS、SOCKS5 和 SOCKS5H。</small></div>
        <template v-else>
            <div class="notice space-top">支持 Clash YAML / JSON、代理链接列表和 Base64 订阅中的 HTTP、HTTPS、SOCKS5/H 节点。SS、VMess、Trojan 等节点会跳过。</div>
            <div v-for="source in form.subscriptions" :key="source.id" class="subscription-card">
                <div class="subscription-title"><label class="field-label" :for="`source-name-${source.id}`">订阅名称</label><button class="text-button" :disabled="busy" @click="remove(source.id)">删除</button></div>
                <input :id="`source-name-${source.id}`" :value="source.name" class="text-input" maxlength="60" :disabled="busy" @input="edit(source.id, { name: $event.target.value })" />
                <label class="field-label space-top" :for="`source-url-${source.id}`">订阅链接</label>
                <input :id="`source-url-${source.id}`" :value="source.url" type="password" class="text-input" placeholder="https://example.com/subscription" autocomplete="off" spellcheck="false" :disabled="busy" @input="edit(source.id, { url: $event.target.value.trim() })" />
                <div class="subscription-actions"><button class="button secondary small" :disabled="busy || !desktop || !source.url || !savedSource(source)" @click="refresh(source)"><UiIcon name="refresh" :class="{ spinning: snapshot(source)?.loading }" />{{ snapshot(source)?.loading ? '刷新中…' : '刷新订阅' }}</button><small v-if="!savedSource(source)" class="field-hint">请先保存设置，再刷新订阅</small><small v-else-if="snapshot(source)?.updatedAt" class="field-hint">{{ snapshot(source).nodes.length }} 个节点 · {{ new Date(snapshot(source).updatedAt).toLocaleString() }}</small><small v-else class="field-hint">尚未加载节点</small></div>
                <p v-if="snapshot(source)?.skipped" class="field-hint">已跳过 {{ snapshot(source).skipped }} 个不支持、无效或重复的节点。</p>
                <p v-if="snapshot(source)?.error" class="inline-error" role="alert">{{ snapshot(source).error }}{{ snapshot(source).nodes?.length ? '；仍保留上次成功加载的节点。' : '' }}</p>
            </div>
            <button class="button secondary small space-top" :disabled="busy || form.subscriptions.length >= 20" @click="add">添加订阅</button>
            <template v-if="form.subscriptions.length">
                <div class="section-rule"></div>
                <label class="field-label" for="active-subscription">使用的订阅</label><select id="active-subscription" class="text-input" :disabled="busy" :value="form.subscriptionId" @change="update({ subscriptionId: $event.target.value, subscriptionNodeId: '' })"><option value="" disabled>选择订阅</option><option v-for="source in form.subscriptions" :key="source.id" :value="source.id">{{ source.name || '未命名订阅' }}</option></select>
                <label class="field-label space-top" for="active-node">使用的节点</label><select id="active-node" class="text-input" :disabled="busy" :value="form.subscriptionNodeId" @change="update({ subscriptionNodeId: $event.target.value })"><option value="">自动选择 · 建立连接时轮换</option><option v-if="form.subscriptionNodeId && !nodes.some(node => node.id === form.subscriptionNodeId)" :value="form.subscriptionNodeId" disabled>所选节点尚未加载或已移除</option><option v-for="node in nodes" :key="node.id" :value="node.id" :title="`${node.protocol.toUpperCase()} · ${node.host}:${node.port}`">{{ proxyNodeName(node) }}</option></select>
                <small class="field-hint">可固定使用一个节点。已建立的购票请求链路会继续使用原节点。</small>
            </template>
            <p v-if="!desktop" class="field-hint space-top">浏览器预览可编辑配置，刷新订阅需要在桌面应用中使用。</p>
            <p class="field-hint space-top">保存设置后生效；重启时自动刷新选中的订阅。订阅链接会保存在本机，节点认证信息仅留在内存中。</p>
        </template>
    </section>
</template>

<style scoped>
.proxy-settings { margin-top: 20px; }
.proxy-settings > .field-label { display: block; margin-bottom: 8px; }
.subscription-card { border: 1px solid var(--border); border-radius: 9px; padding: 16px; margin-top: 14px; }
.subscription-title, .subscription-actions { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.subscription-title { margin-bottom: 8px; }
.subscription-card > .field-label { display: block; margin-bottom: 8px; }
.subscription-actions { margin-top: 12px; flex-wrap: wrap; }
.subscription-actions .field-hint { margin: 0; }
.proxy-settings .notice { margin-bottom: 0; }
</style>
