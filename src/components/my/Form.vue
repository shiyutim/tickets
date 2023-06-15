<script setup lang="js">
import { ref, reactive } from 'vue';


const form = reactive({
    cookie: "",
    url: "",
    num: 1,
    retry: 2,
    // selectVisitUserList: [] //选择的观演人
});

const props = defineProps({
    handleSubmit: Function
})

function handleSubmit(e) {
    if(e.errors) {
        return
    }

    // 检查票数和观演人是否一致
    if(Number(form.num) !== form.selectVisitUserList.length) {
        Message.error("购买票数需要与观演人一致")

        return
    }



}
</script>

<template>
    <section>
        <a-form
            :model="form"
            :style="{ width: '800px' }"
            @submit="handleSubmit"
        ></a-form>
    </section>
</template>
