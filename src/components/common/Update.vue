<script setup lang="js">
import { ref } from 'vue';
const downloadUrl = "https://github.com/shiyutim/tickets/releases"


const visible = ref(false)

function show() {
    visible.value = true
}

defineExpose({
    showUpdate: show
})
</script>

<template>
    <a-modal v-model:visible="visible" :footer="false">
        <template #title> 版本更新 </template>
        <div>
            检测到最新版本，
            <a :href="downloadUrl" target="_blank">点击</a>
            下载最新版本
        </div>
    </a-modal>
</template>
