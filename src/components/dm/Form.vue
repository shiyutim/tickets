<script setup lang="js">
import { reactive, watch } from 'vue';
import { getQueryString } from '../../../utils/common';
import { Message } from "@arco-design/web-vue";
import { useStore } from 'vuex';
import {
    getToken,
} from "../../../utils/dm/index.js";

const store = useStore();

const form = reactive({
    cookie: "",
    itemId: "",
    token: "", // 获取 sign 使用
    url: "",
    num: "1",
    retry: "2",
});

watch(() => form.url, (url) => {
    const search = url.split("html")[1]
    const itemId = getQueryString('itemId', search)
    if(itemId) {
        form.itemId = itemId;
    } else {
        Message.warning("此url无法获取 itemId")
    }
})

const props = defineProps({
    handleSubmit: Function
})

function handleSubmit(e) {
    if(e.errors) {
        return
    }

    store.commit('addForm', {
        ...e.values,
        cookie: e.values.cookie.trim(),
        // 根据 cookie 解析 token，用来生成 t 和 sign
        token:  getToken(e.values.cookie.trim())
    })
    props.handleSubmit()
}
</script>

<template>
    <section>
        <a-form
            :model="form"
            :style="{ width: '600px' }"
            @submit="handleSubmit"
        >
            <a-form-item field="cookie" label="cookie" required>
                <a-textarea
                    v-model="form.cookie"
                    placeholder="请输入 cookie"
                    allow-clear
                />
            </a-form-item>

            <a-form-item field="url" label="url">
                <a-input v-model="form.url" placeholder="请输入商品详情页url">
                </a-input>
            </a-form-item>

            <a-form-item
                field="itemId"
                tooltip="进入商品页面，查看页面路径上的 itemId"
                label="itemId"
                required
            >
                <a-input v-model="form.itemId" placeholder="请输入itemId..." />
            </a-form-item>

            <a-form-item
                field="num"
                label="购买张数"
                required
                tooltip="目前的购买张数必须跟实名信息个数一致"
            >
                <a-input v-model="form.num" placeholder="请输入购买数量..." />
            </a-form-item>
            <a-form-item
                field="retry"
                label="重试次数"
                required
                tooltip="下订单失败的重试次数"
            >
                <a-input v-model="form.retry" disabled />
            </a-form-item>

            <a-form-item>
                <a-button html-type="submit">确定</a-button>
            </a-form-item>
        </a-form>
    </section>
</template>
