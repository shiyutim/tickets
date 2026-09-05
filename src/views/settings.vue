<script setup>
import { reactive, ref, watch } from "vue";
import UiIcon from "../components/common/UiIcon.vue";
import { runtime, saveSettings, openExternal, errorText } from "../services/runtime";
import { Message } from "@arco-design/web-vue";
const copySettings = () => JSON.parse(JSON.stringify(runtime.settings));
const form = reactive(copySettings());
const dirty = ref(false);
const saving = ref(false);
const checking = ref(false);
const updateText = ref("");
const label = ref("");
const version = appVersion;
watch(() => runtime.settings, () => { if (!dirty.value) Object.assign(form, copySettings()); }, { deep: true });
async function save() {
    saving.value = true;
    try { await saveSettings(JSON.parse(JSON.stringify(form))); dirty.value = false; Message.success("设置已保存"); }
    catch (error) { Message.error(errorText(error)); }
    finally { saving.value = false; }
}
function createId() {
    if (!label.value.trim()) { Message.warning("请先填写运行标识的备注"); return; }
    const id = `app_${crypto.randomUUID().replaceAll('-', '')}`;
    form.appid_list.push({ id, desc: label.value.trim() }); form.appid = id; label.value = ""; dirty.value = true;
}
async function checkUpdate() {
    checking.value = true; updateText.value = "";
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 8000);
    try {
        const response = await fetch("https://api.github.com/repos/shiyutim/tickets/releases/latest", { signal: controller.signal });
        if (!response.ok) throw new Error("暂时无法获取更新信息");
        const release = await response.json();
        const latest = String(release.tag_name || "").replace(/^v/, "");
        const left = latest.split('.').map(Number), right = String(version).split('.').map(Number);
        const firstDifference = left.map((part, index) => part - (right[index] || 0)).find(value => value !== 0) || 0;
        updateText.value = firstDifference > 0 ? `发现新版本 v${latest}，可前往发布页下载。` : "当前已是最新版本。";
    } catch (error) { updateText.value = errorText(error); }
    finally { clearTimeout(timer); checking.value = false; }
}
</script>
<template>
    <div class="settings-page"><div class="page-heading"><div><div class="eyebrow">MAKE IT YOURS</div><h1>设置与帮助</h1><p>让工作台按照你的习惯运行。</p></div><button class="button primary" :disabled="saving || !runtime.ready" @click="save">{{ saving ? '正在保存…' : '保存设置' }}<UiIcon name="check" /></button></div>
        <div class="settings-grid"><div>
            <section class="panel settings-panel" @input="dirty = true" @change="dirty = true"><div class="section-heading"><span class="section-icon"><UiIcon name="settings" /></span><div><h2>通用设置</h2><p>应用于两个购票平台</p></div></div><label class="setting-toggle"><span><strong>自动校准时间</strong><small>启动应用及创建任务前自动同步公共服务器时间</small></span><input type="checkbox" v-model="form.autoSync" /></label><label class="setting-toggle"><span><strong>订单成功提示音</strong><small>创建订单成功时播放声音提醒</small></span><input type="checkbox" v-model="form.sound" /></label><div class="section-rule"></div><div class="field"><label class="field-label" for="global-proxy">全局代理地址</label><input id="global-proxy" v-model.trim="form.proxy" class="text-input" placeholder="http://127.0.0.1:7890" autocomplete="off" /><small class="field-hint">支持 HTTP、HTTPS、SOCKS5；在各平台页勾选后生效。</small></div><div class="section-rule"></div><div class="field"><label class="field-label" for="app-id">运行标识</label><select id="app-id" v-model="form.appid" class="text-input"><option value="">默认 · 共用日志</option><option v-for="item in form.appid_list" :key="item.id" :value="item.id">{{ item.desc || item.id }}</option></select><small class="field-hint">为不同使用场景分别保存操作日志。</small></div><div class="inline-form"><input aria-label="运行标识备注" v-model="label" class="text-input" placeholder="例如：日常演出" maxlength="60" /><button class="button secondary" @click="createId">创建标识</button></div></section>
            <section class="panel about-panel"><div class="compact-heading"><span class="brand-icon"><UiIcon name="ticket" /></span><div><h2>Tickets <span class="muted-text">v{{ version }}</span></h2><p>一个开源的多平台购票工作台</p></div></div><div class="button-row"><button class="button secondary small" :disabled="checking" @click="checkUpdate">{{ checking ? '检查中…' : '检查更新' }}</button><button class="button secondary small" @click="openExternal('https://github.com/shiyutim/tickets/releases')">下载与发布<UiIcon name="launch" /></button><button class="text-button" @click="openExternal('https://github.com/shiyutim/tickets')">源代码<UiIcon name="github" /></button></div><p v-if="updateText" class="field-hint" role="status">{{ updateText }}</p></section>
        </div><section class="panel help-panel"><div class="section-heading"><span class="section-icon"><UiIcon name="info" /></span><div><h2>开始之前</h2><p>常见问题与使用说明</p></div></div><details open><summary>如何获取 Cookie？</summary><p>在浏览器登录对应平台官网，打开活动页面。按 F12 打开开发者工具，在 Network 中选择该平台的接口请求，复制 Request Headers 中的完整 Cookie。大麦请使用 H5 页面。</p></details><details><summary>切换平台会停止任务吗？</summary><p>不会。每个平台的表单与任务独立保存，切换左侧标签即可查看。关闭应用、电脑休眠或断网会影响任务；重启后不会自动恢复下单。</p></details><details><summary>修正时间应该怎么填写？</summary><p>点击“同步服务器时间”即可自动填写。修正值为服务器时间减本机时间；正值表示本机较慢。校时失败会保留当前值，预约输入统一使用北京时间。</p></details><details><summary>提示需要验证或已有订单？</summary><p>在官方页面完成验证、检查登录状态或处理已有订单，再回到应用重新加载。遇到下单超时，请先查看官方订单页确认结果。</p></details><details><summary>支持选座和自动支付吗？</summary><p>当前支持无需选座的票档。订单创建后，点击“前往支付”，在官方页面完成支付。</p></details><details><summary>个人信息保存在哪里？</summary><p>表单与设置保存在当前设备。仅在勾选“记住 Cookie”后保存登录凭证，取消勾选会删除该平台的已保存 Cookie。观演人的完整证件信息只在当前运行中使用，不写入操作日志。</p></details><div class="help-footnote">本项目用于学习与交流，请遵守平台规则，勿用于商业代抢。</div></section></div>
    </div>
</template>
