<script setup>
import { reactive, ref, watch } from "vue";
import { runtime, readLocal, saveLocal, errorText } from "../services/runtime";
import { monitorOptions, restoreMonitorForm } from "../services/monitoring";
import UiIcon from "./common/UiIcon.vue";
const props = defineProps({ platform: String, locked: Boolean, ready: Boolean });
const emit = defineEmits(["start"]);
const key = `tickets.monitor.${props.platform}`;
const form = reactive(restoreMonitorForm(readLocal(key, {}), runtime.clock));
const error = ref("");
watch(form, value => saveLocal(key, value), { deep: true });
watch(() => runtime.clock, value => { if (value && !props.locked) form.offsetMs = value.offsetMs; });
function start() {
    error.value = "";
    try { emit("start", monitorOptions(form, runtime.settings.wechat)); }
    catch (value) { error.value = errorText(value); }
}
</script>
<template>
    <section class="panel purchase-panel">
        <div class="section-heading"><span class="section-icon"><UiIcon name="search" /></span><div><h2>余票监控</h2><p>首次发现所选票档可购时提醒并结束监控</p></div></div>
        <div class="three-fields">
            <div class="field"><label class="field-label" :for="`${platform}-monitor-interval`">查询间隔 / 秒</label><input :id="`${platform}-monitor-interval`" class="text-input" type="number" min="5" max="3600" step="1" v-model.number="form.intervalSeconds" :disabled="locked" /></div>
            <div class="field"><label class="field-label" :for="`${platform}-monitor-checks`">最多查询 / 次</label><input :id="`${platform}-monitor-checks`" class="text-input" type="number" min="0" max="100000" v-model.number="form.maxChecks" :disabled="locked" /><small class="field-hint">0 表示持续查询，直到有票或手动停止。</small></div>
            <div class="field"><label class="field-label" :for="`${platform}-monitor-offset`">修正时间 / ms</label><input :id="`${platform}-monitor-offset`" class="text-input" type="number" min="-86400000" max="86400000" v-model.number="form.offsetMs" :disabled="locked" /></div>
        </div>
        <div class="two-fields space-top">
            <div class="field"><label class="check-label"><input type="checkbox" v-model="form.scheduled" :disabled="locked" />预约开始（北京时间）</label><input v-if="form.scheduled" aria-label="监控开始时间（北京时间）" class="text-input space-top" type="datetime-local" step="1" v-model="form.startAt" :disabled="locked" /></div>
            <div class="field"><label class="check-label"><input type="checkbox" v-model="form.hasEnd" :disabled="locked" />指定结束时间（北京时间）</label><input v-if="form.hasEnd" aria-label="监控结束时间（北京时间）" class="text-input space-top" type="datetime-local" step="1" v-model="form.endAt" :disabled="locked" /></div>
        </div>
        <div class="section-rule"></div>
        <p class="field-hint">{{ runtime.settings.wechat.enabled ? '已启用微信 ClawBot 通知。启动时使用当前已保存的通知设置。' : '当前仅在应用内提醒。如需微信提醒，请先在设置中启用 ClawBot 通知。' }} <router-link to="/settings">通知设置 →</router-link></p>
        <p class="field-hint">无需填写观演人。可监控售罄、未开售或需要选座的票档；有票后请前往官方页面购票。保持应用运行和电脑唤醒，重启后需重新启动监控。</p>
        <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
        <div class="purchase-footer"><span class="muted-text">{{ form.intervalSeconds }} 秒一次 · 首次有票即提醒</span><button class="button primary" :disabled="locked || !ready || !runtime.ready" @click="start"><UiIcon name="play" />{{ locked ? '任务进行中' : form.scheduled ? '预约余票监控' : '开始余票监控' }}</button></div>
    </section>
</template>
