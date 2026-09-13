<script setup>
import { computed, ref } from "vue";
import TicketWorkspace from "../components/TicketWorkspace.vue";
import TaskStatus from "../components/TaskStatus.vue";
import UiIcon from "../components/common/UiIcon.vue";
import { monitorTasks, activeMonitors, isActive, runtime } from "../services/runtime";
import { platforms, formatTime } from "../services/platforms";

const creating = ref(monitorTasks.value.length === 0);
const platform = ref("dm");
const filter = ref("all");
const activeOnly = ref(false);
const busy = ref(false);
const visible = computed(() => monitorTasks.value.filter(task =>
    (filter.value === "all" || task.platform === filter.value) && (!activeOnly.value || isActive(task))));
function created() { creating.value = false; busy.value = false; filter.value = "all"; activeOnly.value = false; }
</script>

<template>
    <div class="monitor-page">
        <div class="page-heading"><div><div class="eyebrow">TICKET MONITOR</div><h1>余票监控</h1><p>多个活动、多个票档，一起等好消息。</p></div><button class="button primary" :disabled="busy || !runtime.ready" @click="creating = !creating"><UiIcon :name="creating ? 'close' : 'search'" />{{ creating ? '收起新增' : '新增监控' }}</button></div>
        <div class="monitor-summary"><span class="pill">{{ activeMonitors.length }} 个监控运行中</span><span class="muted-text">共 {{ monitorTasks.length }} 个监控 · 每个任务独立定时、独立停止</span><router-link to="/settings" class="text-button">微信通知设置<UiIcon name="chevron" /></router-link></div>

        <div v-show="creating" class="monitor-create">
            <div class="section-heading"><div><h2>新增监控</h2><p>选择平台和票档，再设置监控时间；添加后可继续创建。</p></div></div>
            <div class="segmented-control" aria-label="新增监控平台"><button v-for="(meta, id) in platforms" :key="id" :class="{ selected: platform === id }" :aria-pressed="platform === id" :disabled="busy" @click="platform = id">{{ meta.name }}</button></div>
            <KeepAlive><TicketWorkspace v-if="creating" :key="platform" :platform="platform" monitoring @created="created" @busy="busy = $event" /></KeepAlive>
        </div>

        <div class="monitor-list-heading"><h2>监控列表</h2><label class="check-label"><input type="checkbox" v-model="activeOnly" />仅显示运行中</label></div>
        <div class="segmented-control" aria-label="筛选监控平台"><button v-for="item in [{ id: 'all', name: '全部平台' }, { id: 'dm', name: '大麦' }, { id: 'bilibili', name: 'Bilibili' }]" :key="item.id" :class="{ selected: filter === item.id }" @click="filter = item.id">{{ item.name }}</button></div>
        <div v-if="visible.length" class="task-grid">
            <article v-for="task in visible" :key="task.id" class="panel task-card">
                <div class="task-card-title"><span class="pill">{{ platforms[task.platform]?.name }}</span><small>更新于 {{ formatTime(task.updatedAt) }}</small></div>
                <h3>{{ task.title }}</h3>
                <div class="monitor-details"><span v-if="task.intervalMs">每 {{ task.intervalMs / 1000 }} 秒查询</span><span>{{ task.maxAttempts ? `最多 ${task.maxAttempts} 次` : '不限查询次数' }}</span><span>开始：{{ formatTime(task.startAt) }}</span><span v-if="task.endAt">结束：{{ formatTime(task.endAt) }}</span></div>
                <TaskStatus :task="task" />
            </article>
        </div>
        <section v-else class="panel empty-inline"><span class="empty-icon"><UiIcon name="search" /></span><h2>{{ monitorTasks.length ? '没有符合筛选条件的监控' : '还没有监控任务' }}</h2><p>{{ monitorTasks.length ? '调整平台或运行状态筛选，查看其他监控。' : '添加大麦或会员购的活动和票档，发现可购票时及时提醒。' }}</p><button v-if="!creating" class="button secondary" @click="creating = true">新增监控<UiIcon name="arrow" /></button></section>
    </div>
</template>

<style scoped>
.monitor-summary { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; margin-bottom: 24px; }
.monitor-summary > .text-button { margin-left: auto; }
.monitor-create { margin-bottom: 32px; }
.monitor-list-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 16px; }
.monitor-details { display: flex; flex-wrap: wrap; gap: 6px 16px; color: var(--muted); font-size: 11px; line-height: 1.7; margin-bottom: 16px; }
</style>
