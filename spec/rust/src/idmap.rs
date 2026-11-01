//! 语义 id ↔ Creo 内部 id 的持久化映射。
//!
//! 语义引用 `F002.face[top]` 是 LLM 唯一可用的拓扑句柄；每次内核创建/导出后
//! 由调度层维护本映射，供后续改参和画布拾取反查。映射可序列化到 `.usda`
//! sidecar，应用重启后无需重新探测。
//!
//! 真值仍在 Creo；本表只是 id 翻译层（ADR-011 / ADR-012）。

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::TopoRef;

/// 单个特征的绑定记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRecord {
    /// `ProFeature.id` 返回值。
    pub internal_id: i64,
    /// 模型树显示名。
    pub name: String,
    /// 创建该节点的 FeatureSpec `op`。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub op: String,
}

/// 双向映射表（JSON 可持久化）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureIdMap {
    #[serde(default)]
    features: BTreeMap<String, FeatureRecord>,
    /// 以 `TopoRef` 的规范字符串为键（避免把 face index 泄漏给语义层）。
    #[serde(default)]
    topology: BTreeMap<String, i64>,
}

impl FeatureIdMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_feature(
        &mut self,
        feature: impl Into<String>,
        internal_id: i64,
        name: impl Into<String>,
        op: impl Into<String>,
    ) -> Result<(), IdMapError> {
        let feature = feature.into();
        if internal_id <= 0 {
            return Err(IdMapError::InvalidInternalId(internal_id));
        }
        if self.features.contains_key(&feature) {
            return Err(IdMapError::FeatureAlreadyBound(feature));
        }
        if let Some(existing) = self.feature_for_internal(internal_id) {
            return Err(IdMapError::InternalIdConflict {
                internal_id,
                existing: existing.to_owned(),
            });
        }
        self.features.insert(
            feature,
            FeatureRecord {
                internal_id,
                name: name.into(),
                op: op.into(),
            },
        );
        Ok(())
    }

    pub fn bind_topology(
        &mut self,
        reference: TopoRef,
        internal_id: i64,
    ) -> Result<(), IdMapError> {
        if internal_id <= 0 {
            return Err(IdMapError::InvalidInternalId(internal_id));
        }
        if !self.features.contains_key(&reference.feature_id) {
            return Err(IdMapError::FeatureNotBound(reference.feature_id));
        }
        let key = reference.to_string();
        if self.topology.contains_key(&key) {
            return Err(IdMapError::TopologyAlreadyBound(key));
        }
        if let Some(existing) = self.topology_for_internal(internal_id) {
            return Err(IdMapError::TopologyIdConflict {
                internal_id,
                existing: existing.to_owned(),
            });
        }
        self.topology.insert(key, internal_id);
        Ok(())
    }

    pub fn feature_record(&self, feature: &str) -> Option<&FeatureRecord> {
        self.features.get(feature)
    }

    pub fn resolve_feature(&self, feature: &str) -> Option<i64> {
        self.features.get(feature).map(|record| record.internal_id)
    }

    pub fn resolve_topology(&self, reference: &TopoRef) -> Option<i64> {
        self.topology.get(&reference.to_string()).copied()
    }

    pub fn feature_for_internal(&self, internal_id: i64) -> Option<&str> {
        self.features
            .iter()
            .find(|(_, record)| record.internal_id == internal_id)
            .map(|(feature, _)| feature.as_str())
    }

    pub fn topology_for_internal(&self, internal_id: i64) -> Option<&str> {
        self.topology
            .iter()
            .find(|(_, id)| **id == internal_id)
            .map(|(reference, _)| reference.as_str())
    }

    /// 画布拾取（Creo 内部 id）→ 语义 target 字符串，供 `{target,param}` 回传。
    pub fn pick_to_target(&self, internal_id: i64) -> Option<String> {
        self.topology_for_internal(internal_id).map(ToOwned::to_owned)
    }

    pub fn remove_feature(&mut self, feature: &str) -> Option<FeatureRecord> {
        let record = self.features.remove(feature);
        self.topology
            .retain(|reference, _| !reference.starts_with(&format!("{feature}.")));
        record
    }

    pub fn len(&self) -> usize {
        self.features.len()
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }
}

/// 绑定失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdMapError {
    InvalidInternalId(i64),
    FeatureAlreadyBound(String),
    FeatureNotBound(String),
    InternalIdConflict {
        internal_id: i64,
        existing: String,
    },
    TopologyAlreadyBound(String),
    TopologyIdConflict {
        internal_id: i64,
        existing: String,
    },
}

impl fmt::Display for IdMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInternalId(id) => write!(f, "Creo 内部 id 必须为正整数，收到 {id}"),
            Self::FeatureAlreadyBound(feature) => write!(f, "特征 {feature} 已绑定"),
            Self::FeatureNotBound(feature) => {
                write!(f, "特征 {feature} 尚未绑定，不能绑定其拓扑")
            }
            Self::InternalIdConflict {
                internal_id,
                existing,
            } => write!(f, "Creo 特征 id {internal_id} 已绑定到 {existing}"),
            Self::TopologyAlreadyBound(reference) => write!(f, "拓扑引用 {reference} 已绑定"),
            Self::TopologyIdConflict {
                internal_id,
                existing,
            } => write!(f, "Creo 拓扑 id {internal_id} 已绑定到 {existing}"),
        }
    }
}

impl std::error::Error for IdMapError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> TopoRef {
        TopoRef {
            feature_id: "F002".to_owned(),
            face: Some("top".to_owned()),
        }
    }

    #[test]
    fn binds_and_resolves_both_directions() {
        let mut map = FeatureIdMap::new();
        map.bind_feature("F001", 10, "Sketch", "sketch").unwrap();
        map.bind_feature("F002", 20, "Extrude", "extrude").unwrap();
        map.bind_topology(reference(), 4).unwrap();

        assert_eq!(map.resolve_feature("F002"), Some(20));
        assert_eq!(map.feature_for_internal(10), Some("F001"));
        assert_eq!(map.resolve_topology(&reference()), Some(4));
        assert_eq!(map.pick_to_target(4).as_deref(), Some("F002.face[top]"));
    }

    #[test]
    fn rejects_duplicates_and_conflicts() {
        let mut map = FeatureIdMap::new();
        map.bind_feature("F001", 10, "Sketch", "sketch").unwrap();
        assert_eq!(
            map.bind_feature("F001", 11, "Again", "sketch"),
            Err(IdMapError::FeatureAlreadyBound("F001".to_owned()))
        );
        assert!(matches!(
            map.bind_feature("F002", 10, "Extrude", "extrude"),
            Err(IdMapError::InternalIdConflict { .. })
        ));
        assert_eq!(
            map.bind_feature("F002", 0, "Extrude", "extrude"),
            Err(IdMapError::InvalidInternalId(0))
        );

        map.bind_feature("F002", 20, "Extrude", "extrude").unwrap();
        map.bind_topology(reference(), 4).unwrap();
        assert_eq!(
            map.bind_topology(reference(), 5),
            Err(IdMapError::TopologyAlreadyBound("F002.face[top]".to_owned()))
        );
    }

    #[test]
    fn topology_requires_bound_feature() {
        let mut map = FeatureIdMap::new();
        assert_eq!(
            map.bind_topology(reference(), 4),
            Err(IdMapError::FeatureNotBound("F002".to_owned()))
        );
    }

    #[test]
    fn remove_feature_cleans_topology() {
        let mut map = FeatureIdMap::new();
        map.bind_feature("F002", 20, "Extrude", "extrude").unwrap();
        map.bind_topology(reference(), 4).unwrap();
        map.remove_feature("F002");
        assert!(map.resolve_topology(&reference()).is_none());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn json_roundtrip_preserves_mapping() {
        let mut map = FeatureIdMap::new();
        map.bind_feature("F001", 10, "Sketch", "sketch").unwrap();
        map.bind_feature("F002", 20, "Extrude", "extrude").unwrap();
        map.bind_topology(reference(), 4).unwrap();

        let json = map.to_json_pretty().unwrap();
        let back = FeatureIdMap::from_json(&json).unwrap();
        assert_eq!(back, map);
    }
}
