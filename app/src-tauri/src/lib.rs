// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, validate_spec])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
