//! FeatureSpec IR — 唯一写入 Creo 内核的输入（ADR-012）。
//!
//! 设计要点（与 `docs/ARCHITECTURE.md` §3–§4 对齐）：
//! * LLM 只输出本 crate 定义的受控 JSON，禁止直接产出 Toolkit API 调用/脚本；
//! * `features` 用于首次构建，`edits` 用于增量修改（G3：只改参数 + `Regenerate`，不重建）；
//! * 拓扑引用 `F002.face[top]` 由本 crate 解析为结构化 [`TopoRef`]，LLM 永不猜 face index；
//! * 校验**收集全部错误**而非快速失败，便于一次反馈给 LLM 修正；
//! * 所有长度单位毫米（mm）。
//!
//! 错误码约定（与 `docs/ARCHITECTURE.md` §6 错误分层衔接）：
//! * `FS1xx`：FeatureSpec 语法/语义校验错误（本 crate）；
//! * `FS2xx`：执行器能力约束（native 侧，如 v1 仅支持首个 sketch+extrude 对）。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 编译期内嵌的 JSON Schema（与 `spec/featurespec.schema.json` 单一来源）。
pub const SCHEMA_JSON: &str = include_str!("../../featurespec.schema.json");

// ---------------------------------------------------------------------------
// 子模块（调度层原语）
// ---------------------------------------------------------------------------

pub mod edit_plan;
pub mod graph;
pub mod idmap;
pub mod plan;

pub use edit_plan::{plan_edits, AppliedEdit, EditPlan};
pub use graph::DependencyGraph;
pub use idmap::{FeatureIdMap, FeatureRecord, IdMapError};
pub use plan::{build_plan, BuildPlan, BuildStep};

// ---------------------------------------------------------------------------
// 错误体系
// ---------------------------------------------------------------------------

/// 一条校验错误。`code` 为 `FS1xx` 系列错误码，`path` 指向出错位置（如 `features[1].depth`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError {
    pub code: &'static str,
    pub message: String,
    pub path: Option<String>,
}

impl SpecError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), path: None }
    }

    fn at(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), path: Some(path.into()) }
    }
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(p) => write!(f, "[{}] {} (at {})", self.code, self.message, p),
            None => write!(f, "[{}] {}", self.code, self.message),
        }
    }
}

impl std::error::Error for SpecError {}

// ---------------------------------------------------------------------------
// 反序列化中间结构（宽松字段全集 + deny_unknown_fields，随后做逐 op 语义校验）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpec {
    model: String,
    #[serde(rename = "type")]
    spec_type: String,
    features: Vec<RawFeature>,
    #[serde(default)]
    edits: Vec<RawEdit>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFeature {
    id: String,
    op: String,
    // sketch
    plane: Option<String>,
    profile: Option<String>,
    dims: Option<RawDims>,
    // extrude
    sketch: Option<String>,
    depth: Option<f64>,
    side: Option<String>,
    // hole
    on: Option<String>,
    dia: Option<f64>,
    pattern: Option<RawPattern>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDims {
    w: f64,
    h: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPattern {
    #[serde(rename = "type")]
    kind: String,
    n: [u32; 2],
    pitch: [f64; 2],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEdit {
    target: String,
    value: f64,
}

// ---------------------------------------------------------------------------
// 对外类型（规范化后的 IR）
// ---------------------------------------------------------------------------

/// 模型类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecType {
    Part,
    Assembly,
}

/// 基准面（Creo 标准 datum plane 名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Plane {
    #[serde(rename = "TOP")]
    Top,
    #[serde(rename = "FRONT")]
    Front,
    #[serde(rename = "RIGHT")]
    Right,
    #[serde(rename = "BOTTOM")]
    Bottom,
    #[serde(rename = "BACK")]
    Back,
    #[serde(rename = "LEFT")]
    Left,
}

/// 轮廓类型。v1 仅矩形。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Profile {
    #[serde(rename = "rect")]
    Rect,
}

/// 拉伸方向语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// 深度对称分布（例：depth=10 → 草绘面两侧各 5）。
    Symmetric,
    /// 自草绘面单向拉伸。
    OneSide,
}

/// 阵列类型。v1 仅矩形阵列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PatternKind {
    #[serde(rename = "rect")]
    Rect,
}

/// 草绘矩形尺寸（mm）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Dims {
    pub w: f64,
    pub h: f64,
}

/// 矩形阵列定义。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pattern {
    #[serde(rename = "type")]
    pub kind: PatternKind,
    /// 两方向实例数（每向 ≥2）。
    pub n: [u32; 2],
    /// 两方向间距（mm，>0）。
    pub pitch: [f64; 2],
}

/// 拓扑引用（由语义 id + 可选面名构成）。
///
/// 语法：`F002` 或 `F002.face[top]`。Display 还原为语法串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopoRef {
    pub feature_id: String,
    pub face: Option<String>,
}

impl fmt::Display for TopoRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.face {
            Some(face) => write!(f, "{}.face[{}]", self.feature_id, face),
            None => write!(f, "{}", self.feature_id),
        }
    }
}

impl Serialize for TopoRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// 特征节点。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Feature {
    pub id: String,
    #[serde(flatten)]
    pub kind: FeatureKind,
}

/// 特征类型（serde 以 `op` 字段作内部 tag，与 JSON 语法一致）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FeatureKind {
    Sketch { plane: Plane, profile: Profile, dims: Dims },
    Extrude { sketch: String, depth: f64, side: Side },
    Hole { on: TopoRef, dia: f64, pattern: Option<Pattern> },
}

impl FeatureKind {
    /// 返回该特征的 op 名称（"sketch"/"extrude"/"hole"）。
    pub fn op(&self) -> &'static str {
        match self {
            FeatureKind::Sketch { .. } => "sketch",
            FeatureKind::Extrude { .. } => "extrude",
            FeatureKind::Hole { .. } => "hole",
        }
    }
}

/// 可编辑参数（v1 全部为数值参数）。序列化名与 target 后缀一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EditParam {
    /// sketch 的 `dims.w`
    #[serde(rename = "dims.w")]
    DimW,
    /// sketch 的 `dims.h`
    #[serde(rename = "dims.h")]
    DimH,
    /// extrude 的 `depth`
    #[serde(rename = "depth")]
    Depth,
    /// hole 的 `dia`
    #[serde(rename = "dia")]
    Dia,
}

impl EditParam {
    /// 参数在 target 语法中的后缀形式。
    pub fn suffix(&self) -> &'static str {
        match self {
            EditParam::DimW => "dims.w",
            EditParam::DimH => "dims.h",
            EditParam::Depth => "depth",
            EditParam::Dia => "dia",
        }
    }

    fn accepts(op: &str) -> &'static [EditParam] {
        match op {
            "sketch" => &[EditParam::DimW, EditParam::DimH],
            "extrude" => &[EditParam::Depth],
            "hole" => &[EditParam::Dia],
            _ => &[],
        }
    }
}

/// 增量修改指令（G3：只改参数，不重建）。
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    /// 目标特征 id（已解析自 target 语法）。
    pub feature_id: String,
    pub param: EditParam,
    pub value: f64,
}

impl Serialize for Edit {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Edit", 2)?;
        s.serialize_field("target", &format!("{}.{}", self.feature_id, self.param.suffix()))?;
        s.serialize_field("value", &self.value)?;
        s.end()
    }
}

/// FeatureSpec 根对象。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureSpec {
    pub model: String,
    #[serde(rename = "type")]
    pub spec_type: SpecType,
    pub features: Vec<Feature>,
    pub edits: Vec<Edit>,
}

impl FeatureSpec {
    /// 按 id 查找特征。
    pub fn feature(&self, id: &str) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }
}

// ---------------------------------------------------------------------------
// 解析与校验
// ---------------------------------------------------------------------------

/// 解析并校验 FeatureSpec JSON 文本。**收集全部错误**，有错则返回完整清单。
pub fn parse(json: &str) -> Result<FeatureSpec, Vec<SpecError>> {
    let raw: RawSpec = serde_json::from_str(json)
        .map_err(|e| vec![SpecError::new("FS100", format!("JSON 解析失败: {e}"))])?;
    validate(raw)
}

/// 解析并校验已加载的 JSON 值（供调度层复用）。
pub fn parse_value(value: serde_json::Value) -> Result<FeatureSpec, Vec<SpecError>> {
    let raw: RawSpec = serde_json::from_value(value)
        .map_err(|e| vec![SpecError::new("FS100", format!("JSON 结构不合法: {e}"))])?;
    validate(raw)
}

/// 语义校验（跨字段规则）。语法层已由 serde `deny_unknown_fields` 把关。
fn validate(raw: RawSpec) -> Result<FeatureSpec, Vec<SpecError>> {
    let mut errs: Vec<SpecError> = Vec::new();

    // --- model -----------------------------------------------------------
    if !is_valid_model_name(&raw.model) {
        errs.push(SpecError::new(
            "FS101",
            format!(
                "model 名称不合法：{:?}（须字母开头，仅 [A-Za-z0-9_]，≤32 字符）",
                raw.model
            ),
        ));
    }

    // --- type -------------------------------------------------------------
    let spec_type = match raw.spec_type.as_str() {
        "part" => SpecType::Part,
        "assembly" => SpecType::Assembly,
        other => {
            errs.push(SpecError::new(
                "FS102",
                format!("未知 type：{other:?}（仅允许 part / assembly）"),
            ));
            SpecType::Part
        }
    };

    // --- features：逐条语法/取值校验 ----------------------------------------
    if raw.features.is_empty() {
        errs.push(SpecError::new(
            "FS123",
            "features 为空：FeatureSpec 必须至少描述一个特征（改参请使用 edits 并保留原 features）",
        ));
    }
    let mut feats: Vec<Feature> = Vec::with_capacity(raw.features.len());
    for (idx, rf) in raw.features.into_iter().enumerate() {
        let path = format!("features[{idx}]");
        if !is_valid_feature_id(&rf.id) {
            errs.push(SpecError::at(
                "FS110",
                &path,
                format!("特征 id 不合法：{:?}（须形如 F001：F + ≥3 位数字）", rf.id),
            ));
        }
        if feats.iter().any(|f: &Feature| f.id == rf.id) {
            errs.push(SpecError::at("FS111", &path, format!("重复的特征 id：{}", rf.id)));
        }

        let kind = match rf.op.as_str() {
            "sketch" => {
                if let Err(e) = require_fields(&rf, &["plane", "profile", "dims"], &path) {
                    errs.push(e);
                }
                if rf.depth.is_some() || rf.sketch.is_some() || rf.on.is_some() || rf.dia.is_some()
                {
                    errs.push(SpecError::at(
                        "FS121",
                        &path,
                        "sketch 不允许携带 depth/sketch/on/dia 字段",
                    ));
                }
                match (rf.plane.as_deref(), rf.profile.as_deref(), rf.dims) {
                    (Some(p), Some(pr), Some(d)) => {
                        let plane = match parse_plane(p) {
                            Ok(pl) => pl,
                            Err(()) => {
                                errs.push(SpecError::at(
                                    "FS133",
                                    format!("{path}.plane"),
                                    format!(
                                        "未知基准面：{p:?}（允许 TOP/FRONT/RIGHT/BOTTOM/BACK/LEFT）"
                                    ),
                                ));
                                Plane::Top
                            }
                        };
                        let profile = match pr {
                            "rect" => Profile::Rect,
                            other => {
                                errs.push(SpecError::at(
                                    "FS170",
                                    format!("{path}.profile"),
                                    format!("不支持的轮廓类型：{other:?}（v1 仅 rect）"),
                                ));
                                Profile::Rect
                            }
                        };
                        if !is_positive_finite(d.w) || !is_positive_finite(d.h) {
                            errs.push(SpecError::at(
                                "FS140",
                                format!("{path}.dims"),
                                format!(
                                    "轮廓尺寸必须为有限正数（得到 w={}, h={}）",
                                    d.w, d.h
                                ),
                            ));
                        }
                        FeatureKind::Sketch { plane, profile, dims: Dims { w: d.w, h: d.h } }
                    }
                    _ => FeatureKind::Sketch {
                        plane: Plane::Top,
                        profile: Profile::Rect,
                        dims: Dims { w: 1.0, h: 1.0 },
                    },
                }
            }
            "extrude" => {
                if let Err(e) = require_fields(&rf, &["sketch", "depth", "side"], &path) {
                    errs.push(e);
                }
                if rf.plane.is_some() || rf.profile.is_some() || rf.dims.is_some()
                    || rf.on.is_some() || rf.dia.is_some()
                {
                    errs.push(SpecError::at(
                        "FS121",
                        &path,
                        "extrude 不允许携带 plane/profile/dims/on/dia 字段",
                    ));
                }
                let sketch = rf.sketch.clone().unwrap_or_default();
                if let Some(d) = rf.depth {
                    if !is_positive_finite(d) {
                        errs.push(SpecError::at(
                            "FS140",
                            format!("{path}.depth"),
                            format!("拉伸深度必须为有限正数（得到 {d}）"),
                        ));
                    }
                }
                let side = match rf.side.as_deref() {
                    Some("symmetric") => Side::Symmetric,
                    Some("one_side") => Side::OneSide,
                    Some(other) => {
                        errs.push(SpecError::at(
                            "FS133",
                            format!("{path}.side"),
                            format!("未知 side：{other:?}（允许 symmetric / one_side）"),
                        ));
                        Side::Symmetric
                    }
                    None => Side::Symmetric,
                };
                FeatureKind::Extrude {
                    sketch,
                    depth: rf.depth.unwrap_or(1.0),
                    side,
                }
            }
            "hole" => {
                if let Err(e) = require_fields(&rf, &["on", "dia"], &path) {
                    errs.push(e);
                }
                if rf.plane.is_some() || rf.profile.is_some() || rf.dims.is_some()
                    || rf.sketch.is_some() || rf.depth.is_some()
                {
                    errs.push(SpecError::at(
                        "FS121",
                        &path,
                        "hole 不允许携带 plane/profile/dims/sketch/depth 字段",
                    ));
                }
                let on = match rf.on.as_deref() {
                    Some(s) => match parse_topo_ref(s) {
                        Ok(t) => t,
                        Err(()) => {
                            errs.push(SpecError::at(
                                "FS150",
                                format!("{path}.on"),
                                format!(
                                    "拓扑引用语法不合法：{s:?}（须形如 F002 或 F002.face[top]）"
                                ),
                            ));
                            TopoRef { feature_id: String::new(), face: None }
                        }
                    },
                    None => TopoRef { feature_id: String::new(), face: None },
                };
                if let Some(d) = rf.dia {
                    if !is_positive_finite(d) {
                        errs.push(SpecError::at(
                            "FS140",
                            format!("{path}.dia"),
                            format!("孔径必须为有限正数（得到 {d}）"),
                        ));
                    }
                }
                let pattern = rf.pattern.map(|p| {
                    let kind = if p.kind == "rect" {
                        PatternKind::Rect
                    } else {
                        errs.push(SpecError::at(
                            "FS142",
                            format!("{path}.pattern.type"),
                            format!("不支持的阵列类型：{:?}（v1 仅 rect）", p.kind),
                        ));
                        PatternKind::Rect
                    };
                    if p.n[0] < 2 || p.n[1] < 2 {
                        errs.push(SpecError::at(
                            "FS141",
                            format!("{path}.pattern.n"),
                            format!("阵列实例数必须 ≥ 2（得到 {:?}）", p.n),
                        ));
                    }
                    if !is_positive_finite(p.pitch[0]) || !is_positive_finite(p.pitch[1]) {
                        errs.push(SpecError::at(
                            "FS140",
                            format!("{path}.pattern.pitch"),
                            format!("阵列间距必须为有限正数（得到 {:?}）", p.pitch),
                        ));
                    }
                    Pattern { kind, n: p.n, pitch: p.pitch }
                });
                FeatureKind::Hole { on, dia: rf.dia.unwrap_or(1.0), pattern }
            }
            other => {
                errs.push(SpecError::at(
                    "FS122",
                    format!("{path}.op"),
                    format!("未知 op：{other:?}（允许 sketch / extrude / hole）"),
                ));
                continue;
            }
        };

        feats.push(Feature { id: rf.id, kind });
    }

    // --- 引用语义（在全部特征收集完后统一判） --------------------------------
    //
    // 规则：
    // * 只允许引用**更早**出现的特征（Creo 特征树只能向后构建，前向引用会形成环）；
    // * extrude.sketch 必须指向 sketch；
    // * hole.on 必须指向材料特征（v1 仅 extrude），且必须带 `.face[...]` 面名。
    let position = |id: &str| feats.iter().position(|f| f.id == id);
    for (idx, f) in feats.iter().enumerate() {
        let path = format!("features[{idx}]");
        match &f.kind {
            FeatureKind::Extrude { sketch, .. } => {
                if !is_valid_feature_id(sketch) {
                    errs.push(SpecError::at(
                        "FS130",
                        format!("{path}.sketch"),
                        format!("sketch 引用语法不合法：{sketch:?}"),
                    ));
                } else {
                    match position(sketch) {
                        None => errs.push(SpecError::at(
                            "FS131",
                            format!("{path}.sketch"),
                            format!("extrude 引用了不存在的 sketch：{sketch}"),
                        )),
                        Some(target_idx) if target_idx > idx => errs.push(SpecError::at(
                            "FS134",
                            format!("{path}.sketch"),
                            format!("extrude 引用了更晚出现的特征 {sketch}（只允许向后引用）"),
                        )),
                        Some(target_idx) => {
                            if feats[target_idx].kind.op() != "sketch" {
                                errs.push(SpecError::at(
                                    "FS132",
                                    format!("{path}.sketch"),
                                    format!(
                                        "extrude.sketch 必须引用 sketch 特征，但 {} 是 {}",
                                        sketch,
                                        feats[target_idx].kind.op()
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
            FeatureKind::Hole { on, .. } => {
                if !is_valid_feature_id(&on.feature_id) {
                    // 语法错误已由 FS150 报出，此处不再级联。
                    continue;
                }
                if on.face.is_none() {
                    errs.push(SpecError::at(
                        "FS152",
                        format!("{path}.on"),
                        format!(
                            "hole.on 必须指定面引用（{}.face[top]），仅给特征 id 无法定位落孔面",
                            on.feature_id
                        ),
                    ));
                }
                match position(&on.feature_id) {
                    None => errs.push(SpecError::at(
                        "FS151",
                        format!("{path}.on"),
                        format!("hole.on 引用了不存在的特征：{on}"),
                    )),
                    Some(target_idx) if target_idx > idx => errs.push(SpecError::at(
                        "FS134",
                        format!("{path}.on"),
                        format!("hole.on 引用了更晚出现的特征 {on}（只允许向后引用）"),
                    )),
                    Some(target_idx) => {
                        if feats[target_idx].kind.op() != "extrude" {
                            errs.push(SpecError::at(
                                "FS153",
                                format!("{path}.on"),
                                format!(
                                    "hole.on 必须落在材料特征（v1 仅 extrude）上，但 {} 是 {}",
                                    on.feature_id,
                                    feats[target_idx].kind.op()
                                ),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // --- edits -------------------------------------------------------------
    let find = |id: &str| feats.iter().find(|f| f.id == id);
    let mut edits = Vec::with_capacity(raw.edits.len());
    let mut seen_targets: Vec<String> = Vec::new();
    for (idx, re) in raw.edits.into_iter().enumerate() {
        let path = format!("edits[{idx}]");
        let parsed = parse_edit_target(&re.target);
        let (fid, param) = match parsed {
            Ok(v) => v,
            Err(code_msg) => {
                errs.push(SpecError::at("FS160", &path, code_msg));
                continue;
            }
        };
        let target = format!("{fid}.{}", param.suffix());
        if seen_targets.contains(&target) {
            errs.push(SpecError::at(
                "FS163",
                &path,
                format!("重复修改同一目标 {target}（同一参数只能出现一次）"),
            ));
        } else {
            seen_targets.push(target);
        }
        match find(&fid) {
            None => {
                errs.push(SpecError::at(
                    "FS161",
                    &path,
                    format!("edit 目标特征不存在：{fid}"),
                ));
                continue;
            }
            Some(f) => {
                let allowed = EditParam::accepts(f.kind.op());
                if !allowed.contains(&param) {
                    let allowed_s: Vec<&str> = allowed.iter().map(|p| p.suffix()).collect();
                    errs.push(SpecError::at(
                        "FS162",
                        &path,
                        format!(
                            "参数 {} 不适用于 {} 特征 {}（可用：{}）",
                            param.suffix(),
                            f.kind.op(),
                            fid,
                            allowed_s.join("/")
                        ),
                    ));
                    continue;
                }
            }
        }
        if !is_positive_finite(re.value) {
            errs.push(SpecError::at(
                "FS140",
                format!("{path}.value"),
                format!("edit 值必须为有限正数（得到 {}）", re.value),
            ));
            continue;
        }
        edits.push(Edit { feature_id: fid, param, value: re.value });
    }

    if errs.is_empty() {
        Ok(FeatureSpec { model: raw.model, spec_type, features: feats, edits })
    } else {
        Err(errs)
    }
}

// ---------------------------------------------------------------------------
// 基础语法工具
// ---------------------------------------------------------------------------

/// model 名：字母开头，仅 [A-Za-z0-9_]，≤32 字符。
fn is_valid_model_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && b[0].is_ascii_alphabetic()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// 特征 id：`F` + 至少 3 位数字（F001、F012、F1234…）。
fn is_valid_feature_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 4 && b[0] == b'F' && b[1..].iter().all(|c| c.is_ascii_digit())
}

/// 尺寸/参数取值：有限且严格为正。
///
/// `NaN`/`±inf` 会绕过 `v <= 0.0` 这类比较（`NaN <= 0.0 == false`），必须显式拦截，
/// 否则会带着非法值进入 Creo 内核。
fn is_positive_finite(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

/// 解析基准面名。
fn parse_plane(s: &str) -> Result<Plane, ()> {
    match s {
        "TOP" => Ok(Plane::Top),
        "FRONT" => Ok(Plane::Front),
        "RIGHT" => Ok(Plane::Right),
        "BOTTOM" => Ok(Plane::Bottom),
        "BACK" => Ok(Plane::Back),
        "LEFT" => Ok(Plane::Left),
        _ => Err(()),
    }
}

/// 解析拓扑引用 `F002` / `F002.face[top]`。
fn parse_topo_ref(s: &str) -> Result<TopoRef, ()> {
    let (fid, face) = match s.split_once(".face[") {
        Some((fid, rest)) => {
            if !rest.ends_with(']') {
                return Err(());
            }
            let face = &rest[..rest.len() - 1];
            if face.is_empty()
                || !face.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
            {
                return Err(());
            }
            (fid, Some(face.to_string()))
        }
        None => (s, None),
    };
    if !is_valid_feature_id(fid) {
        return Err(());
    }
    Ok(TopoRef { feature_id: fid.to_string(), face })
}

/// 解析编辑目标语法 `F003.dia` / `F001.dims.w`。
fn parse_edit_target(s: &str) -> Result<(String, EditParam), String> {
    let bad = |why: &str| format!("target 语法不合法：{s:?}（{why}）");
    let (fid, suffix) = match s.split_once('.') {
        Some(v) => v,
        None => return Err(bad("缺少参数后缀，如 F003.dia")),
    };
    if !is_valid_feature_id(fid) {
        return Err(bad("特征 id 须形如 F001"));
    }
    let param = match suffix {
        "dims.w" => EditParam::DimW,
        "dims.h" => EditParam::DimH,
        "depth" => EditParam::Depth,
        "dia" => EditParam::Dia,
        other => return Err(format!("未知参数后缀：{other:?}（允许 dims.w/dims.h/depth/dia）")),
    };
    Ok((fid.to_string(), param))
}

/// 检查必填字段（配合逐 op 校验给出可读错误）。
fn require_fields(rf: &RawFeature, names: &[&str], path: &str) -> Result<(), SpecError> {
    let missing: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| match *n {
            "plane" => rf.plane.is_none(),
            "profile" => rf.profile.is_none(),
            "dims" => rf.dims.is_none(),
            "sketch" => rf.sketch.is_none(),
            "depth" => rf.depth.is_none(),
            "side" => rf.side.is_none(),
            "on" => rf.on.is_none(),
            "dia" => rf.dia.is_none(),
            _ => false,
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(SpecError::at(
            "FS120",
            path,
            format!("op={} 缺少必填字段：{}", rf.op, missing.join(", ")),
        ))
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../featurespec.example.json");

    fn parse_ok(json: &str) -> FeatureSpec {
        parse(json).unwrap_or_else(|e| panic!("应校验通过，实际错误: {e:?}"))
    }

    fn codes(errs: &[SpecError]) -> Vec<&'static str> {
        errs.iter().map(|e| e.code).collect()
    }

    #[test]
    fn example_spec_parses() {
        let spec = parse_ok(EXAMPLE);
        assert_eq!(spec.model, "base_plate");
        assert_eq!(spec.spec_type, SpecType::Part);
        assert_eq!(spec.features.len(), 3);
        assert_eq!(spec.edits.len(), 1);
        assert_eq!(spec.edits[0].feature_id, "F003");
        assert_eq!(spec.edits[0].param, EditParam::Dia);
        assert_eq!(spec.edits[0].value, 7.0);
    }

    #[test]
    fn topo_ref_display_roundtrip() {
        let spec = parse_ok(EXAMPLE);
        match &spec.features[2].kind {
            FeatureKind::Hole { on, .. } => assert_eq!(on.to_string(), "F002.face[top]"),
            other => panic!("第三个特征应为 hole，实际 {other:?}"),
        }
    }

    #[test]
    fn serialize_roundtrip_keeps_semantics() {
        let spec = parse_ok(EXAMPLE);
        let json = serde_json::to_string_pretty(&spec).unwrap();
        let spec2 = parse_ok(&json);
        assert_eq!(spec, spec2);
    }

    #[test]
    fn duplicate_id_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F001","op":"extrude","sketch":"F001","depth":5,"side":"one_side"}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS111"]);
    }

    #[test]
    fn unknown_op_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"revolve"}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS122"]);
    }

    #[test]
    fn unknown_field_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10},"foo":1}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS100"]);
    }

    #[test]
    fn missing_required_field_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"extrude","sketch":"F001","side":"one_side"}]}"#,
        )
        .unwrap_err();
        assert!(codes(&errs).contains(&"FS120"));
    }

    #[test]
    fn op_specific_forbidden_field_rejected() {
        // sketch 携带 depth → FS121；但 depth 也参与 sketch 引用检查，因此只断言含 FS121
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10},"depth":3}]}"#,
        )
        .unwrap_err();
        assert!(codes(&errs).contains(&"FS121"));
    }

    #[test]
    fn extrude_unknown_sketch_ref_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F002","op":"extrude","sketch":"F999","depth":5,"side":"one_side"}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS131"]);
    }

    #[test]
    fn extrude_ref_to_non_sketch_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":5,"side":"one_side"},
              {"id":"F003","op":"extrude","sketch":"F002","depth":5,"side":"one_side"}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS132"]);
    }

    #[test]
    fn bad_topo_ref_grammar_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":5,"side":"one_side"},
              {"id":"F003","op":"hole","on":"F002.face[Top Face]","dia":6}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS150"]);
    }

    #[test]
    fn hole_unknown_ref_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":5,"side":"one_side"},
              {"id":"F003","op":"hole","on":"F009.face[top]","dia":6}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS151"]);
    }

    #[test]
    fn non_positive_values_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":0,"h":10}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":-5,"side":"one_side"},
              {"id":"F003","op":"hole","on":"F002.face[top]","dia":0}]}"#,
        )
        .unwrap_err();
        let c = codes(&errs);
        assert!(c.iter().all(|&x| x == "FS140"), "实际: {c:?}");
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn pattern_constraints_enforced() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":100,"h":60}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":10,"side":"symmetric"},
              {"id":"F003","op":"hole","on":"F002.face[top]","dia":6,
               "pattern":{"type":"polar","n":[0,2],"pitch":[80,0]}}]}"#,
        )
        .unwrap_err();
        let c = codes(&errs);
        assert!(c.contains(&"FS142"));
        assert!(c.contains(&"FS141"));
        assert!(c.contains(&"FS140"));
    }

    #[test]
    fn bad_model_name_rejected() {
        let errs = parse(
            r#"{"model":"1bad-name","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS101"]);
    }

    #[test]
    fn bad_spec_type_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"block","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS102"]);
    }

    #[test]
    fn bad_feature_id_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"feature1","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS110"]);
    }

    #[test]
    fn edit_target_rules_enforced() {
        let base = r#"{"model":"m","type":"part","features":[
          {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
          {"id":"F002","op":"extrude","sketch":"F001","depth":5,"side":"one_side"}],"#;
        // 未知参数后缀
        let errs = parse(&format!("{base}\"edits\":[{{\"target\":\"F001.dims.z\",\"value\":3}}]}}"))
            .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS160"]);
        // 参数与 op 不匹配
        let errs = parse(&format!("{base}\"edits\":[{{\"target\":\"F002.dia\",\"value\":3}}]}}"))
            .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS162"]);
        // 目标特征不存在
        let errs = parse(&format!("{base}\"edits\":[{{\"target\":\"F009.depth\",\"value\":3}}]}}"))
            .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS161"]);
        // 非正值
        let errs = parse(&format!("{base}\"edits\":[{{\"target\":\"F002.depth\",\"value\":0}}]}}"))
            .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS140"]);
        // 合法目标
        let spec = parse_ok(&format!("{base}\"edits\":[{{\"target\":\"F001.dims.w\",\"value\":20}}]}}"));
        assert_eq!(spec.edits[0].param, EditParam::DimW);
    }

    #[test]
    fn all_errors_collected_not_fail_fast() {
        let errs = parse(
            r#"{"model":"9bad","type":"wat","features":[
              {"id":"F001","op":"sketch","plane":"SIDEWAYS","profile":"triangle","dims":{"w":-1,"h":10}},
              {"id":"F001","op":"extrude","sketch":"F004","depth":-2,"side":"tilted"}]}"#,
        )
        .unwrap_err();
        assert!(errs.len() >= 8, "应收集全部错误，实际 {}: {:?}", errs.len(), errs);
    }

    #[test]
    fn forward_reference_rejected() {
        // extrude 引用更晚出现的 sketch：Creo 特征树无法前向引用
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"extrude","sketch":"F002","depth":5,"side":"one_side"},
              {"id":"F002","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS134"]);
    }

    #[test]
    fn hole_must_sit_on_material_face() {
        // 目标不是材料特征（v1 仅 extrude）
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F002","op":"hole","on":"F001.face[top]","dia":6}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS153"]);

        // 只给特征 id、不给面名：无法定位落孔面
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":5,"side":"one_side"},
              {"id":"F003","op":"hole","on":"F002","dia":6}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS152"]);
    }

    #[test]
    fn duplicate_edit_target_rejected() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}}],
              "edits":[{"target":"F001.dims.w","value":20},
                       {"target":"F001.dims.w","value":30}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS163"]);
    }

    #[test]
    fn non_finite_values_rejected_at_parse() {
        // JSON 本身不允许 NaN/inf；serde_json 对超出 f64 范围的字面量直接报 FS100。
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":1e999,"h":10}}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS100"]);
    }

    #[test]
    fn empty_document_rejected() {
        let errs = parse(r#"{"model":"m","type":"part","features":[]}"#).unwrap_err();
        assert_eq!(codes(&errs), vec!["FS123"]);
    }

    #[test]
    fn pattern_requires_at_least_two_per_direction() {
        let errs = parse(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":100,"h":60}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":10,"side":"symmetric"},
              {"id":"F003","op":"hole","on":"F002.face[top]","dia":6,
               "pattern":{"type":"rect","n":[1,2],"pitch":[80,40]}}]}"#,
        )
        .unwrap_err();
        assert_eq!(codes(&errs), vec!["FS141"]);
    }

    #[test]
    fn side_is_preserved_in_ir() {
        let spec = parse_ok(EXAMPLE);
        match &spec.features[1].kind {
            FeatureKind::Extrude { side, depth, .. } => {
                assert_eq!(*side, Side::Symmetric);
                assert_eq!(*depth, 10.0);
            }
            other => panic!("第二个特征应为 extrude，实际 {other:?}"),
        }
        let one_side = parse_ok(
            r#"{"model":"m","type":"part","features":[
              {"id":"F001","op":"sketch","plane":"TOP","profile":"rect","dims":{"w":10,"h":10}},
              {"id":"F002","op":"extrude","sketch":"F001","depth":5,"side":"one_side"}]}"#,
        );
        match &one_side.features[1].kind {
            FeatureKind::Extrude { side, .. } => assert_eq!(*side, Side::OneSide),
            other => panic!("第二个特征应为 extrude，实际 {other:?}"),
        }
    }

    #[test]
    fn embedded_schema_is_valid_json() {
        let v: serde_json::Value = serde_json::from_str(SCHEMA_JSON).expect("内嵌 Schema 必须是合法 JSON");
        assert_eq!(v["$schema"], "https://json-schema.org/draft/2020-12/schema");
    }

    /// 防漂移：Schema 中影响语义的关键约束必须与校验器一致。
    #[test]
    fn embedded_schema_matches_validator_rules() {
        let v: serde_json::Value = serde_json::from_str(SCHEMA_JSON).unwrap();

        assert_eq!(v["additionalProperties"], false, "根对象必须拒绝未知字段");
        assert_eq!(v["properties"]["features"]["minItems"], 1, "features 不得为空");
        assert_eq!(
            v["$defs"]["pattern"]["properties"]["n"]["items"]["minimum"], 2,
            "pattern.n 每向实例数 ≥ 2"
        );
        assert!(
            v["$defs"]["hole"]["properties"]["on"]["pattern"]
                .as_str()
                .unwrap()
                .contains("\\.face\\["),
            "hole.on 必须要求面名，如 F002.face[top]"
        );
        assert_eq!(
            v["$defs"]["extrude"]["properties"]["side"]["enum"],
            serde_json::json!(["symmetric", "one_side"])
        );
        assert_eq!(
            v["$defs"]["edit"]["properties"]["value"]["exclusiveMinimum"],
            serde_json::json!(0)
        );
    }
}
