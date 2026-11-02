//! 多协议 LLM 接入（ROADMAP P5.1）。
//!
//! 支持三种协议：OpenAI Chat Completions、OpenAI Responses、Anthropic Messages。
//! 供应商配置持久化到 `app_config_dir/yuzao-llm.json`。LLM 只负责产出
//! FeatureSpec JSON（ADR-012）；校验与「错误回喂重试」由本模块闭环，
//! 前端拿到的要么是可直接应用的合法 spec，要么是结构化错误清单。

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// 首次 + 纠错重试的最大尝试次数。
const MAX_ATTEMPTS: usize = 3;
const HTTP_TIMEOUT: Duration = Duration::from_secs(180);

// ---------------------------------------------------------------------------
// 供应商配置
// ---------------------------------------------------------------------------

/// 支持的 LLM 协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// `POST {base}/chat/completions`
    OpenAiChat,
    /// `POST {base}/responses`
    OpenAiResponses,
    /// `POST {base}/v1/messages`
    AnthropicMessages,
}

impl Protocol {
    fn path(self) -> &'static str {
        match self {
            Protocol::OpenAiChat => "/chat/completions",
            Protocol::OpenAiResponses => "/responses",
            Protocol::AnthropicMessages => "/v1/messages",
        }
    }
}

/// 一个 LLM 服务供应商。`api_key` 仅落盘到本地配置文件，不上传。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
}

impl Provider {
    fn endpoint(&self) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), self.protocol.path())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderStore {
    #[serde(default)]
    pub providers: Vec<Provider>,
}

impl ProviderStore {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("yuzao-llm.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("读取 AI 配置失败: {e}"))
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("yuzao-llm.json"), text).map_err(|e| e.to_string())
    }

    /// 按 id 插入或覆盖。
    pub fn upsert(&mut self, provider: Provider) {
        match self.providers.iter_mut().find(|p| p.id == provider.id) {
            Some(slot) => *slot = provider,
            None => self.providers.push(provider),
        }
    }

    pub fn find(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }
}

// ---------------------------------------------------------------------------
// 对外结果
// ---------------------------------------------------------------------------

/// 连通性测试结果。
#[derive(Debug, Clone, Serialize)]
pub struct TestReport {
    pub ok: bool,
    pub latency_ms: u128,
    pub detail: String,
}

/// 一次 LLM 生成/修改的结果。`ok=false` 时 `spec` 为空，`errors` 为校验错误清单。
#[derive(Debug, Clone, Serialize)]
pub struct GenerateOutcome {
    pub ok: bool,
    pub spec: String,
    pub raw: String,
    pub attempts: usize,
    pub errors: Vec<String>,
}

/// `llm_run` 命令的入参（单对象传参，避开 IPC 命名转换歧义）。
#[derive(Debug, Clone, Deserialize)]
pub struct LlmRunRequest {
    pub provider_id: String,
    pub prompt_id: String,
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub current_spec: Option<String>,
}

// ---------------------------------------------------------------------------
// 系统提示词
// ---------------------------------------------------------------------------

/// 系统提示词 = FeatureSpec 规则 + 内嵌 JSON Schema + 任务指令。
///
/// 规则与 Schema 由本函数统一注入，用户只需在提示词库里写「任务本身」。
pub fn system_for(instruction: &str) -> String {
    format!(
        "你是 Creo 参数化建模的 FeatureSpec 生成器。\n\
只输出一个合法的 FeatureSpec JSON 对象：不要解释、不要注释、不要 markdown 代码围栏。\n\
单位一律为毫米（mm）。\n\
\n\
FeatureSpec JSON Schema:\n{}\n\
\n\
关键规则：\n\
- features 中每个特征有唯一 id（形如 F001），op 只能是 sketch / extrude / hole；\n\
- 只允许引用更早出现的特征（禁止前向引用，Creo 特征树只能向后构建）；\n\
- extrude.sketch 必须引用 sketch；hole.on 必须形如 F002.face[top] 且落在 extrude 上；\n\
- 所有尺寸 / 深度 / 孔径必须为有限正数；\n\
- edits 用于改参，target 形如 F003.dia 或 F001.dims.w，同一 target 只能出现一次。\n\
\n\
当前任务：{instruction}",
        featurespec::SCHEMA_JSON
    )
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// 从三种协议的响应体里取出助手文本。
fn extract_text(protocol: Protocol, v: &serde_json::Value) -> Option<String> {
    match protocol {
        Protocol::OpenAiChat => v["choices"][0]["message"]["content"]
            .as_str()
            .map(ToOwned::to_owned),
        Protocol::OpenAiResponses => {
            if let Some(text) = v["output_text"].as_str() {
                return Some(text.to_owned());
            }
            v["output"]
                .as_array()
                .and_then(|items| {
                    items.iter().find_map(|item| {
                        item["content"].as_array().and_then(|blocks| {
                            blocks
                                .iter()
                                .find_map(|b| b["text"].as_str().map(ToOwned::to_owned))
                        })
                    })
                })
        }
        Protocol::AnthropicMessages => v["content"][0]["text"]
            .as_str()
            .map(ToOwned::to_owned),
    }
}

/// 发送一次对话请求，返回助手文本。
async fn chat(
    http: &reqwest::Client,
    provider: &Provider,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let endpoint = provider.endpoint();
    let resp = match provider.protocol {
        Protocol::OpenAiChat => {
            let body = serde_json::json!({
                "model": provider.model,
                "temperature": 0,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user },
                ]
            });
            http.post(&endpoint)
                .header("Authorization", format!("Bearer {}", provider.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
        }
        Protocol::OpenAiResponses => {
            let body = serde_json::json!({
                "model": provider.model,
                "input": [
                    { "role": "system", "content": [{ "type": "input_text", "text": system }] },
                    { "role": "user",   "content": [{ "type": "input_text", "text": user }] },
                ]
            });
            http.post(&endpoint)
                .header("Authorization", format!("Bearer {}", provider.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
        }
        Protocol::AnthropicMessages => {
            let body = serde_json::json!({
                "model": provider.model,
                "max_tokens": 8192,
                "system": system,
                "messages": [{ "role": "user", "content": user }]
            });
            http.post(&endpoint)
                .header("x-api-key", &provider.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
        }
    }
    .map_err(|e| format!("请求失败：{e}"))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败：{e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("响应不是合法 JSON：{e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", truncate(&text, 600)));
    }
    extract_text(provider.protocol, &value)
        .ok_or_else(|| format!("响应缺少文本字段：{}", truncate(&text, 600)))
}

// ---------------------------------------------------------------------------
// 连通性测试
// ---------------------------------------------------------------------------

/// 用最小请求探测供应商是否可用（鉴权 + 协议 + 模型名）。
pub async fn test_connection(provider: &Provider) -> TestReport {
    if provider.base_url.trim().is_empty() {
        return TestReport { ok: false, latency_ms: 0, detail: "请求地址为空".into() };
    }
    if provider.model.trim().is_empty() {
        return TestReport { ok: false, latency_ms: 0, detail: "模型 ID 为空".into() };
    }

    let http = match client() {
        Ok(c) => c,
        Err(e) => return TestReport { ok: false, latency_ms: 0, detail: e },
    };
    let started = std::time::Instant::now();
    let result = chat(&http, provider, "You are a connectivity probe.", "ping").await;
    let latency = started.elapsed().as_millis();

    match result {
        Ok(text) => TestReport {
            ok: true,
            latency_ms: latency,
            detail: format!("连接成功，首段响应：{}", truncate(&text, 80)),
        },
        Err(e) => TestReport { ok: false, latency_ms: latency, detail: e },
    }
}

// ---------------------------------------------------------------------------
// 生成 / 修改（含校验重试）
// ---------------------------------------------------------------------------

/// 从模型输出中截取 JSON 对象（容忍 ```json 围栏与前后废话）。
fn extract_json(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim_start();
    let stripped = stripped.strip_suffix("```").unwrap_or(stripped).trim_end();

    let start = stripped.find('{').ok_or_else(|| "输出中未找到 JSON 对象".to_string())?;
    let end = stripped.rfind('}').ok_or_else(|| "输出中未找到 JSON 对象".to_string())?;
    if end < start {
        return Err("输出中未找到 JSON 对象".to_string());
    }
    let candidate = &stripped[start..=end];
    serde_json::from_str::<serde_json::Value>(candidate)
        .map_err(|e| format!("JSON 解析失败：{e}"))?;
    Ok(candidate.to_string())
}

/// 调用 LLM 产出 FeatureSpec：校验失败时把全部错误回喂，最多 [`MAX_ATTEMPTS`] 次。
pub async fn run_llm(
    provider: &Provider,
    system: &str,
    user: &str,
) -> Result<GenerateOutcome, String> {
    let http = client()?;
    let mut last_errors: Vec<String> = Vec::new();
    let mut raw = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        let full_user = if attempt == 1 {
            user.to_string()
        } else {
            format!(
                "{user}\n\n上一版输出未通过 FeatureSpec 校验，请修正后重新输出完整 JSON：\n{}",
                last_errors.join("\n")
            )
        };

        raw = chat(&http, provider, system, &full_user).await?;
        let json = extract_json(&raw)?;

        match featurespec::parse(&json) {
            Ok(spec) => {
                let pretty = serde_json::to_string_pretty(&spec).map_err(|e| e.to_string())?;
                return Ok(GenerateOutcome {
                    ok: true,
                    spec: pretty,
                    raw,
                    attempts: attempt,
                    errors: Vec::new(),
                });
            }
            Err(errs) => {
                last_errors = errs.iter().map(|e| e.to_string()).collect();
            }
        }
    }

    Ok(GenerateOutcome { ok: false, spec: String::new(), raw, attempts: MAX_ATTEMPTS, errors: last_errors })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_appends_protocol_path() {
        let p = Provider {
            id: "p1".into(),
            name: "测试".into(),
            protocol: Protocol::OpenAiChat,
            base_url: "https://api.example.com/v1/".into(),
            model: "m".into(),
            api_key: "k".into(),
        };
        assert_eq!(p.endpoint(), "https://api.example.com/v1/chat/completions");
        assert_eq!(Protocol::AnthropicMessages.path(), "/v1/messages");
    }

    #[test]
    fn anthropic_endpoint_is_versioned() {
        let p = Provider {
            id: "p1".into(),
            name: String::new(),
            protocol: Protocol::AnthropicMessages,
            base_url: "https://api.anthropic.com".into(),
            model: "claude".into(),
            api_key: String::new(),
        };
        assert_eq!(p.endpoint(), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn extract_json_tolerates_fences_and_prose() {
        let raw = "好的，这是结果：\n```json\n{\"model\":\"m\",\"type\":\"part\",\"features\":[]}\n```\n完毕";
        let json = extract_json(raw).expect("应能截取 JSON");
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
    }

    #[test]
    fn extract_json_rejects_non_json() {
        assert!(extract_json("没有对象").is_err());
    }
}
