<script setup>
import { onMounted, ref, reactive } from "vue";
import Header from "./components/common/Header.vue";
import { routes } from "./router";
import {
    initLogTable,
    insert,
    selectAll,
    settingTableName,
    changeLogTableName,
    initSettingTable,
    execute,
    update,
} from "./sql";
import { IconSettings } from "@arco-design/web-vue/es/icon";
import { createAppId } from "../utils/common";
import { Message } from "@arco-design/web-vue";
import Log from "../utils/common/log";
import { invoke } from "@tauri-apps/api";
import Update from "./components/common/Update.vue";
import Tip from "./components/common/Tip.vue";

const log = new Log();

onMounted(async () => {
    // 初始化表名（根据appid，如果不存在，则为默认`LOG`）
    await changeLogTableName();

    try {
        // 初始化日志表
        await initLogTable();
    } catch (e) {
        console.log("初始化表失败", e);
    }
    // 初始化设置表
    await initSettingTable();

    // 初始化配置
    initSetting();

    // 每次启动app，检查版本号
    checkVersion();
});

const updateRef = ref(null);
async function checkVersion() {
    try {
        let res = await fetch(
            "https://api.github.com/repos/shiyutim/tickets/releases",
            {
                method: "GET",
            }
        );

        let json = await res.json();
        if (Array.isArray(json) && json.length) {
            let tag_name = json[0].tag_name;
            let version = tag_name.replace("v", "");
            if (version !== appVersion) {
                updateRef.value.showUpdate();
            }
        }

        // TODO
        // let res = await invoke("get_repo_version");
        // console.log("resaaaaa", res, JSON.parse(res));
    } catch (e) {
        console.log("e", e);
    }
}

async function handleOk() {
    const params = {
        proxy: form.proxy,
        appid_list: form.appid_list,
        appid: form.appid,
    };

    try {
        // 如果不存在，则插入，否则更新
        const fun = await getLastFun();
        const res = fun(settingTableName, params);

        if (res) {
            Message.success("保存成功");
            log.save(
                log.getTemplate(
                    "setting",
                    "设置保存成功",
                    "success",
                    `proxy: ${form.proxy}; appid: ${form.appid}`
                )
            );
            // 每次保存成功，则重新设置log表和初始化
            await changeLogTableName();
            await initLogTable();
        }
    } catch (e) {
        Message.error(e.toString());
    }
}

const visible = ref(false);
function setting() {
    visible.value = true;
}

const form = reactive({
    proxy: "",
    appid_list: [],
    appid: "",
});

const descIpt = ref("");

async function initSetting() {
    const res = await getSetting();
    if (!res) return;

    form.proxy = res.proxy;
    form.appid_list = res.appid_list ? JSON.parse(res.appid_list) : [];
    form.appid = res.appid;
}

async function getSetting() {
    try {
        const res = await selectAll(settingTableName);
        if (Array.isArray(res) && res.length) {
            return res[0];
        }
    } catch (e) {
        console.log(e);
    }

    return null;
}

const createBtnLoading = ref(false);
async function createId() {
    createBtnLoading.value = true;
    try {
        const current = {
            id: createAppId(),
            desc: descIpt.value,
        };

        const setting = (await getSetting()) || {};

        let list = setting.appid_list ? JSON.parse(setting.appid_list) : [];
        list.push(current);

        const fun = await getLastFun();
        let res = fun(settingTableName, {
            proxy: setting.proxy || "",
            appid_list: list,
            appid: setting.appid || "",
        });

        createBtnLoading.value = false;
        res && Message.success("创建成功！");

        descIpt.value = "";
        initSetting();
    } catch (e) {
        Message.error(e.toString());
    } finally {
        createBtnLoading.value = false;
    }
}

// 如果存在数据，则更新，否则添加
async function getLastFun() {
    const res = await getSetting();
    return res ? update : insert;
}
</script>

<template>
    <div class="app-container">
        <div class="nav-wrap">
            <a-menu
                :default-selected-keys="['dm']"
                :style="{ width: '200px', height: '100%' }"
            >
                <router-link
                    v-for="item in routes.filter(
                        (item) => item.meta && !item.meta.hideInMenu
                    )"
                    :key="item.name"
                    :to="item.path"
                >
                    <a-menu-item :key="item.name">
                        {{ item.meta.name }}
                    </a-menu-item>
                </router-link>
            </a-menu>

            <div class="setting-btn" @click="setting">
                <icon-settings style="font-size: 35px" />
            </div>
        </div>

        <div class="right">
            <Header></Header>
            <Tip
                style="margin-bottom: 5px"
                :text="`本软件目的是为了学习交流，严禁用于商业用途。该软件已在 Github 上开源，任何用于商业用途产生的后果与作者无关`"
            />
            <router-view></router-view>
        </div>

        <Update ref="updateRef" />

        <a-modal v-model:visible="visible" @ok="handleOk" width="620px">
            <template #title> 全局设置 </template>
            <div>
                <a-form :model="form" :style="{ width: '600px' }">
                    <a-form-item
                        field="proxy"
                        tooltip="本软件发送请求时使用的代理"
                        label="代理"
                    >
                        <a-input
                            v-model="form.proxy"
                            :style="{ width: '320px', margin: '0 10px 0 0' }"
                            placeholder="https://127.0.0.1:443"
                            allow-clear
                        />
                    </a-form-item>

                    <a-form-item field="appId" label="appId">
                        <template #extra>
                            <div>用来标记此次运行app的唯一标识</div>
                        </template>
                        <a-select
                            :style="{ width: '320px' }"
                            placeholder="请选择appId"
                            v-model="form.appid"
                        >
                            <a-option
                                :value="item.id"
                                :key="item.id"
                                v-for="item in form.appid_list"
                                >{{ item.id }}（{{ item.desc }}）</a-option
                            >
                        </a-select>
                    </a-form-item>
                    <a-form-item field="desc">
                        <a-input
                            style="width: 200px; margin-right: 10px"
                            v-model="descIpt"
                            placeholder="请填入备注"
                        />
                        <template #extra> 生成id的描述 </template>
                        <a-button :loading="createBtnLoading" @click="createId"
                            >生成id</a-button
                        >
                    </a-form-item>
                </a-form>
            </div>
        </a-modal>
    </div>
</template>

<style scoped lang="scss">
.app-container {
    display: flex;
    flex-flow: row nowrap;
    height: 100vh;
    overflow: hidden;
}
.right {
    width: 100%;
    height: 100%;
    overflow: auto;
    flex: 1;
}
.nav-wrap {
    width: 200px;
    height: 100%;
    position: relative;

    box-sizing: border-box;
    background-color: var(--color-neutral-2);
}

.setting-btn {
    position: absolute;
    bottom: 10px;
    left: 50%;
    transform: translateX(-50%);
    cursor: pointer;
}
</style>
