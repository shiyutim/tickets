<script setup lang="js">
import { ref, onMounted } from 'vue'

function initProxy() {
    const proxy = localStorage.getItem("proxy");

    proxy && (iptValue.value = JSON.parse(proxy))
}

const visible = ref(false)

function showModal() {
    visible.value = true
}

function hideModal() {
    visible.value = false
}

const iptValue = ref('')


function save() {
    if(!iptValue.value) {
        Message.warning("请填写内容")
        return
    }

    localStorage.setItem("proxy", JSON.stringify(iptValue.value))
    Message.success("设置成功")
    hideModal()
}

onMounted(() => {
    initProxy()
})

defineExpose({
    showModal,
})
</script>

<template>
    <a-modal v-model:visible="visible" :footer="false">
        <template #title>代理设置</template>
        <div>
            <a-input
                v-model="iptValue"
                :style="{ width: '320px', margin: '0 10px 0 0' }"
                placeholder="https://www.baidu.com:443"
                allow-clear
            />
            <a-button type="primary" @click="save">保存</a-button>
        </div>
    </a-modal>
</template>
