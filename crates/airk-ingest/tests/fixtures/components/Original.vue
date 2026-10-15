<!-- SPDX-License-Identifier: Apache-2.0 -->
<template>
  <ArchiveCard :title="title" @save="save"><template #default>{{ title }}</template></ArchiveCard>
</template>
<script setup lang="ts">
const title: string = "café";
function save(): string { return title; }
</script>
<style scoped lang="scss">
$tone: #369;
.archive { color: $tone; }
</style>
