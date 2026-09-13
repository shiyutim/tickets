import { createRouter, createWebHashHistory } from "vue-router";
export const routes = [
    { path: "/", redirect: "/dm" },
    { path: "/dm", name: "dm", component: () => import("../views/dm.vue"), meta: { title: "大麦" } },
    { path: "/bilibili", name: "bilibili", component: () => import("../views/bilibili.vue"), meta: { title: "Bilibili 会员购" } },
    { path: "/monitor", name: "monitor", component: () => import("../views/monitor.vue"), meta: { title: "余票监控" } },
    { path: "/activity", name: "activity", component: () => import("../views/activity.vue"), meta: { title: "任务与记录" } },
    { path: "/settings", name: "settings", component: () => import("../views/settings.vue"), meta: { title: "设置与帮助" } },
    { path: "/:pathMatch(.*)*", redirect: "/dm" },
];
export default createRouter({ history: createWebHashHistory(), routes });
