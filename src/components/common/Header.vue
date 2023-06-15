<script setup lang="js">
import { ref } from 'vue';
import dayjs from 'dayjs'
import Log from '../../../utils/dm/log';
import { WebviewWindow } from '@tauri-apps/api/window'
import { Message } from '@arco-design/web-vue';
const visible = ref(false);

const log = new Log()

const defaultValue = ref([dayjs().format('YYYY-MM-DD'), dayjs().format('YYYY-MM-DD')])

function initLog() {
    let res = log.getRangeDayStorage(new Date(defaultValue.value[0]), new Date(defaultValue.value[1]))
    list.value = res.flat(Infinity).reverse();
}

function initDate() {
    defaultValue.value = [dayjs().format('YYYY-MM-DD'), dayjs().format('YYYY-MM-DD')]
}

function showLog() {
    visible.value = true;
    initDate()
    initLog()
}

function handleCancel() {
    visible.value = false;
}

const list = ref([])
function onChange(dateString, date) {
    let res = Array.isArray(dateString) && dateString.length ? log.getRangeDayStorage(new Date(dateString[0]), new Date(dateString[1])) : log.getAllStorageList()
    list.value = res.flat(Infinity).reverse();
}

function isShowTag(status) {
    if(status === 'success' || status === 'error') {
        return true
    }

    return false
}

function getTagColor(status) {
    const template = {
        success: "green",
        error: "#f53f3f"
    }

    return template[status] || '#86909c'
}


const timeWebview = ref(null)
async function goTime() {
    if(timeWebview.value) {
        console.log(timeWebview.value)
        return
    }
    timeWebview.value = new WebviewWindow('time', {
        url: 'http://time.tianqi.com/',
    })

    timeWebview.value.once('tauri://created', function () {
        console.log("创建成功")
    })
    timeWebview.value.once('tauri://error', function (e) {
        Message.error("窗口创建失败，请手动访问 http://time.tianqi.com/或百度搜索 北京时间，自行查看校准")
    })
}
</script>

<template>
    <div class="container">
        <a-button style="margin-right: 10px" type="primary" @click="showLog"
            >查看日志</a-button
        >
        <a-button type="primary" @click="goTime">校准时间</a-button>

        <a-drawer
            :footer="false"
            :width="340"
            :visible="visible"
            @cancel="handleCancel"
            unmountOnClose
        >
            <template #title> 操作日志（暂时） </template>
            <div>
                <div style="margin-bottom: 10px">
                    <a-range-picker
                        @change="onChange"
                        style="width: 254px; marginbottom: 20px"
                        :defaultValue="defaultValue"
                    />
                </div>

                <div v-if="Array.isArray(list) && list.length">
                    <a-card
                        v-for="item in list"
                        :key="item.time"
                        :style="{
                            width: '260px',
                            margin: '0 0 10px 0',
                        }"
                        :title="item.title"
                    >
                        <template #extra>
                            <a-tag
                                v-if="isShowTag(item.status)"
                                :color="getTagColor(item.status)"
                                >{{ item.status }}</a-tag
                            >
                        </template>
                        {{ dayjs(item.time).format("YYYY-MM-DD HH:mm:ss.SSS") }}
                        <br />
                        {{ item.msg }}
                    </a-card>
                </div>
                <div v-else>暂无数据</div>
            </div>
        </a-drawer>
    </div>
</template>

<style scoped>
.container {
    margin: 20px;
}
</style>
