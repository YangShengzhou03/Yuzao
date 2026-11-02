import { invoke } from "@tauri-apps/api/core";

/**
 * AI 服务的 IPC 边界。类型与 Rust 侧 `src-tauri/src/llm.rs` / `prompts.rs` 一一对应，
 * 字段名保持 snake_case（Tauri 只对「命令参数名」做 camelCase 转换，结构体字段由 serde 决定）。
 */

/** 与 Rust `Protocol` 的 `rename_all = "snake_case"` 对齐。 */
export type Protocol = "open_ai_chat" | "open_ai_responses" | "anthropic_messages";

export interface Provider {
  id: string;
  name: string;
  protocol: Protocol;
  base_url: string;
  model: string;
  api_key: string;
}

export interface Prompt {
  id: string;
  name: string;
  instruction: string;
  template: string;
}

export interface ProviderStore {
  providers: Provider[];
}

export interface PromptStore {
  prompts: Prompt[];
}

export interface TestReport {
  ok: boolean;
  latency_ms: number;
  detail: string;
}

export interface GenerateOutcome {
  ok: boolean;
  spec: string;
  raw: string;
  attempts: number;
  errors: string[];
}

export const PROTOCOL_LABELS: Record<Protocol, string> = {
  open_ai_chat: "OpenAI Chat Completions",
  open_ai_responses: "OpenAI Responses",
  anthropic_messages: "Anthropic Messages",
};

/** 各协议的请求地址前缀提示（Anthropic 走 /v1/messages，其余走 /chat/completions 或 /responses）。 */
export const PROTOCOL_HINTS: Record<Protocol, string> = {
  open_ai_chat: "https://api.openai.com/v1",
  open_ai_responses: "https://api.openai.com/v1",
  anthropic_messages: "https://api.anthropic.com",
};

export const PROTOCOLS = Object.keys(PROTOCOL_LABELS) as Protocol[];

/** 列表展示名：显示名称优先，否则退化为「协议 · 模型」。 */
export function providerDisplayName(p: Pick<Provider, "name" | "protocol" | "model">): string {
  const name = p.name.trim();
  return name || `${PROTOCOL_LABELS[p.protocol]} · ${p.model}`;
}

/** API 密钥脱敏展示。 */
export function maskKey(key: string): string {
  if (!key) return "未设置";
  if (key.length <= 8) return "••••";
  return `${key.slice(0, 4)}••••${key.slice(-4)}`;
}

export const llmApi = {
  listProviders: () => invoke<ProviderStore>("llm_list_providers"),
  saveProvider: (provider: Provider) => invoke<void>("llm_save_provider", { provider }),
  deleteProvider: (id: string) => invoke<void>("llm_delete_provider", { id }),
  testConnection: (providerId: string) => invoke<TestReport>("llm_test_connection", { providerId }),
  listPrompts: () => invoke<PromptStore>("llm_list_prompts"),
  savePrompt: (prompt: Prompt) => invoke<void>("llm_save_prompt", { prompt }),
  deletePrompt: (id: string) => invoke<void>("llm_delete_prompt", { id }),
  run: (req: {
    provider_id: string;
    prompt_id: string;
    input: string;
    current_spec?: string;
  }) => invoke<GenerateOutcome>("llm_run", { req }),
};
