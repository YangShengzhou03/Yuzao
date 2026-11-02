// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod llm;
mod prompts;

use tauri::Manager;

/// 校验 FeatureSpec JSON（ADR-012：LLM 的唯一输出格式）。
/// 返回结构化结果：Ok(摘要) / Err(错误码清单)，供前端即时反馈。
#[tauri::command]
fn validate_spec(json: String) -> Result<serde_json::Value, Vec<String>> {
    match featurespec::parse(&json) {
        Ok(spec) => Ok(serde_json::json!({
            "model": spec.model,
            "type": spec.spec_type,
            "features": spec.features.len(),
            "edits": spec.edits.len(),
        })),
        Err(errs) => Err(errs.iter().map(|e| e.to_string()).collect()),
    }
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// ---------------------------------------------------------------------------
// AI 服务（供应商 / 提示词 / 连通性 / 生成）
// ---------------------------------------------------------------------------

/// 配置目录：供应商与提示词库持久化于此（yuzao-llm.json / yuzao-prompts.json）。
fn config_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录：{e}"))
}

#[tauri::command]
fn llm_list_providers(app: tauri::AppHandle) -> Result<llm::ProviderStore, String> {
    llm::ProviderStore::load(&config_dir(&app)?)
}

#[tauri::command]
fn llm_save_provider(app: tauri::AppHandle, provider: llm::Provider) -> Result<(), String> {
    let dir = config_dir(&app)?;
    let mut store = llm::ProviderStore::load(&dir)?;
    store.upsert(provider);
    store.save(&dir)
}

#[tauri::command]
fn llm_delete_provider(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = config_dir(&app)?;
    let mut store = llm::ProviderStore::load(&dir)?;
    store.providers.retain(|p| p.id != id);
    store.save(&dir)
}

#[tauri::command]
async fn llm_test_connection(app: tauri::AppHandle, provider_id: String) -> Result<llm::TestReport, String> {
    let dir = config_dir(&app)?;
    let store = llm::ProviderStore::load(&dir)?;
    let provider = store
        .find(&provider_id)
        .ok_or_else(|| format!("未找到供应商：{provider_id}"))?;
    Ok(llm::test_connection(provider).await)
}

/// 用指定提示词 + 供应商执行一次生成/修改，返回可直接应用的 FeatureSpec 或错误清单。
#[tauri::command]
async fn llm_run(app: tauri::AppHandle, req: llm::LlmRunRequest) -> Result<llm::GenerateOutcome, String> {
    let dir = config_dir(&app)?;
    let providers = llm::ProviderStore::load(&dir)?;
    let provider = providers
        .find(&req.provider_id)
        .ok_or_else(|| format!("未找到供应商：{}", req.provider_id))?;
    let prompt_store = prompts::PromptStore::load(&dir)?;
    let prompt = prompt_store
        .find(&req.prompt_id)
        .ok_or_else(|| format!("未找到提示词：{}", req.prompt_id))?;

    let system = llm::system_for(&prompt.instruction);
    let user = prompts::render(&prompt.template, &req.input, req.current_spec.as_deref());
    llm::run_llm(provider, &system, &user).await
}

#[tauri::command]
fn llm_list_prompts(app: tauri::AppHandle) -> Result<prompts::PromptStore, String> {
    prompts::PromptStore::load(&config_dir(&app)?)
}

#[tauri::command]
fn llm_save_prompt(app: tauri::AppHandle, prompt: prompts::Prompt) -> Result<(), String> {
    let dir = config_dir(&app)?;
    let mut store = prompts::PromptStore::load(&dir)?;
    store.upsert(prompt);
    store.save(&dir)
}

#[tauri::command]
fn llm_delete_prompt(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = config_dir(&app)?;
    let mut store = prompts::PromptStore::load(&dir)?;
    store.prompts.retain(|p| p.id != id);
    store.save(&dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            validate_spec,
            llm_list_providers,
            llm_save_provider,
            llm_delete_provider,
            llm_test_connection,
            llm_run,
            llm_list_prompts,
            llm_save_prompt,
            llm_delete_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
