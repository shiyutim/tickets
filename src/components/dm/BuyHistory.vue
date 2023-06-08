<script setup lang="js">
import { useStore } from 'vuex'
import { onMounted , computed} from 'vue';
import dayjs from 'dayjs'

const store = useStore()
const format = `YYYY-MM-DD HH:mm:ss`

const historyList = computed(() => store.state.dm.buyHistory.reverse())

// TODO 显示优化
function init() {
    // 加载本地数据
    let storageData = localStorage.getItem('buyHistory')
    storageData = storageData ? JSON.parse(storageData) : []

    // 写入 vuex 中
    store.commit("pushBuyHistory", storageData)
}

function getDesc(item) {
    return `开始: ${dayjs(item.startTime).format(format)}  结束: ${dayjs(item.endTime).format(format)}  花费: ${item.diff}ms`
}


onMounted(() => {
    init()
})
</script>

<template>
    <div>
        <a-collapse :default-active-key="[]">
            <a-collapse-item
                header="抢票记录(只保存在本地，卸载即消失)"
                key="1"
            >
                <a-list :max-height="300">
                    <a-list-item v-for="(item, idx) in historyList" :key="idx">
                        <a-list-item-meta
                            :title="item.message"
                            :description="getDesc(item)"
                        >
                        </a-list-item-meta>
                    </a-list-item>
                </a-list>
            </a-collapse-item>
        </a-collapse>
    </div>
</template>
