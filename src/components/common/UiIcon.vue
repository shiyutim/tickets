<script setup>
defineProps({ name: { type: String, default: "ticket" } });
const paths = {
    ticket: 'M4 4h16v5a3 3 0 0 0 0 6v5H4v-5a3 3 0 0 0 0-6V4Zm11 0v3m0 4v2m0 4v3',
    bilibili: 'M8 3l2 3m6-3-2 3M5 6h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2Zm3 5v3m8-3v3m-6 3h4',
    activity: 'M3 12h4l3-8 4 16 3-8h4',
    settings: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm0-6v3m0 14v3M2 12h3m14 0h3M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2',
    chevron: 'm9 5 7 7-7 7',
    arrow: 'M4 12h16m-6-6 6 6-6 6',
    link: 'M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-2 2m2 6a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l2-2',
    clock: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 4v5l3 2',
    refresh: 'M20 7v5h-5M4 17v-5h5m-4-4a8 8 0 0 1 13-3l2 2M4 17l2 2a8 8 0 0 0 13-3',
    check: 'm5 12 4 4L19 6',
    close: 'm6 6 12 12M6 18 18 6',
    user: 'M12 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM4 21v-3a5 5 0 0 1 5-5h6a5 5 0 0 1 5 5v3',
    calendar: 'M5 5h14a2 2 0 0 1 2 2v13H3V7a2 2 0 0 1 2-2Zm2-3v6m10-6v6M3 11h18m-13 4h1m6 0h1',
    location: 'M12 22s8-7 8-13a8 8 0 0 0-16 0c0 6 8 13 8 13Zm0-16a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z',
    launch: 'M14 3h7v7m0-7L10 14M9 3H3v18h18v-6',
    lock: 'M6 10h12v11H6V10Zm2 0V6a4 4 0 0 1 8 0v4m-4 4v3',
    info: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 7v6m0-9v.1',
    pause: 'M7 5v14M17 5v14',
    play: 'm7 4 14 8-14 8V4Z',
    search: 'M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14Zm5 12 6 6',
    download: 'M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5',
    github: 'M9 19c-5 1-5-2-7-2m14 5v-4a4 4 0 0 0-1-3c4 0 6-2 6-5a5 5 0 0 0-1-3 5 5 0 0 0 0-4s-2 0-4 2a14 14 0 0 0-8 0C6 3 4 3 4 3a5 5 0 0 0 0 4 5 5 0 0 0-1 3c0 3 2 5 6 5a4 4 0 0 0-1 3v4',
};
</script>
<template><svg class="ui-icon" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path :d="paths[name] || paths.ticket" /></svg></template>
