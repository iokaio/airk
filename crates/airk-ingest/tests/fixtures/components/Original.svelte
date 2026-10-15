<!-- SPDX-License-Identifier: Apache-2.0 -->
<script context="module">
export const label = "café";
</script>
<script>
let title = "archive";
function save() { return title; }
</script>
<input bind:value="title" on:change="save" />
<slot name="content"></slot>
