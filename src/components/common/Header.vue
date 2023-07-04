<script setup lang="js">
import { ref } from 'vue';
import dayjs from 'dayjs'
import Log from '../../../utils/common/log';
import { WebviewWindow } from '@tauri-apps/api/window'
import Qa from '../common/Qa.vue'

const visible = ref(false);

const log = new Log()

const defaultValue = ref([dayjs().format('YYYY-MM-DD'), dayjs().format('YYYY-MM-DD')])

async function initLog() {
    const res = await log.getRangeDayStorage(new Date(defaultValue.value[0]), new Date(defaultValue.value[1]))
    list.value = res
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
async function onChange(dateString, date) {
    const res = Array.isArray(dateString) && dateString.length ? await log.getRangeDayStorage(new Date(dateString[0]), new Date(dateString[1])) : await log.getAllStore()
    list.value = res
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
        timeWebview.value = null
    }
    timeWebview.value = new WebviewWindow('time', {
        url: 'http://time.tianqi.com/',
    })

    timeWebview.value.once('tauri://created', function () {
    })
    timeWebview.value.once('tauri://error', function (e) {
        Message.error("窗口创建失败，请手动访问 http://time.tianqi.com/或百度搜索 北京时间，自行查看校准")
    })
}

const qaRef = ref(null)

function showQa() {
    qaRef.value.showModal()
}
</script>

<template>
    <div class="container">
        <a-button style="margin-right: 10px" type="primary" @click="showLog"
            >查看日志</a-button
        >
        <a-button style="margin-right: 10px" type="primary" @click="goTime"
            >北京时间</a-button
        >

        <a-button style="margin-right: 10px" type="primary" @click="showQa"
            >QA</a-button
        >

        <a-drawer
            :footer="false"
            :width="340"
            :visible="visible"
            @cancel="handleCancel"
            unmountOnClose
        >
            <template #title> 操作日志 </template>
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

        <Qa ref="qaRef"></Qa>
    </div>
</template>

<style scoped>
.container {
    margin: 20px;
}
</style>
