//! FeatureSpec 离线工具链 CLI。
//!
//! 用法：
//! ```text
//! featurespec validate <spec.json>   # 校验，输出错误码清单；退出码 0/1
//! featurespec plan <spec.json> [--json]
//!                                    # 打印有序构建计划 + 改参重算闭包
//! featurespec schema                 # 打印内嵌 JSON Schema（单一来源）
//! ```
//!
//! 退出码：`0` 成功；`1` 校验/规划失败；`2` 用法或 IO 错误（便于 CI 区分）。

use std::process::ExitCode;

use featurespec::{build_plan, parse, plan_edits, FeatureSpec};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schema") => {
            println!("{}", featurespec::SCHEMA_JSON);
            ExitCode::SUCCESS
        }
        Some("validate") => {
            let Some(path) = args.get(1) else {
                eprintln!("用法: featurespec validate <spec.json>");
                return ExitCode::from(2);
            };
            let spec = match load_spec(path) {
                Ok(spec) => spec,
                Err(code) => return code,
            };
            println!(
                "[OK] {}: {} 个特征, {} 条编辑指令",
                spec.model,
                spec.features.len(),
                spec.edits.len()
            );
            ExitCode::SUCCESS
        }
        Some("plan") => {
            let paths: Vec<&String> = args[1..]
                .iter()
                .filter(|a| !a.starts_with("--"))
                .collect();
            let json = args.iter().any(|a| a == "--json");
            let Some(path) = paths.first() else {
                eprintln!("用法: featurespec plan <spec.json> [--json]");
                return ExitCode::from(2);
            };
            let spec = match load_spec(path) {
                Ok(spec) => spec,
                Err(code) => return code,
            };
            if json {
                print_json_plan(&spec)
            } else {
                print_human_plan(&spec)
            }
        }
        _ => {
            eprintln!("用法: featurespec <validate <spec.json> | plan <spec.json> [--json] | schema>");
            ExitCode::from(2)
        }
    }
}

/// 读取并校验 spec 文件；错误以退出码返回。
fn load_spec(path: &str) -> Result<FeatureSpec, ExitCode> {
    let Ok(json) = std::fs::read_to_string(path) else {
        eprintln!("[FS100] 无法读取文件: {path}");
        return Err(ExitCode::from(2));
    };
    // Windows 编辑器常带 UTF-8 BOM；serde_json 不接受 BOM，先剥离。
    let json = json.strip_prefix('\u{feff}').unwrap_or(&json);
    match parse(json) {
        Ok(spec) => Ok(spec),
        Err(errs) => {
            report_errors(&errs);
            Err(ExitCode::FAILURE)
        }
    }
}

fn report_errors(errs: &[featurespec::SpecError]) {
    eprintln!("[FAIL] 发现 {} 处校验错误:", errs.len());
    for e in errs {
        eprintln!(" - {e}");
    }
}

fn print_json_plan(spec: &FeatureSpec) -> ExitCode {
    let build = build_plan(spec);
    let edits = match plan_edits(spec) {
        Ok(plan) => plan,
        Err(errs) => {
            report_errors(&errs);
            return ExitCode::FAILURE;
        }
    };
    let payload = serde_json::json!({ "build": build, "edits": edits });
    println!("{}", serde_json::to_string_pretty(&payload).expect("计划可序列化"));
    ExitCode::SUCCESS
}

fn print_human_plan(spec: &FeatureSpec) -> ExitCode {
    let build = build_plan(spec);
    println!("模型 {} ({:?})", build.model, build.spec_type);
    println!("构建计划（按建模顺序）:");
    for step in &build.steps {
        let deps = if step.depends_on.is_empty() {
            String::new()
        } else {
            format!("  <- {}", step.depends_on.join(", "))
        };
        println!("  {}. {} {}{}", step.order, step.feature_id, step.op, deps);
    }

    let edit_plan = match plan_edits(spec) {
        Ok(plan) => plan,
        Err(errs) => {
            report_errors(&errs);
            return ExitCode::FAILURE;
        }
    };
    if edit_plan.applied.is_empty() {
        println!("改参计划: 无（edits 为空）");
        return ExitCode::SUCCESS;
    }
    println!("改参计划:");
    for edit in &edit_plan.applied {
        println!(
            "  {}: {} -> {}（仅改参数，Regenerate）",
            edit.target, edit.old_value, edit.new_value
        );
    }
    println!("重算闭包: {}", edit_plan.regenerate.join(", "));
    if !edit_plan.downstream.is_empty() {
        println!("受影响下游: {}", edit_plan.downstream.join(", "));
    }
    ExitCode::SUCCESS
}
