<script setup>
import { ref, onMounted, onUnmounted, computed } from "vue";
import { isActive, statusLabels, openExternal, stopTask } from "../services/runtime";
import { formatTime } from "../services/platforms";
import UiIcon from "./common/UiIcon.vue";
const props = defineProps({ task: Object });
const now = ref(Date.now());
let timer;
onMounted(() => { timer = setInterval(() => { now.value = Date.now(); }, 250); });
onUnmounted(() => clearInterval(timer));
const remaining = computed(() => {
    const seconds = Math.max(0, Math.ceil((props.task.startAt - props.task.offsetMs - now.value) / 1000));
    const days = Math.floor(seconds / 86400);
    return `${days ? `${days} 天 ` : ""}${[Math.floor(seconds / 3600) % 24, Math.floor(seconds / 60) % 60, seconds % 60].map(x => String(x).padStart(2, "0")).join(":")}`;
});
const stopping = ref(false);
async function stop() { stopping.value = true; await stopTask(props.task.id); stopping.value = false; }
</script>
<template>
    <section class="task-status" :class="task.status" aria-live="polite">
        <div class="task-status-head"><span class="status-pill" :class="task.status"><span class="live-dot"></span>{{ statusLabels[task.status] }}</span><span class="muted-text">{{ task.mode === 'monitor' ? '查询' : '尝试' }} {{ task.attempt }} / {{ task.maxAttempts || '不限' }}</span></div>
        <div v-if="task.status === 'waiting'" class="task-countdown">{{ remaining }}</div>
        <p>{{ task.message }}</p>
        <small v-if="task.status === 'waiting'">{{ formatTime(task.startAt) }} · 北京时间</small>
        <div class="task-status-actions">
            <button v-if="isActive(task)" class="button danger small" :disabled="stopping" @click="stop"><UiIcon name="pause" />{{ stopping ? '正在停止…' : '停止任务' }}</button>
            <button v-if="task.orderUrl" class="button primary small" @click="openExternal(task.orderUrl)">{{ task.status === 'succeeded' ? '前往支付' : '前往官方页面' }}<UiIcon name="launch" /></button>
            <router-link v-if="['disabled', 'failed', 'unknown'].includes(task.notificationStatus)" class="text-button" to="/settings">微信通知设置 →</router-link>
        </div>
    </section>
</template>
