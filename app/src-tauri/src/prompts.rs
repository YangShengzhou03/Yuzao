//! 内置提示词库（ROADMAP P5.1）。
//!
//! 一条提示词 = 任务指令（`instruction`）+ 模板（`template`）。模板中的
//! `{input}` / `{spec}` 占位符在调用时替换。系统提示词由 [`crate::llm::system_for`]
//! 统一注入 FeatureSpec 规则，用户只需写「任务本身」。
//!
//! 库持久化到 `app_config_dir/yuzao-prompts.json`；首次运行时写入内置项。

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub template: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptStore {
    #[serde(default)]
    pub prompts: Vec<Prompt>,
}

impl PromptStore {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("yuzao-prompts.json");
        if !path.exists() {
            let store = Self { prompts: defaults() };
            store.save(dir)?;
            return Ok(store);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("读取提示词失败: {e}"))
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("yuzao-prompts.json"), text).map_err(|e| e.to_string())
    }

    /// 按 id 插入或覆盖。
    pub fn upsert(&mut self, prompt: Prompt) {
        match self.prompts.iter_mut().find(|p| p.id == prompt.id) {
            Some(slot) => *slot = prompt,
            None => self.prompts.push(prompt),
        }
    }

    pub fn find(&self, id: &str) -> Option<&Prompt> {
        self.prompts.iter().find(|p| p.id == id)
    }
}

/// 内置提示词：覆盖「从零生成」与「在现有模型上改」两条主路径。
pub fn defaults() -> Vec<Prompt> {
    vec![
        Prompt {
            id: "gen_part".into(),
            name: "生成零件".into(),
            instruction: "根据用户的自然语言描述，从零生成一个完整的 FeatureSpec（features 必填，edits 可选）。".into(),
            template: "零件描述：{input}".into(),
        },
        Prompt {
            id: "edit_part".into(),
            name: "修改特征".into(),
            instruction: "保留当前 FeatureSpec 的 features，按用户要求修改参数（用 edits 表达）；如需新增特征也可加入 features。".into(),
            template: "当前 FeatureSpec：\n{spec}\n\n修改要求：{input}".into(),
        },
    ]
}

/// 替换模板占位符；`{spec}` 缺省时替换为空串。
pub fn render(template: &str, input: &str, spec: Option<&str>) -> String {
    template.replace("{input}", input).replace("{spec}", spec.unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_replaces_placeholders() {
        let t = "当前：{spec}\n要求：{input}";
        assert_eq!(render(t, "孔径改为 12", Some("{\"model\":\"m\"}")), "当前：{\"model\":\"m\"}\n要求：孔径改为 12");
        assert_eq!(render(t, "x", None), "当前：\n要求：x");
    }

    #[test]
    fn defaults_cover_generate_and_edit() {
        let d = defaults();
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|p| p.id == "gen_part"));
        assert!(d.iter().any(|p| p.id == "edit_part"));
    }

    #[test]
    fn upsert_replaces_by_id() {
        let mut store = PromptStore::default();
        store.upsert(Prompt { id: "a".into(), name: "A".into(), instruction: "i".into(), template: "t".into() });
        store.upsert(Prompt { id: "a".into(), name: "A2".into(), instruction: "i".into(), template: "t".into() });
        assert_eq!(store.prompts.len(), 1);
        assert_eq!(store.prompts[0].name, "A2");
    }
}
