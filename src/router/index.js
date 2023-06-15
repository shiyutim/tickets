import { createRouter, createWebHashHistory } from "vue-router";

export const routes = [
    {
        path: "/",
        redirect: "dm",
        meta: {
            hideInMenu: true,
        },
    },
    {
        path: "/dm",
        name: "dm",
        component: () => import("../views/dm.vue"),
        meta: {
            name: "大麦",
        },
    },
    {
        path: "/my",
        name: "my",
        component: () => import("../views/my.vue"),
        meta: {
            name: "猫眼",
        },
    },
];

export default createRouter({
    history: createWebHashHistory(),
    routes,
});
