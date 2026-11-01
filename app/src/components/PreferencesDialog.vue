<script setup lang="ts">
import { useCad } from "../state/cad";

const { prefOpen, unit, unitOptions, wireframe, showGrid, showAxes, resetPrefs } = useCad();
</script>

<template>
  <el-dialog v-model="prefOpen" title="首选项" width="420px" append-to-body>
    <div class="pref-row">
      <span>单位</span>
      <el-segmented v-model="unit" :options="unitOptions" />
    </div>
    <div class="pref-row">
      <span>线框显示</span>
      <el-switch v-model="wireframe" />
    </div>
    <div class="pref-row">
      <span>显示网格</span>
      <el-switch v-model="showGrid" />
    </div>
    <div class="pref-row">
      <span>显示坐标轴</span>
      <el-switch v-model="showAxes" />
    </div>
    <template #footer>
      <el-button @click="resetPrefs">恢复默认</el-button>
      <el-button type="primary" @click="prefOpen = false">完成</el-button>
    </template>
  </el-dialog>
</template>

<style scoped>
.pref-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 7px 0;
  font-size: 13px;
}
</style>
