<script setup lang="ts">
import { ref } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import {
  llmApi,
  maskKey,
  providerDisplayName,
  PROTOCOL_HINTS,
  PROTOCOL_LABELS,
  PROTOCOLS,
  type Prompt,
  type Provider,
  type TestReport,
} from "../ai/llm";
import { useCad } from "../state/cad";

const {
  aiOpen,
  providers,
  prompts,
  activeProviderId,
  activePromptId,
  loadAi,
  setActiveProvider,
  setActivePrompt,
} = useCad();

const blankProvider = (): Provider => ({
  id: "",
  name: "",
  protocol: "open_ai_chat",
  base_url: "",
  model: "",
  api_key: "",
});

const blankPrompt = (): Prompt => ({ id: "", name: "", instruction: "", template: "" });

const providerForm = ref<Provider>(blankProvider());
const promptForm = ref<Prompt>(blankPrompt());
const testingId = ref<string | null>(null);
const saving = ref(false);
const lastReport = ref<TestReport | null>(null);

function editProvider(p: Provider): void {
  providerForm.value = { ...p };
  lastReport.value = null;
}

function newProvider(): void {
  providerForm.value = blankProvider();
  lastReport.value = null;
}

function editPrompt(p: Prompt): void {
  promptForm.value = { ...p };
}

function newPrompt(): void {
  promptForm.value = blankPrompt();
}

/** 当前表单所选协议的请求地址示例（模板里做下标访问会被 vue-tsc 的 ref 解包误判）。 */
function protocolHint(): string {
  return PROTOCOL_HINTS[providerForm.value.protocol];
}

/** 表单 → 落盘 → 重新拉取。新条目自动补 id。 */
async function persist(): Promise<{ ok: boolean; id: string }> {
  const form = providerForm.value;
  if (!form.base_url.trim() || !form.model.trim()) {
    ElMessage.warning("请填写请求地址与模型 ID");
    return { ok: false, id: "" };
  }
  if (!form.id) form.id = crypto.randomUUID();
  saving.value = true;
  try {
    await llmApi.saveProvider(form);
    await loadAi();
    return { ok: true, id: form.id };
  } catch (e) {
    ElMessage.error(`保存供应商失败：${String(e)}`);
    return { ok: false, id: "" };
  } finally {
    saving.value = false;
  }
}

async function saveProviderForm(): Promise<void> {
  const { ok, id } = await persist();
  if (ok) {
    ElMessage.success("供应商已保存");
    setActiveProvider(id);
  }
}

/** 连通性测试：先落盘再探测（后端按 id 从配置读取）。 */
async function testProviderForm(): Promise<void> {
  const { ok, id } = await persist();
  if (ok) await testProvider(id);
}

async function testProvider(id: string): Promise<void> {
  testingId.value = id;
  try {
    const report = await llmApi.testConnection(id);
    lastReport.value = report;
    if (report.ok) ElMessage.success(`连通正常 · ${report.latency_ms} ms`);
    else ElMessage.error(`连通失败：${report.detail}`);
  } catch (e) {
    ElMessage.error(`连通测试失败：${String(e)}`);
  } finally {
    testingId.value = null;
  }
}

async function removeProvider(p: Provider): Promise<void> {
  await ElMessageBox.confirm(`删除供应商「${providerDisplayName(p)}」？`, "删除供应商", {
    type: "warning",
    confirmButtonText: "删除",
    cancelButtonText: "取消",
  })
    .then(async () => {
      await llmApi.deleteProvider(p.id);
      await loadAi();
      if (providerForm.value.id === p.id) newProvider();
      ElMessage.success("已删除");
    })
    .catch(() => undefined);
}

async function savePromptForm(): Promise<void> {
  const form = promptForm.value;
  if (!form.name.trim() || !form.instruction.trim() || !form.template.trim()) {
    ElMessage.warning("名称、指令与模板均需填写");
    return;
  }
  if (!form.id) form.id = crypto.randomUUID();
  saving.value = true;
  try {
    await llmApi.savePrompt(form);
    await loadAi();
    setActivePrompt(form.id);
    ElMessage.success("提示词已保存");
  } catch (e) {
    ElMessage.error(`保存提示词失败：${String(e)}`);
  } finally {
    saving.value = false;
  }
}

async function removePrompt(p: Prompt): Promise<void> {
  await ElMessageBox.confirm(`删除提示词「${p.name}」？`, "删除提示词", {
    type: "warning",
    confirmButtonText: "删除",
    cancelButtonText: "取消",
  })
    .then(async () => {
      await llmApi.deletePrompt(p.id);
      await loadAi();
      if (promptForm.value.id === p.id) newPrompt();
      ElMessage.success("已删除");
    })
    .catch(() => undefined);
}
</script>

<template>
  <el-dialog v-model="aiOpen" title="AI 服务设置" width="780px" append-to-body>
    <el-tabs>
      <!-- ================= 供应商 ================= -->
      <el-tab-pane label="AI 供应商">
        <div class="grid">
          <div class="col">
            <div class="col-head">
              <span>已添加（{{ providers.length }}）</span>
              <el-button text type="primary" @click="newProvider">新建</el-button>
            </div>
            <el-empty v-if="!providers.length" :image-size="56" description="还没有供应商，用右侧表单添加" />
            <div
              v-for="p in providers"
              :key="p.id"
              class="row"
              :class="{ 'is-active': p.id === activeProviderId }"
              @click="setActiveProvider(p.id)"
            >
              <div class="row-main">
                <div class="row-title">
                  <span class="row-name">{{ providerDisplayName(p) }}</span>
                  <el-tag v-if="p.id === activeProviderId" type="primary" effect="plain" size="small">默认</el-tag>
                </div>
                <div class="row-sub">{{ PROTOCOL_LABELS[p.protocol] }} · {{ p.base_url }}</div>
                <div class="row-sub">模型 {{ p.model || "—" }} · 密钥 {{ maskKey(p.api_key) }}</div>
              </div>
              <div class="row-acts" @click.stop>
                <el-button text size="small" @click="editProvider(p)">编辑</el-button>
                <el-button
                  text
                  size="small"
                  :loading="testingId === p.id"
                  @click="testProvider(p.id)"
                >
                  测试
                </el-button>
                <el-button text type="danger" size="small" @click="removeProvider(p)">删除</el-button>
              </div>
            </div>
          </div>

          <div class="col">
            <div class="col-head"><span>{{ providerForm.id ? "编辑供应商" : "新建供应商" }}</span></div>
            <el-form label-position="top" size="default">
              <el-form-item label="显示名称（留空则显示「协议 · 模型」）">
                <el-input v-model="providerForm.name" placeholder="如：公司内网 DeepSeek" />
              </el-form-item>
              <el-form-item label="协议">
                <el-select v-model="providerForm.protocol">
                  <el-option
                    v-for="k in PROTOCOLS"
                    :key="k"
                    :value="k"
                    :label="PROTOCOL_LABELS[k]"
                  />
                </el-select>
              </el-form-item>
              <el-form-item label="请求地址">
                <el-input
                  v-model="providerForm.base_url"
                  placeholder="https://api.openai.com/v1"
                  spellcheck="false"
                />
                <span class="hint">示例：{{ protocolHint() }}</span>
              </el-form-item>
              <el-form-item label="模型 ID">
                <el-input v-model="providerForm.model" placeholder="如：gpt-4o / claude-sonnet-4-6" spellcheck="false" />
              </el-form-item>
              <el-form-item label="API 密钥">
                <el-input
                  v-model="providerForm.api_key"
                  type="password"
                  show-password
                  placeholder="仅保存在本机配置目录"
                  spellcheck="false"
                />
              </el-form-item>
              <div class="acts">
                <el-button type="primary" :loading="saving" @click="saveProviderForm">保存</el-button>
                <el-button :loading="testingId === providerForm.id" @click="testProviderForm">连通性测试</el-button>
                <el-button @click="newProvider">清空</el-button>
              </div>
              <el-alert
                v-if="lastReport"
                :title="lastReport.ok ? `连通正常 · ${lastReport.latency_ms} ms` : '连通失败'"
                :description="lastReport.detail"
                :type="lastReport.ok ? 'success' : 'error'"
                :closable="false"
                show-icon
              />
            </el-form>
          </div>
        </div>
      </el-tab-pane>

      <!-- ================= 提示词 ================= -->
      <el-tab-pane label="提示词库">
        <div class="grid">
          <div class="col">
            <div class="col-head">
              <span>已添加（{{ prompts.length }}）</span>
              <el-button text type="primary" @click="newPrompt">新建</el-button>
            </div>
            <el-empty v-if="!prompts.length" :image-size="56" description="还没有提示词，用右侧表单添加" />
            <div
              v-for="p in prompts"
              :key="p.id"
              class="row"
              :class="{ 'is-active': p.id === activePromptId }"
              @click="setActivePrompt(p.id)"
            >
              <div class="row-main">
                <div class="row-title">
                  <span class="row-name">{{ p.name }}</span>
                  <el-tag v-if="p.id === activePromptId" type="primary" effect="plain" size="small">默认</el-tag>
                </div>
                <div class="row-sub">{{ p.instruction }}</div>
              </div>
              <div class="row-acts" @click.stop>
                <el-button text size="small" @click="editPrompt(p)">编辑</el-button>
                <el-button text type="danger" size="small" @click="removePrompt(p)">删除</el-button>
              </div>
            </div>
            <el-text class="hint" size="small">
              点击列表项设为默认。命令行里输入自然语言即按「默认」提示词执行；画布悬浮条的「AI 修改」固定走
              「修改特征」。模板可用占位符 {input}（用户输入）与 {spec}（当前 FeatureSpec）。
            </el-text>
          </div>

          <div class="col">
            <div class="col-head"><span>{{ promptForm.id ? "编辑提示词" : "新建提示词" }}</span></div>
            <el-form label-position="top" size="default">
              <el-form-item label="名称">
                <el-input v-model="promptForm.name" placeholder="如：生成零件" />
              </el-form-item>
              <el-form-item label="指令（系统提示词的任务部分，FeatureSpec 规则由后端自动注入）">
                <el-input v-model="promptForm.instruction" type="textarea" :rows="3" />
              </el-form-item>
              <el-form-item label="模板（占位符：{input} 用户输入，{spec} 当前 FeatureSpec）">
                <el-input v-model="promptForm.template" type="textarea" :rows="4" spellcheck="false" />
              </el-form-item>
              <div class="acts">
                <el-button type="primary" :loading="saving" @click="savePromptForm">保存</el-button>
                <el-button @click="newPrompt">清空</el-button>
              </div>
            </el-form>
          </div>
        </div>
      </el-tab-pane>
    </el-tabs>

    <template #footer>
      <el-button type="primary" @click="aiOpen = false">完成</el-button>
    </template>
  </el-dialog>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  align-items: start;
}
.col {
  min-width: 0;
}
.col-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 13px;
  font-weight: 600;
  color: #374151;
  margin-bottom: 8px;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px;
  margin-bottom: 6px;
  border: 1px solid var(--cad-line);
  border-radius: var(--cad-radius);
  cursor: pointer;
  transition: border-color 0.12s ease, background-color 0.12s ease;
}
.row:hover {
  background: #f4f7fc;
}
.row.is-active {
  border-color: var(--cad-accent);
  background: var(--el-color-primary-light-9);
}
.row-main {
  min-width: 0;
}
.row-title {
  display: flex;
  align-items: center;
  gap: 8px;
}
.row-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--cad-text);
}
.row-sub {
  font-size: 12px;
  color: var(--cad-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-acts {
  display: flex;
  flex: none;
}
.acts {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.hint {
  display: block;
  margin-top: 6px;
  line-height: 1.5;
}
</style>
