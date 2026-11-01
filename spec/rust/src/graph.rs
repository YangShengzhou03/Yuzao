//! 特征依赖图与下游重算闭包。
//!
//! G2/G3 要求改参后只重算下游、且特征 id/树结构不变；本模块从 IR 引用
//! （`extrude.sketch`、`hole.on`）构建 DAG，并给出**按建模顺序**排序的
//! 下游集合，可直接交给 `ProSolidRegenerate` 之外的增量更新流程。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{Feature, FeatureKind};

/// 由特征 id 索引的依赖图。重复 id 会被合并（校验器另行报错）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    order: Vec<String>,
    dependencies: BTreeMap<String, Vec<String>>,
    dependents: BTreeMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn build(features: &[Feature]) -> Self {
        let mut graph = Self::default();
        for feature in features {
            let id = feature.id.clone();
            graph.order.push(id.clone());
            let deps = match &feature.kind {
                FeatureKind::Extrude { sketch, .. } => vec![sketch.clone()],
                FeatureKind::Hole { on, .. } => vec![on.feature_id.clone()],
                FeatureKind::Sketch { .. } => Vec::new(),
            };
            let entries = graph.dependencies.entry(id.clone()).or_default();
            for dependency in deps {
                if !entries.contains(&dependency) {
                    entries.push(dependency.clone());
                    graph
                        .dependents
                        .entry(dependency)
                        .or_default()
                        .push(id.clone());
                }
            }
            graph.dependents.entry(id).or_default();
        }
        graph
    }

    pub fn order(&self) -> &[String] {
        &self.order
    }

    pub fn dependencies_of(&self, id: &str) -> &[String] {
        self.dependencies.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn dependents_of(&self, id: &str) -> &[String] {
        self.dependents.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn contains(&self, id: &str) -> bool {
        self.dependencies.contains_key(id)
    }

    /// 传递下游（不含 roots），按建模顺序返回。
    pub fn downstream(&self, roots: &[String]) -> Vec<String> {
        let mut visited: BTreeSet<String> = roots.iter().cloned().collect();
        let mut queue: VecDeque<String> = roots.iter().cloned().collect();

        while let Some(current) = queue.pop_front() {
            for dependent in self.dependents_of(&current) {
                if visited.insert(dependent.clone()) {
                    queue.push_back(dependent.clone());
                }
            }
        }

        self.order
            .iter()
            .filter(|id| visited.contains(*id) && !roots.contains(id))
            .cloned()
            .collect()
    }

    /// 含 roots 的完整闭包（UI/调试用）。
    pub fn closure(&self, roots: &[String]) -> Vec<String> {
        let mut set: BTreeSet<String> = roots.iter().cloned().collect();
        set.extend(self.downstream(roots));
        self.order
            .iter()
            .filter(|id| set.contains(*id))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn spec() -> crate::FeatureSpec {
        parse(include_str!("../../featurespec.example.json")).expect("示例应通过校验")
    }

    #[test]
    fn downstream_is_transitive_and_ordered() {
        let spec = spec();
        let graph = DependencyGraph::build(&spec.features);
        assert_eq!(
            graph.downstream(&["F001".to_owned()]),
            vec!["F002".to_owned(), "F003".to_owned()]
        );
        assert!(graph.downstream(&["F003".to_owned()]).is_empty());
        assert_eq!(
            graph.closure(&["F002".to_owned()]),
            vec!["F002".to_owned(), "F003".to_owned()]
        );
        assert_eq!(graph.order().len(), 3);
    }

    #[test]
    fn empty_and_duplicate_edges_are_safe() {
        let spec = spec();
        let graph = DependencyGraph::build(&spec.features);
        assert!(graph.contains("F001"));
        assert!(!graph.contains("F999"));
        assert_eq!(graph.dependencies_of("F002"), &["F001".to_owned()]);
        assert_eq!(
            graph.dependents_of("F002"),
            &["F003".to_owned()]
        );
    }
}
