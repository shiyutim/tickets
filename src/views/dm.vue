<script setup lang="js">
import { ref, reactive, onMounted } from "vue";
import {
    loadBaxiaScript,
    initBaxia,
    loadBaxiaTime,
} from "../../utils/dm/index.js";
import { Message } from "@arco-design/web-vue";
import Form from '../components/dm/Form.vue'
import Product from '../components/dm/Product.vue'

onMounted(() => {
    // 加载凭证脚本
    loadBaxiaScript();
    // 初始化凭证
    setTimeout(() => {
        let res = initBaxia();
        if (!res) {
            Message.error(
                "初始化凭证脚本失败，本脚本将无法继续，请重新启动程序。如一直无法加载，请联系管理员修复。"
            );
        }
    }, loadBaxiaTime);
});


// 商品组件引用
const productRef = ref(null)

// 获取商品信息
const handleSubmit = async () => {
    // await getProductInfo();
    // if(productInfo.value) {
        formActive.value = []
    // }
    productRef.value.getProductInfo()
};

// 展示收起逻辑
const formActive = ref(['1'])
function collapseChange() {
    if(formActive.value.length) {
        formActive.value = []
    } else {
        formActive.value = ['1']
    }
}
</script>

<template>
    <div class="container">
        <a-collapse :activeKey="formActive" :onChange="collapseChange">
            <a-collapse-item header="基本信息" key="1">
                <Form :handleSubmit="handleSubmit"></Form>
            </a-collapse-item>
        </a-collapse>
        <product ref="productRef"></product>
    </div>
</template>

<style scoped lang="scss"></style>
