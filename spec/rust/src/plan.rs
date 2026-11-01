//! 首次构建计划：把已验证的 FeatureSpec 翻译成有序内核指令。
//!
//! 真正的 Creo 调用在 `native/`；本模块只决定顺序、依赖与每个步骤后的
//! 重算点，供 Rust 调度层逐条下发并做错误回滚。

use serde::Serialize;

use crate::graph::DependencyGraph;
use crate::{FeatureSpec, SpecType};

/// 一个待创建的特征。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildStep {
    /// 1-based 创建顺序。
    pub order: usize,
    pub feature_id: String,
    pub op: &'static str,
    /// 必须先存在的特征。
    pub depends_on: Vec<String>,
    /// 该步骤后是否执行一次 `Regenerate`（当前仅在最后一步置真）。
    pub regenerate_after: bool,
}

/// 新建模型的有序构建计划。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildPlan {
    pub model: String,
    pub spec_type: SpecType,
    pub steps: Vec<BuildStep>,
}

/// 根据文档顺序与依赖图生成计划。
///
/// 调用方必须先通过 [`crate::parse`] 校验；本函数对悬空引用是总函数，不会 panic。
pub fn build_plan(spec: &FeatureSpec) -> BuildPlan {
    let graph = DependencyGraph::build(&spec.features);
    let last = spec.features.len().saturating_sub(1);
    let steps = spec
        .features
        .iter()
        .enumerate()
        .map(|(index, feature)| BuildStep {
            order: index + 1,
            feature_id: feature.id.clone(),
            op: feature.kind.op(),
            depends_on: graph.dependencies_of(&feature.id).to_vec(),
            regenerate_after: index == last,
        })
        .collect();

    BuildPlan {
        model: spec.model.clone(),
        spec_type: spec.spec_type,
        steps,
    }
}
