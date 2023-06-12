<script setup lang="js">
import { ref } from 'vue';
import dayjs from 'dayjs'
import Log from '../../../utils/dm/log';
const visible = ref(false);

const log = new Log()

function initLog() {
    // log.getCurrentStorage()
}

function showLog() {
    console.log('showLog')
    visible.value = true;
}

function handleCancel() {
    visible.value = false;
}

// function onSelect(dateString, date) {
//     console.log('onSelect', dateString, date);
// }

const list = ref([])
function onChange(dateString, date) {
    let res = Array.isArray(dateString) && dateString.length ? log.getRangeDayStorage(new Date(dateString[0]), new Date(dateString[1])) : log.getAllStorageList()
    console.log('res' , dateString, res)
    list.value = res.flat(Infinity);
}
</script>

<template>
    <div class="container">
        <a-button type="primary" @click="showLog">查看日志</a-button>

        <a-drawer
            :footer="false"
            :width="340"
            :visible="visible"
            @cancel="handleCancel"
            unmountOnClose
        >
            <template #title> 操作日志（暂时） </template>
            <div>
                <div>
                    <a-range-picker
                        @change="onChange"
                        @select="onSelect"
                        style="width: 254px; marginbottom: 20px"
                    />
                </div>

                <a-list>
                    <!-- <template #header>
      List title
    </template> -->
                    <a-list-item v-for="(item, index) in list" :key="index"
                        >{{
                            dayjs(item.time).format("YYYY-MM-DD HH:mm:ss.SSS")
                        }}
                        {{ item.title }}{{ item.msg }}</a-list-item
                    >
                </a-list>
            </div>
        </a-drawer>
    </div>
</template>

<style scoped>
.container {
    margin: 20px;
}
</style>
