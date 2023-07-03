<script setup lang="js">
import { ref, computed } from 'vue'
import { roundNum } from '../../../utils/common/index'

const qaDmList = [
    {
        keyList: ['挤爆了', '挤爆'],
        valList: [
            "这是一个接口通用返回文案，表示接口请求失败",
        ],
    },
    {
        keyList: ['Session过期'],
        valList: [
            "表示 cookie 已经过期，请在商品页面刷新后重新获取",
        ],
    },
    {
        keyList: ['cookie'],
        valList: [
            "标识用户唯一身份，用来提交订单使用",
        ]
    },
    {
        keyList: ['使用'],
        valList: [
            "填入cookie、itemId、观演人信息后，点击获取商品信息，选择场次、票档后，点击购买或抢票即可。"
        ]
    },
    {
        keyList: ['注意', '注意事项'],
        valList: [
            "最好在商品预售前的一个小时内，进行倒计时抢票",
            "使用最新生成的 cookie",
            "倒计时抢票时，不要关闭本app，保持app为活动状态",
            "因为倒计时是根据你们自己电脑的本地时间，所以需要自行校准北京时间。如果时间不准，可能导致抢票失败！"
        ]
    },
    {
        keyList: ['FAIL_SYS_USER_VALIDATE', 'VALIDATE'],
        valList: [
            "因为在一段时间内频繁请求，导致出现滑块。应重新生成新的 cookie",
        ]
    },
    {
        keyList: ['选座'],
        valList: [
            "目前不支持选座"
        ]
    },
    {
        keyList: ['数据', '安全'],
        valList: [
            "所有数据都储存在本地，不会发送任何第三方"
        ]
    }
]

const iptValue = ref('')
const visible = ref(false)

function showModal() {
    visible.value = true
}

function getAllKeys() {
    return qaDmList.map(item => item.keyList).flat(Infinity);
}

// 随机生成个数
const CREATE_COUNT = 3

function getRandomKey() {
    // 获取所有的 关键词
    let allKeyList = getAllKeys()
    let resultKeys = []

    for(let i=0;i<CREATE_COUNT;i++) {
        let idx = roundNum(0, allKeyList.length - 1)
        resultKeys.push(allKeyList[idx])
    }

    return resultKeys.join(',')
}

const resultTipList = computed(() => {
    if(!iptValue.value) return []

    let idx = qaDmList.findIndex(item => item.keyList.includes(iptValue.value))
    if(idx >= 0) {
        return qaDmList[idx].valList
    }

    return []
})



defineExpose({
    showModal,
})
</script>

<template>
    <a-modal v-model:visible="visible">
        <template #title> QA </template>
        <div>
            <a-input
                v-model="iptValue"
                :style="{ width: '320px' }"
                :placeholder="`请输入关键词，例如: ${getRandomKey()}`"
                allow-clear
            />
        </div>

        <div class="result-tip">
            <ul v-for="item in resultTipList" :key="item">
                <li>{{ item }}</li>
            </ul>
        </div>
    </a-modal>
</template>

<style scoped lang="scss">
.result-tip {
    min-height: 200px;
    padding-top: 10px;
}

ul {
    margin: 0;
}
</style>
