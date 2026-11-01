//! 增量改参计划（G2/G3）。
//!
//! 只允许修改参数：按语义 id 定位特征 → 写入新值 → 计算下游 `Regenerate`
//! 闭包。结构字段（`sketch`/`on`）在 IR 层就没有对应的 [`EditParam`]，因此
//! 不存在"改错字段导致重建"的路径。

use std::collections::BTreeSet;

use serde::Serialize;

use crate::graph::DependencyGraph;
use crate::{EditParam, Feature, FeatureKind, FeatureSpec, SpecError};

/// 一条已接受的参数修改。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppliedEdit {
    pub feature_id: String,
    pub param: EditParam,
    /// `F003.dia` 形式的规范 target。
    pub target: String,
    pub old_value: f64,
    pub new_value: f64,
}

/// 改参计划。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditPlan {
    pub applied: Vec<AppliedEdit>,
    /// 含被改特征 + 全部传递下游，按建模顺序排列（交给增量 `Regenerate`）。
    pub regenerate: Vec<String>,
    /// 仅下游（不含被改特征），用于 UI 高亮"受影响特征"。
    pub downstream: Vec<String>,
}

/// 规划并应用 `spec.edits`。所有编辑先全量校验；任一失败则不产生部分结果。
pub fn plan_edits(spec: &FeatureSpec) -> Result<EditPlan, Vec<SpecError>> {
    let graph = DependencyGraph::build(&spec.features);
    let mut scratch = spec.clone();
    let mut applied = Vec::with_capacity(spec.edits.len());
    let mut errors = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for (index, edit) in spec.edits.iter().enumerate() {
        let target = format!("{}.{}", edit.feature_id, edit.param.suffix());
        if !seen.insert(target.clone()) {
            errors.push(SpecError::at(
                "FS163",
                format!("edits[{index}]"),
                format!("重复修改同一目标：{target}"),
            ));
            continue;
        }

        match apply_one(&mut scratch.features, edit) {
            Ok((old_value, feature)) => applied.push(AppliedEdit {
                feature_id: feature,
                param: edit.param,
                target,
                old_value,
                new_value: edit.value,
            }),
            Err(error) => errors.push(error),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let roots: Vec<String> = applied
        .iter()
        .map(|edit| edit.feature_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let regenerate = graph.closure(&roots);
    let downstream = graph.downstream(&roots);

    Ok(EditPlan {
        applied,
        regenerate,
        downstream,
    })
}

fn apply_one(
    features: &mut [Feature],
    edit: &crate::Edit,
) -> Result<(f64, String), SpecError> {
    let index = features
        .iter()
        .position(|feature| feature.id == edit.feature_id)
        .ok_or_else(|| {
            SpecError::at(
                "FS161",
                format!("edits.{}", edit.param.suffix()),
                format!("edit 目标特征不存在：{}", edit.feature_id),
            )
        })?;

    let feature = &mut features[index];
    if !EditParam::accepts(feature.kind.op()).contains(&edit.param) {
        return Err(SpecError::at(
            "FS162",
            format!("edits[{}]", edit.feature_id),
            format!(
                "参数 {} 不适用于 {} 特征 {}",
                edit.param.suffix(),
                feature.kind.op(),
                edit.feature_id
            ),
        ));
    }

    if !edit.value.is_finite() || edit.value <= 0.0 {
        return Err(SpecError::at(
            "FS140",
            format!("edits[{}].value", edit.feature_id),
            format!("edit 值必须为有限正数（得到 {}）", edit.value),
        ));
    }

    let old_value = read_param(feature, edit.param).ok_or_else(|| {
        SpecError::at(
            "FS164",
            format!("edits[{}]", edit.feature_id),
            format!("特征 {} 当前没有可读参数 {}", edit.feature_id, edit.param.suffix()),
        )
    })?;

    if !write_param(feature, edit.param, edit.value) {
        return Err(SpecError::at(
            "FS162",
            format!("edits[{}]", edit.feature_id),
            format!("参数写入失败：{}", edit.param.suffix()),
        ));
    }

    Ok((old_value, edit.feature_id.clone()))
}

fn read_param(feature: &Feature, param: EditParam) -> Option<f64> {
    match (&feature.kind, param) {
        (FeatureKind::Sketch { dims, .. }, EditParam::DimW) => Some(dims.w),
        (FeatureKind::Sketch { dims, .. }, EditParam::DimH) => Some(dims.h),
        (FeatureKind::Extrude { depth, .. }, EditParam::Depth) => Some(*depth),
        (FeatureKind::Hole { dia, .. }, EditParam::Dia) => Some(*dia),
        _ => None,
    }
}

fn write_param(feature: &mut Feature, param: EditParam, value: f64) -> bool {
    match (&mut feature.kind, param) {
        (FeatureKind::Sketch { dims, .. }, EditParam::DimW) => {
            dims.w = value;
            true
        }
        (FeatureKind::Sketch { dims, .. }, EditParam::DimH) => {
            dims.h = value;
            true
        }
        (FeatureKind::Extrude { depth, .. }, EditParam::Depth) => {
            *depth = value;
            true
        }
        (FeatureKind::Hole { dia, .. }, EditParam::Dia) => {
            *dia = value;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, FeatureSpec};

    fn example() -> FeatureSpec {
        parse(include_str!("../../featurespec.example.json")).expect("示例应通过校验")
    }

    fn with_edit(mut spec: FeatureSpec, feature_id: &str, param: EditParam, value: f64) -> FeatureSpec {
        // 示例文档自带 F003.dia 编辑；测试用「仅此一条」的干净基线。
        spec.edits.clear();
        spec.edits.push(crate::Edit {
            feature_id: feature_id.to_owned(),
            param,
            value,
        });
        spec
    }

    #[test]
    fn hole_edit_has_no_downstream() {
        let spec = with_edit(example(), "F003", EditParam::Dia, 7.0);
        let plan = plan_edits(&spec).unwrap();
        assert_eq!(plan.applied[0].old_value, 6.0);
        assert_eq!(plan.applied[0].new_value, 7.0);
        assert_eq!(plan.regenerate, vec!["F003".to_owned()]);
        assert!(plan.downstream.is_empty());
    }

    #[test]
    fn sketch_edit_regenerates_all_downstream() {
        let spec = with_edit(example(), "F001", EditParam::DimW, 120.0);
        let plan = plan_edits(&spec).unwrap();
        assert_eq!(
            plan.regenerate,
            vec!["F001".to_owned(), "F002".to_owned(), "F003".to_owned()]
        );
        assert_eq!(
            plan.downstream,
            vec!["F002".to_owned(), "F003".to_owned()]
        );
    }

    #[test]
    fn extrude_edit_targets_hole_only() {
        let spec = with_edit(example(), "F002", EditParam::Depth, 12.5);
        let plan = plan_edits(&spec).unwrap();
        assert_eq!(plan.downstream, vec!["F003".to_owned()]);
    }

    #[test]
    fn rejects_unknown_target_and_bad_values() {
        let spec = with_edit(example(), "F999", EditParam::Dia, 1.0);
        let errors = plan_edits(&spec).unwrap_err();
        assert_eq!(errors[0].code, "FS161");

        let spec = with_edit(example(), "F003", EditParam::Dia, -1.0);
        let errors = plan_edits(&spec).unwrap_err();
        assert_eq!(errors[0].code, "FS140");

        let spec = with_edit(example(), "F001", EditParam::Dia, 1.0);
        let errors = plan_edits(&spec).unwrap_err();
        assert_eq!(errors[0].code, "FS162");
    }

    #[test]
    fn rejects_duplicate_targets_without_mutating_input() {
        let mut spec = with_edit(example(), "F003", EditParam::Dia, 7.0);
        spec.edits.push(crate::Edit {
            feature_id: "F003".to_owned(),
            param: EditParam::Dia,
            value: 8.0,
        });
        let errors = plan_edits(&spec).unwrap_err();
        assert_eq!(errors[0].code, "FS163");
    }

    #[test]
    fn rejects_non_finite_edit_without_partial_application() {
        let spec = with_edit(example(), "F003", EditParam::Dia, f64::NAN);
        let errors = plan_edits(&spec).unwrap_err();
        assert_eq!(errors[0].code, "FS140");
    }

    #[test]
    fn multiple_edits_share_one_regenerate_closure() {
        let mut spec = with_edit(example(), "F001", EditParam::DimW, 120.0);
        spec.edits.clear();
        spec.edits.push(crate::Edit {
            feature_id: "F001".to_owned(),
            param: EditParam::DimW,
            value: 120.0,
        });
        spec.edits.push(crate::Edit {
            feature_id: "F002".to_owned(),
            param: EditParam::Depth,
            value: 14.0,
        });
        let plan = plan_edits(&spec).unwrap();
        // 下游闭包去重且按建模顺序，不因两次编辑重复执行。
        assert_eq!(
            plan.regenerate,
            vec!["F001".to_owned(), "F002".to_owned(), "F003".to_owned()]
        );
        assert_eq!(plan.downstream, vec!["F003".to_owned()]);
        assert_eq!(plan.applied.len(), 2);
    }
}
