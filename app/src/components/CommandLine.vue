<script setup lang="ts">
import { useCad } from "../state/cad";

const { input, submitCommand, onInputKey } = useCad();
</script>

<template>
  <!-- 命令行（宽度跟随画布，右侧 side 贯穿到底） -->
  <div class="cmdline">
    <span class="cmd-prompt">命令:</span>
    <el-input
      v-model="input"
      class="cmd-input"
      placeholder="输入自然语言描述零件、指令或粘贴 FeatureSpec JSON，回车执行（↑/↓ 翻历史，help 查看命令）"
      @keydown="onInputKey"
    />
    <el-button type="primary" @click="submitCommand(input)">发送</el-button>
  </div>
</template>

<style scoped>
.cmdline {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 12px;
  height: var(--cad-cmdline-h);
  box-sizing: border-box;
  flex: none;
  background: var(--cad-panel);
  border-top: 1px solid var(--cad-line);
}
.cmd-prompt {
  font-family: var(--cad-mono);
  font-size: 13px;
  font-weight: 600;
  color: var(--cad-accent);
  flex: none;
}
.cmd-input {
  flex: 1;
  min-width: 0;
}
.cmd-input :deep(.el-input__inner) {
  font-family: var(--cad-mono);
  font-size: 13px;
  background: #fbfcfe;
  border-radius: var(--cad-radius);
}
</style>
