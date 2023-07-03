<script setup lang="js">
import { useStore } from 'vuex'
import { computed, ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'
import { Message } from "@arco-design/web-vue";
import {
    getSign,
    commonTip,
    isSuccess,
    joinMsg,
} from "../../../utils/dm/index.js";


onMounted(() => {
    initVisitUser()
})

const store = useStore();

const form = computed(() => store.state.dm.form);
const visitUserList = computed(() => store.state.dm.visitUserList)

// TODO
function initVisitUser() {
    // 先从本地加载
    // 如果不存在，则请求接口并保存
    const data = getStore()
    if(Array.isArray(data) && data.length) {
        store.commit("ADD_VISIT_USER", data)
    } else {
        getVisitUser()
    }
}

const isLoading = ref(false)

async function getVisitUser() {
    const data = `{"customerType":"default","dmChannel":"damai@damaih5_h5"}`;
    const [t, sign] = getSign(data, form.value.token);
    try {
        const res = await invoke("get_user_list", {
            t,
            sign,
            cookie: form.value.cookie,
            data,
            isProxy: form.value.isUseProxy,
            address: form.value.proxy,
        })

        const parseData = JSON.parse(res)
        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];
            commonTip(message);

            if (isSuccess(message)) {
                const result = parseData.data.result
                if(Array.isArray(result) && result.length) {
                    saveStore(result)
                    store.commit("ADD_VISIT_USER", result)
                } else {
                    Message.error("观演人信息获取错误，请设置正确的信息")
                }
            } else {
                Message.error(joinMsg(["获取观演人信息错误", ...parseData.ret]));
            }
        }
    } catch(e) {
        Message.error(joinMsg(["观演人信息获取错误", JSON.stringify(e)]))
    } finally {
        isLoading.value = false
    }
}

function saveStore(data) {
    localStorage.setItem("visitUserList", JSON.stringify(data))
}

function getStore() {
    const visitUserList = localStorage.getItem("visitUserList")
    return visitUserList ? JSON.parse(visitUserList) : []
}

function setLoading() {
    if(!form.value.cookie) {
        Message.warning("请先填写 cookie")

        return
    }
    isLoading.value = true

    // TODO 更新后，清空选择状态
    getVisitUser()
}

function userChange(valList) {
    store.commit("SET_SELECT_VISIT_USER", valList)
}

defineExpose({initVisitUser})
</script>

<template>
    <div>
        <h4>
            观演人选择
            <a-button @click="setLoading" type="primary" :loading="isLoading"
                >更新</a-button
            >
        </h4>
        <a-checkbox-group @change="userChange" direction="vertical">
            <a-checkbox
                v-for="item in visitUserList"
                :key="item.maskedIdentityNo"
                :value="item.maskedIdentityNo"
            >
                {{ item.maskedName }} {{ item.identityTypeName }}
                {{ item.maskedIdentityNo }}
            </a-checkbox>
        </a-checkbox-group>
    </div>
</template>
