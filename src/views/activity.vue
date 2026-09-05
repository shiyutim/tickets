<script setup>
import { ref, computed } from "vue";
import UiIcon from "../components/common/UiIcon.vue";
import TaskStatus from "../components/TaskStatus.vue";
import { runtime, taskList, activeTasks, desktop, errorText } from "../services/runtime";
import { formatTime, platforms } from "../services/platforms";
import { select, logTableName } from "../sql";
import { Message } from "@arco-design/web-vue";
const filter = ref("all");
const query = ref("");
const date = ref("");
const exporting = ref(false);
const filteredTasks = computed(() => taskList.value.filter(task => filter.value === "all" || task.platform === filter.value));
function matches(log) {
    const text = `${log.title} ${log.msg || ''}`.toLowerCase();
    const day = new Date(Number(log.time) + 8 * 3600_000).toISOString().slice(0, 10);
    return (filter.value === "all" || log.type === filter.value) && (!query.value || text.includes(query.value.toLowerCase())) && (!date.value || day === date.value);
}
const logs = computed(() => runtime.logs.filter(matches));
async function exportLogs() {
    exporting.value = true;
    try {
        const source = desktop && !runtime.storageError ? await select(`SELECT * FROM "${logTableName}" ORDER BY time DESC`) : runtime.logs;
        const data = JSON.stringify(source.filter(matches), null, 2);
        const name = `tickets-${new Date().toISOString().slice(0, 10)}.json`;
        if (desktop) {
            const { save } = await import("@tauri-apps/api/dialog");
            const path = await save({ defaultPath: name, filters: [{ name: "JSON 日志", extensions: ["json"] }] });
            if (!path) return;
            const { writeTextFile } = await import("@tauri-apps/api/fs");
            await writeTextFile(path, data);
        } else {
            const url = URL.createObjectURL(new Blob([data], { type: "application/json" }));
            const link = document.createElement("a"); link.href = url; link.download = name; link.click();
            setTimeout(() => URL.revokeObjectURL(url), 1000);
        }
        Message.success("日志已导出");
    } catch (error) { Message.error(errorText(error)); }
    finally { exporting.value = false; }
}
</script>
<template>
    <div class="activity-page">
        <div class="page-heading"><div><div class="eyebrow">YOUR ACTIVITY</div><h1>任务与记录</h1><p>每一步进度，都清晰可见。</p></div><span class="pill">{{ activeTasks.length }} 个任务运行中</span></div>
        <div class="segmented-control" aria-label="筛选平台"><button v-for="item in [{id:'all',name:'全部平台'},{id:'dm',name:'大麦'},{id:'bilibili',name:'Bilibili'}]" :key="item.id" :class="{ selected: filter === item.id }" @click="filter = item.id">{{ item.name }}</button></div>
        <div class="task-grid" v-if="filteredTasks.length"><article v-for="task in filteredTasks" :key="task.id" class="panel task-card"><div class="task-card-title"><span class="pill">{{ platforms[task.platform]?.name || task.platform }}</span><small>{{ formatTime(task.updatedAt) }}</small></div><h3>{{ task.title }}</h3><TaskStatus :task="task" /></article></div>
        <section v-else class="panel empty-inline"><span class="empty-icon"><UiIcon name="activity" /></span><h2>还没有购票任务</h2><p>在平台页选择活动并开始后，可以在这里查看进度。</p><router-link to="/dm" class="button secondary">去购票工作台<UiIcon name="arrow" /></router-link></section>
        <section class="panel logs-panel"><div class="section-heading"><span class="section-icon"><UiIcon name="activity" /></span><div><h2>操作记录</h2><p>显示最近 500 条 · 导出包含符合筛选条件的全部日志</p></div><button class="button secondary small" :disabled="exporting" @click="exportLogs"><UiIcon name="download" />导出日志</button></div><div class="log-filters"><input aria-label="搜索操作记录" class="text-input" v-model="query" placeholder="搜索操作或提示内容…" /><input aria-label="筛选日志日期（北京时间）" type="date" class="text-input" v-model="date" /><button v-if="query || date" class="text-button" @click="query = ''; date = ''">清除筛选</button></div><div class="log-list" role="log"><div v-for="(entry, index) in logs" :key="`${entry.time}-${index}`" class="log-row"><span class="log-status-dot" :class="entry.status"></span><div><strong>{{ entry.title }}</strong><p v-if="entry.msg">{{ entry.msg }}</p><small>{{ platforms[entry.type]?.name || '系统' }}</small></div><time>{{ formatTime(Number(entry.time)) }}</time></div><p v-if="!logs.length" class="empty-log">暂无符合条件的操作记录</p></div></section>
    </div>
</template>
