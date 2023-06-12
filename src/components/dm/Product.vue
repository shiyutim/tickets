<script setup lang="js">
import { ref, reactive, computed } from 'vue'
import {
    getSign,
    getHeaderUaAndUmidtoken,
    commonTip,
    isSuccess,
    combinationOrderParams,
    joinMsg,
    encode,
    HAVE_ORDER,
    VALIDATE,
} from "../../../utils/dm/index.js";
import { invoke } from "@tauri-apps/api/tauri";
import { Message, Notification } from "@arco-design/web-vue";
import dayjs from 'dayjs';
import { useStore } from 'vuex'
import Log from '../../../utils/dm/log.js'

const store = useStore()
const log = new Log()

const productInfo = ref(null);

// 是否是预售商品
const isPreSell = ref(false)
// 倒计时
const isShowCountDown = ref(false);
// 接口时间
const countDownVal = ref(0);
// 修正时间
const timeFix = ref(0)
// 最终时间
const lastCountDownVal = computed(() => countDownVal.value + timeFix.value)

// 从 store 获取 form
const form = computed(() => store.state.dm.form)
async function getProductInfo() {
    const data = `{"itemId":"${form.value.itemId}","bizCode":"ali.china.damai","scenario":"itemsku","exParams":"{\\"dataType\\":4,\\"dataId\\":\\"\\",\\"privilegeActId\\":\\"\\"}","dmChannel":"damai@damaih5_h5"}`;
    const [t, sign] = getSign(data, form.value.token);

    try {
        const res = await invoke("get_product_info", {
            t,
            sign,
            itemid: form.value.itemId,
            cookie: form.value.cookie.trim(),
        });

        const parseData = JSON.parse(res);
        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                const result = parseData.data
                    ? JSON.parse(parseData.data.result)
                    : "";
                if (result) {
                    if (result.itemBuyBtn.btnStatus === "303") {
                        // 下架
                        Message.warning(`商品已下架`);
                        return;
                    } else if(result.itemBuyBtn.btnStatus === "100") {
                        // 不支持该渠道
                        Message.error(joinMsg([result.itemBuyBtn.btnText, result.itemBuyBtn.btnTips]))
                        return
                    } else if(result.itemBuyBtn.btnStatus === "106") {
                        // console.log('result', result)
                        // 即将开抢
                        // TODO bug fix
                        countDownVal.value = dayjs(result.itemBasicInfo.sellingStartTime).valueOf();
                        isPreSell.value = true
                        isShowCountDown.value = true;
                    }
                    productInfo.value = result;
                }
            } else {
                Message.error("获取商品详情失败");
            }
        }
    } catch (e) {
        Message.error(joinMsg(["获取商品详情失败", JSON.stringify(e)]));
    }
}

defineExpose({getProductInfo})

// sku列表，获取票档等信息
const skuInfo = reactive({});

// 当前高亮的 票档
const activeSku = ref(null);

async function getSkuInfo(item) {
    if (!item && item.performId) {
        Message.error("场地信息或场次 id 获取错误，请重新获取商品信息");
        return;
    }

    const data = `{"itemId":"${form.value.itemId}","bizCode":"ali.china.damai","scenario":"itemsku","exParams":"{\\"dataType\\":2,\\"dataId\\":\\"${item.performId}\\",\\"privilegeActId\\":\\"\\"}","dmChannel":"damai@damaih5_h5"}`;
    const [t, sign] = getSign(data, form.value.token);

    try {
        const res = await invoke("get_ticket_list", {
            t,
            sign,
            itemid: form.value.itemId,
            cookie: form.value.cookie,
            dataid: item.performId,
        });

        const parseData = JSON.parse(res);
        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                const result = parseData.data
                    ? JSON.parse(parseData.data.result)
                    : "";
                if (result) {
                    skuInfo[item.performId] = result;
                }
            }
        }
    } catch (e) {
        Message.error(joinMsg(["票档获取失败，请重试", JSON.stringify(e)]));
    }
}

function saveSku(item) {
    activeSku.value = item
}

// 获取订单详情
async function getOrderDetail(item) {
    const data = `{"buyNow":true,"exParams":"{\\"channel\\":\\"damai_app\\",\\"damai\\":\\"1\\",\\"umpChannel\\":\\"100031004\\",\\"subChannel\\":\\"damai@damaih5_h5\\",\\"atomSplit\\":1,\\"serviceVersion\\":\\"2.0.0\\",\\"customerType\\":\\"default\\"}","buyParam":"${item.itemId}_${form.value.num}_${item.skuId}","dmChannel":"damai@damaih5_h5"}`;

    const [t, sign] = getSign(data, form.value.token);
    const [ua, umidtoken] = getHeaderUaAndUmidtoken();

    try {
        log.save(log.getTemplate('tip', '获取订单详情'))
        const res = await invoke("get_ticket_detail", {
            t,
            sign,
            cookie: form.value.cookie,
            data,
            ua,
            umidtoken,
        });
        log.save(log.getTemplate('tip', '订单详情获取结束'))
        const parseData = JSON.parse(res);

        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                otherHandle(parseData.data)
            } else {
                Message.error(joinMsg(["error", ...parseData.ret]));
            }
        }
    } catch (e) {
        console.log(e)
        Message.error(joinMsg(["订单详情获取失败，请重试", JSON.stringify(e)]))
    } finally {
        return null
    }
}

const currentRetryCount = ref(0)
// 下订单（对应提交订单按钮）
async function createOrder(data, submitref) {
    const [t, sign] = getSign(data, form.value.token);
    const [ua, umidtoken] = getHeaderUaAndUmidtoken();

    let lastData = encode({
        data
    })
    lastData = `${lastData}&bx-ua=${ua}&bx-umidtoken=${umidtoken}`;
    try {
        log.save(log.getTemplate('tip', '开始创建订单'))
        const res = await invoke("create_order", {
            cookie: form.value.cookie,
            t,
            sign,
            data: lastData,
            submitref,
        });
        log.save(log.getTemplate('tip', '订单创建结束'))
        const parseData = JSON.parse(res);
        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                const msg = "购买成功!!!，请在订单页进行支付"
                log.save(log.getTemplate('tip', '购买成功', 'success', msg))
                Message.success(msg);
            } else {
                const msg = joinMsg(["error", ...parseData.ret])
                log.save(log.getTemplate('tip', '购买失败', 'error', msg))
                Message.error(msg);

                // 重新下订单过滤条件
                // 1. 已经有订单
                if(msg.includes(HAVE_ORDER)) {
                    const newMsg = '检测到已有订单'
                    log.save(log.getTemplate('tip', '已有订单', 'error', newMsg))
                    return
                }
                // 2. 账号被限制
                if(msg.includes(VALIDATE)) {
                    const newMsg = '检测到账号已被限制，请重新生成 cookie'
                    Message.error(newMsg)
                    log.save(log.getTemplate('tip', '账号被限制', 'error', newMsg))
                    return
                }

                // 只要没有抢票成功，就重新创建订单
                if(currentRetryCount.value) {
                    currentRetryCount.value =  currentRetryCount.value - 1
                    createOrder(data, submitref)
                } else {
                    // loading 取消
                    isRob.value = false
                }
            }
        }
    } catch (e) {
        console.log(e)
        Message.error(joinMsg(["订单发送失败，请重试", JSON.stringify(e)]))
    }
}

function getSecret(data) {
    return data.global
        ? data.global.secretKey + "=" + data.global.secretValue
        : "";
}

async function buy() {
    // 点击购买，就 重置 重试次数
    currentRetryCount.value = Number(form.value.retry) - 1

    // 对应票档的信息（对应*订单*详情页）
    getOrderDetail(activeSku.value)
}

const selectVisitUserList = computed(() => store.state.dm.selectVisitUserList)
function otherHandle(orderDetail) {
    const confirmParams = JSON.stringify(
        combinationOrderParams(orderDetail, selectVisitUserList.value)
    );

    const submitref = getSecret(orderDetail);
    createOrder(confirmParams, submitref);
}

// 1. 如果为非预售商品，则点击直接购买
// 2. 如果为预售商品，则点击后，倒计时结束则开始自动抢票
const isRob = ref(false)
async function rob() {
    log.save(log.getTemplate('click', '点击抢票'))
    // checkTime()
    isRob.value = true
    Notification.info({
        title: "开始抢票，当倒计时结束时，将自动购买。请确保本地电脑时间准确",
        duration: 8000
    })
}

// 时间提醒
function checkTime() {

}


// 倒计时是否结束
const isFinish = ref(false)
// 倒计时结束，如果为抢票开启状态，则直接购买
async function countDownFinished() {
    isFinish.value = true;

    if(isRob.value) {
        Notification.info(`开始抢票，当前时间为：${dayjs().format('hh:mm:ss:SSS')}`)
        log.save(log.getTemplate('tip', '开始抢票'))
        buy()
    } else {
        Notification.info("可以抢票啦")
    }
}
</script>

<template>
    <section class="product-wrap">
        <div class="subtitle">商品信息</div>
        <div v-if="productInfo">
            <div class="info-wrap" v-if="productInfo.itemBasicInfo">
                <div class="left">
                    <img
                        class="img"
                        :src="productInfo.itemBasicInfo.mainImageUrl"
                    />

                    <div v-if="isShowCountDown">
                        修正时间(毫秒)：
                        <a-input-number
                            v-model="timeFix"
                            :style="{ width: '200px' }"
                            placeholder="倒计时修正时间"
                        />
                    </div>
                    <div>
                        <a-countdown
                            v-if="isShowCountDown"
                            title="开售倒计时"
                            :value="lastCountDownVal"
                            :now="Date.now()"
                            format="HH:mm:ss.SSS"
                            @finish="countDownFinished"
                        />
                    </div>

                    <a-button
                        v-if="isPreSell"
                        style="margin-bottom: 10px"
                        type="primary"
                        status="success"
                        @click="rob"
                        :loading="isRob"
                        :disabled="!activeSku"
                        >抢票</a-button
                    >
                    <a-button
                        v-else
                        style="margin-bottom: 10px"
                        type="primary"
                        status="normal"
                        @click="buy"
                        :disabled="!activeSku"
                        >购票</a-button
                    >
                </div>

                <div class="right">
                    <div class="column">
                        <div>{{ productInfo.itemBasicInfo.cityName }}</div>
                        <div>{{ productInfo.itemBasicInfo.venueName }}</div>
                    </div>
                    <div class="column">
                        <div class="label">名称：</div>
                        <div class="value">
                            {{
                                productInfo.itemBasicInfo.itemTitle ||
                                productInfo.itemBasicInfo.projectTitle
                            }}
                        </div>
                    </div>
                    <div class="column">
                        <div class="label">itemId：</div>
                        <div class="value">
                            {{ productInfo.itemBasicInfo.itemId }}
                        </div>
                    </div>
                    <div class="column">
                        <div class="label">价格：</div>
                        <div class="value">
                            {{ productInfo.itemBasicInfo.priceRange }}
                        </div>
                    </div>

                    <div class="ticket-list">
                        <div
                            v-for="item in productInfo.performCalendar
                                .performViews"
                            class="ticket-item"
                            @click="getSkuInfo(item)"
                        >
                            {{ item.performName }}
                        </div>
                    </div>

                    <div class="sku-wrap">
                        <div v-for="item in skuInfo" class="sku">
                            <div
                                class="sku-item"
                                :class="{
                                    'active-sku':
                                        activeSku &&
                                        activeSku.skuId === sku.skuId,
                                }"
                                v-for="sku in item.perform.skuList"
                                @click="saveSku(sku)"
                            >
                                <div>{{ sku.priceName }}</div>
                                <div>{{ sku.price }}</div>
                                <div
                                    class="tag"
                                    v-if="
                                        Array.isArray(sku.tags) &&
                                        sku.tags.length
                                    "
                                >
                                    <a-tag color="#f53f3f">{{
                                        sku.tags[0].tagDesc
                                    }}</a-tag>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
        <div v-else>请先获取商品信息</div>
    </section>
</template>

<style scoped lang="scss">
.product-wrap {
    padding: 20px;
}
.info-wrap {
    display: flex;
    flex-flow: row nowrap;
}
.left {
    margin-right: 20px;
    .img {
        width: 250px;
        height: 350px;
    }
}
.column {
    display: flex;
    flex-flow: row nowrap;
    align-items: center;

    margin-bottom: 20px;
    &:last-child {
        margin-bottom: 0;
    }

    .label {
        margin-right: 5px;
        font-weight: 600;
    }
}

.subtitle {
    font-size: 20px;
    margin-bottom: 10px;
}

.ticket-list {
    display: flex;
    flex-flow: row nowrap;
}
.ticket-item {
    border: 1px solid #ccc;
    width: 200px;
    padding: 20px 0;
    margin-bottom: 10px;
    margin-right: 30px;

    text-align: center;
    cursor: pointer;
    &:hover {
        background: #eee;
    }
}

.sku-wrap {
    display: flex;
    flex-flow: row wrap;

    .sku-item {
        border: 1px solid #ccc;
        cursor: pointer;
        padding: 20px 0;
        margin-bottom: 10px;
        text-align: center;

        .tag {
            color: #ff0000;
        }
    }
    .active-sku {
        background: #eee;
    }
}
</style>