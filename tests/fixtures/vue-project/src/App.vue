<script setup lang="ts">
import { formatDate } from './utils';
import { unusedImported } from './utils';
const date = new Date();
</script>

<template>
    <div>{{ formatDate(date) }}</div>
</template>
