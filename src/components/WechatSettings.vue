<script setup>
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from "vue";
import QRCode from "qrcode";
import { call, desktop, errorText, runtime, saveWechatSettings, syncWechatStatus, testWechatNotification } from "../services/runtime";
import UiIcon from "./common/UiIcon.vue";
const connection = ref({ status: "unbound", accountId: "", target: "", message: "" });
const session = ref(null);
const qrImage = ref("");
const verifyCode = ref("");
const busy = ref("");
const polling = ref(false);
const result = ref("");
const failed = ref(false);
const statusError = ref("");
const statusLabels = { unbound: "尚未绑定", waiting_message: "等待微信消息", ready: "已连接", reconnecting: "正在重新连接", expired: "登录已失效", error: "连接需要处理" };
const pending = computed(() => ["wait", "scaned", "need_verifycode"].includes(session.value?.status));
const canTest = computed(() => desktop && connection.value.status === "ready" && !session.value && !busy.value);
let active = false;
let generation = 0;
let checkingStatus = false;
let timer;

function toggle(event) {
    try {
        syncWechatStatus(connection.value);
        saveWechatSettings(event.target.checked);
        feedback(runtime.settings.wechat.enabled ? "微信购票与余票提醒已启用并保存，新任务将发送通知。" : "微信购票与余票提醒已关闭并保存。");
    } catch (error) { feedback(errorText(error), true); }
    finally { event.target.checked = runtime.settings.wechat.enabled; }
}
function feedback(message = "", error = false) { result.value = message; failed.value = error; }
function applyStatus(value) {
    connection.value = value;
    syncWechatStatus(value);
    statusError.value = "";
}
function schedule() {
    clearTimeout(timer);
    if (active && desktop) timer = setTimeout(pump, 2000);
}
async function refreshStatus() {
    if (checkingStatus) return;
    checkingStatus = true;
    const current = generation;
    try {
        const value = await call("get_wechat_status");
        if (active && current === generation) applyStatus(value);
    } catch (error) {
        if (active && current === generation) statusError.value = errorText(error);
    } finally {
        checkingStatus = false;
    }
}
async function pollLogin(code = null) {
    if (!pending.value || polling.value) return;
    const current = generation;
    const sessionId = session.value.sessionId;
    polling.value = true;
    try {
        const value = await call("poll_wechat_login", { sessionId, verifyCode: code });
        if (!active || current !== generation) return;
        const image = value.qrContent && value.qrContent !== session.value.qrContent
            ? await QRCode.toDataURL(value.qrContent, { width: 240, margin: 4 }) : qrImage.value;
        if (!active || current !== generation) return;
        session.value = value;
        qrImage.value = image;
        verifyCode.value = "";
        feedback();
        if (value.status === "confirmed") {
            session.value = null; qrImage.value = "";
            feedback("扫码成功，绑定已保存。请在微信中向 Bot 发一条消息以启用通知会话。");
            await refreshStatus();
        } else if (["expired", "verify_code_blocked"].includes(value.status)) {
            qrImage.value = "";
        }
    } catch (error) {
        if (active && current === generation) {
            session.value = { ...session.value, status: "error" };
            feedback(errorText(error), true);
        }
    } finally {
        polling.value = false;
    }
}
async function pump() {
    if (!active || !desktop) return;
    if (busy.value || polling.value) { schedule(); return; }
    if (pending.value) {
        if (session.value.expiresAt <= Date.now()) {
            const sessionId = session.value.sessionId;
            session.value = { ...session.value, status: "expired", message: "二维码已过期，请重新获取。" };
            qrImage.value = "";
            await call("cancel_wechat_login", { sessionId }).catch(() => {});
        } else if (session.value.status !== "need_verifycode") await pollLogin();
    } else if (!session.value) await refreshStatus();
    schedule();
}
async function cancelSession() {
    const sessionId = session.value?.sessionId;
    generation++; clearTimeout(timer);
    session.value = null; qrImage.value = ""; verifyCode.value = "";
    if (sessionId && desktop) await call("cancel_wechat_login", { sessionId });
}
async function startLogin() {
    if (!desktop || busy.value) return;
    busy.value = "start"; feedback();
    try {
        await cancelSession();
        const current = generation;
        const value = await call("start_wechat_login");
        if (!active || current !== generation) {
            await call("cancel_wechat_login", { sessionId: value.sessionId }).catch(() => {});
            return;
        }
        session.value = value;
        const image = await QRCode.toDataURL(value.qrContent, { width: 240, margin: 4 });
        if (active && current === generation) qrImage.value = image;
    } catch (error) {
        if (active) {
            if (session.value) session.value = { ...session.value, status: "error" };
            feedback(errorText(error), true);
        }
    } finally { busy.value = ""; schedule(); }
}
async function cancelLogin() {
    busy.value = "cancel";
    try { await cancelSession(); feedback("本次扫码已取消。"); await refreshStatus(); }
    catch (error) { feedback(errorText(error), true); }
    finally { busy.value = ""; schedule(); }
}
async function submitCode() {
    if (busy.value || polling.value || !verifyCode.value.trim()) return;
    clearTimeout(timer);
    busy.value = "verify";
    try { await pollLogin(verifyCode.value.trim()); }
    finally { busy.value = ""; schedule(); }
}
async function disconnect() {
    if (busy.value) return;
    busy.value = "disconnect"; feedback();
    try {
        await cancelSession();
        applyStatus(await call("disconnect_wechat"));
        feedback("已解除此设备的微信绑定，自动提醒已关闭并保存。");
    } catch (error) { feedback(errorText(error), true); }
    finally { busy.value = ""; schedule(); }
}
async function test() {
    if (!canTest.value) return;
    busy.value = "test"; feedback();
    try {
        feedback(await testWechatNotification(connection.value));
    } catch (error) { feedback(errorText(error), true); await refreshStatus(); }
    finally { busy.value = ""; schedule(); }
}
function activate() { if (active) return; active = true; if (desktop) pump(); }
function deactivate() {
    if (!active) return;
    active = false;
    cancelSession().catch(() => {});
}
onMounted(activate);
onActivated(activate);
onDeactivated(deactivate);
onBeforeUnmount(deactivate);
</script>
<template>
    <section class="panel settings-panel wechat-settings">
        <div class="section-heading"><span class="section-icon"><UiIcon name="link" /></span><div><h2>微信 ClawBot 通知</h2><p>扫码绑定微信，大麦与会员购创建订单成功或发现余票时发送提醒</p></div></div>
        <label class="setting-toggle"><span><strong>启用微信购票与余票提醒</strong><small>开关修改后立即保存，对新任务生效，无需点击页面上方的“保存设置”</small></span><input type="checkbox" :checked="runtime.settings.wechat.enabled" :disabled="!desktop || !runtime.ready || Boolean(busy) || Boolean(session)" @change="toggle" /></label>
        <p class="field-hint">{{ runtime.settings.wechat.enabled ? '自动提醒已启用。' : '自动提醒未启用；绑定或发送测试通知后，仍需打开上方开关。' }}</p>
        <p class="field-hint">购票提醒表示订单创建成功、等待支付，请在官方页面及时支付。通知失败不影响订单，也不会重新下单。</p>
        <p v-if="!desktop" class="field-hint">扫码绑定和发送通知需要在桌面应用中使用。</p>
        <div class="wechat-connection" aria-live="polite">
            <div class="wechat-status"><span class="live-dot" :class="{ muted: connection.status !== 'ready' }"></span><strong>{{ statusLabels[connection.status] || '正在检查连接' }}</strong></div>
            <p class="field-hint">{{ connection.message || '在此设备绑定微信后即可接收购票与余票提醒。' }}</p>
            <p v-if="connection.status === 'waiting_message'" class="wechat-next-step">请打开微信，向刚绑定的 Bot 发一条消息。收到后会自动显示“已连接”。</p>
            <p v-if="connection.target" class="field-hint wechat-recipient">当前接收会话：{{ connection.target }}</p>
            <p v-if="statusError" class="inline-error" role="alert">{{ statusError }} 将自动重试，也可重新扫码绑定。</p>
        </div>
        <div v-if="session" class="wechat-login">
            <img v-if="qrImage && pending" :src="qrImage" alt="微信 ClawBot 绑定二维码" width="240" height="240" />
            <p class="wechat-login-message" role="status">{{ session.message || (session.status === 'scaned' ? '已扫码，请在微信中确认登录。' : session.status === 'need_verifycode' ? '请填写微信要求的验证码。' : session.status === 'expired' ? '二维码已过期，请重新获取。' : session.status === 'verify_code_blocked' ? '验证码尝试受限，请稍后重新扫码。' : session.status === 'error' ? '扫码暂时中断，请重新获取二维码。' : '请使用微信扫描二维码，并确认绑定。') }}</p>
            <form v-if="session.status === 'need_verifycode'" class="wechat-verify" @submit.prevent="submitCode">
                <label class="field-label" for="wechat-code">微信验证码</label>
                <div class="inline-form"><input id="wechat-code" v-model="verifyCode" class="text-input" inputmode="numeric" pattern="[0-9]{1,64}" autocomplete="one-time-code" maxlength="64" :disabled="busy === 'verify'" /><button class="button secondary small" type="submit" :disabled="Boolean(busy) || polling || !verifyCode.trim()">{{ busy === 'verify' ? '验证中…' : '提交验证码' }}</button></div>
            </form>
            <div class="button-row">
                <button v-if="!pending" class="button secondary small" :disabled="Boolean(busy)" @click="startLogin">重新获取二维码</button>
                <button class="text-button" :disabled="Boolean(busy)" @click="cancelLogin">取消扫码</button>
            </div>
        </div>
        <div class="button-row">
            <button v-if="!session" class="button secondary small" :disabled="!desktop || Boolean(busy)" @click="startLogin">{{ busy === 'start' ? '正在获取二维码…' : connection.accountId ? '重新扫码绑定' : '扫码绑定微信' }}</button>
            <button class="button secondary small" :disabled="!canTest" @click="test">{{ busy === 'test' ? '正在发送…' : '发送测试通知' }}</button>
            <button v-if="connection.accountId || ['expired', 'error'].includes(connection.status)" class="text-button" :disabled="!desktop || Boolean(busy)" @click="disconnect">{{ busy === 'disconnect' ? '正在解绑…' : '解除绑定' }}</button>
        </div>
        <p v-if="result" :class="failed ? 'inline-error' : 'field-hint'" role="status">{{ result }}</p>
        <p class="field-hint">微信登录凭证以明文保存在此设备的应用数据目录。关闭应用后将停止提醒；长时间没有对话后如发送失败，请向 Bot 再发一条消息刷新会话。购票与监控任务使用启动时已保存的提醒开关和接收人，更改设置只对新任务生效。</p>
    </section>
</template>
<style scoped>
.wechat-settings { margin-top: 22px; }
.wechat-connection { border: 1px solid var(--border); border-radius: 8px; background: var(--surface-subtle); padding: 14px; margin-top: 16px; }
.wechat-status { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.wechat-next-step { margin-top: 10px; font-size: 12px; line-height: 1.8; color: var(--accent); }
.wechat-recipient { overflow-wrap: anywhere; }
.wechat-login { margin-top: 18px; padding: 16px; border: 1px solid var(--border); border-radius: 8px; }
.wechat-login img { display: block; max-width: 100%; height: auto; margin: 0 auto; background: #fff; border-radius: 6px; }
.wechat-login-message { font-size: 12px; line-height: 1.8; margin-top: 12px; }
.wechat-verify { margin-top: 14px; }
.wechat-verify .inline-form { margin-top: 6px; }
.wechat-verify input { min-width: 0; }
</style>
