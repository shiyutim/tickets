<script setup>
import { onMounted } from "vue";
import { useRoute } from "vue-router";
import { Message } from "@arco-design/web-vue";
import UiIcon from "./components/common/UiIcon.vue";
import { activeTasks, activeMonitors, runtime, desktop, initializeRuntime, errorText, openExternal } from "./services/runtime";
const route = useRoute();
const version = appVersion;
const navigation = [
    { path: "/dm", label: "大麦", detail: "现场演出", icon: "ticket", platform: "dm" },
    { path: "/bilibili", label: "Bilibili", detail: "会员购", icon: "bilibili", platform: "bilibili" },
];
onMounted(() => initializeRuntime().catch(error => Message.error(errorText(error))));
</script>

<template>
    <div class="app-shell">
        <aside class="sidebar">
            <router-link to="/dm" class="brand" aria-label="Tickets 首页">
                <span class="brand-icon"><UiIcon name="ticket" /></span>
                <span>tickets<span class="brand-dot">.</span><small>每一场期待，都值得抵达</small></span>
            </router-link>
            <div class="nav-label">购票工作台</div>
            <nav class="platform-nav" aria-label="购票平台">
                <router-link v-for="item in navigation" :key="item.path" :to="item.path" class="nav-item" :class="{ selected: route.path === item.path, pink: item.platform === 'bilibili' }">
                    <UiIcon :name="item.icon" />
                    <span>{{ item.label }}<small>{{ item.detail }}</small></span>
                    <span v-if="activeTasks.some(task => task.platform === item.platform && task.mode !== 'monitor')" class="live-dot" aria-label="任务运行中"></span>
                    <UiIcon v-else name="chevron" class="nav-chevron" />
                </router-link>
            </nav>
            <div class="nav-divider"></div>
            <router-link to="/monitor" class="nav-item" :class="{ selected: route.path === '/monitor' }"><UiIcon name="search" /><span>余票监控</span><span class="nav-count" v-if="activeMonitors.length">{{ activeMonitors.length }}</span></router-link>
            <router-link to="/activity" class="nav-item" :class="{ selected: route.path === '/activity' }"><UiIcon name="activity" /><span>任务与记录</span><span class="nav-count" v-if="activeTasks.length">{{ activeTasks.length }}</span></router-link>
            <div class="sidebar-bottom">
                <div class="sidebar-note"><span class="live-dot" :class="{ muted: !activeTasks.length }"></span>{{ activeTasks.length ? `${activeTasks.length} 个任务运行中` : "准备好下一场相遇" }}<small>保持应用运行，等待好消息。</small></div>
                <router-link to="/settings" class="nav-item" :class="{ selected: route.path === '/settings' }"><UiIcon name="settings" /><span>设置与帮助</span></router-link>
                <button class="sidebar-footer" @click="openExternal('https://github.com/shiyutim/tickets')"><UiIcon name="github" /><span>开源共建</span><span>v{{ version }}</span></button>
            </div>
        </aside>
        <main class="main-shell">
            <header class="topbar"><span>工作空间 <UiIcon name="chevron" /> <strong>{{ route.meta.title }}</strong></span><span class="local-badge"><span class="live-dot"></span>{{ desktop ? '本地运行' : '界面预览' }}</span></header>
            <div class="main-scroll">
                <div v-if="!desktop" class="notice preview-notice"><UiIcon name="info" />当前为浏览器预览。连接账号和购票功能请通过 npm run tauri dev 启动桌面应用。</div>
                <div v-if="runtime.storageError" class="notice warning">{{ runtime.storageError }}</div>
                <router-view v-slot="{ Component }"><keep-alive><component :is="Component" :key="route.name" /></keep-alive></router-view>
                <footer class="page-footer">Tickets · 让期待更从容<span>开源学习项目 · 请合理使用</span></footer>
            </div>
        </main>
    </div>
</template>
