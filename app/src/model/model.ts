import * as THREE from "three";

export interface FeatureNode {
  id: string;
  op: string;
  label: string;
  params: Array<[string, string | number]>;
}

interface SpecFeature {
  id: string;
  op: string;
  plane?: string;
  profile?: string;
  dims?: { w?: number; h?: number };
  sketch?: string;
  depth?: number;
  side?: string;
  on?: string;
  dia?: number;
  pattern?: { type?: string; n?: number[]; pitch?: number[] };
}

interface SpecDoc {
  model?: string;
  type?: string;
  features?: SpecFeature[];
  edits?: Array<{ target: string; value: number }>;
}

export interface BuiltModel {
  group: THREE.Group;
  features: FeatureNode[];
  name: string;
}

/** 与 spec/examples/plate_with_holes.json 一致的默认预览件。 */
export const DEFAULT_SPEC = `{
  "model": "plate_with_holes",
  "type": "part",
  "features": [
    { "id": "F001", "op": "sketch",  "plane": "TOP", "profile": "rect",
      "dims": { "w": 120, "h": 80 } },
    { "id": "F002", "op": "extrude", "sketch": "F001", "depth": 12,
      "side": "symmetric" },
    { "id": "F003", "op": "hole",    "on": "F002.face[top]",
      "dia": 8.0, "pattern": { "type": "rect", "n": [2, 2], "pitch": [80, 40] } },
    { "id": "F004", "op": "hole",    "on": "F002.face[bottom]", "dia": 5.0 }
  ],
  "edits": [
    { "target": "F003.dia", "value": 9.0 },
    { "target": "F002.depth", "value": 14.0 }
  ]
}`;

const EDGE_COLOR = 0x46566b;

/** 由 FeatureSpec JSON 构建预览实体；解析失败时退回默认示例。 */
export function buildModelFromSpec(json: string): BuiltModel {
  let spec: SpecDoc;
  try {
    spec = JSON.parse(json) as SpecDoc;
    if (!Array.isArray(spec.features)) throw new Error("缺少 features");
  } catch {
    return buildModelFromSpec(DEFAULT_SPEC);
  }
  return buildModel(spec);
}

function buildModel(spec: SpecDoc): BuiltModel {
  // 新建的空白零件没有特征：返回空组，不要退化成默认板件。
  if (!spec.features?.length) {
    const empty = new THREE.Group();
    empty.name = spec.model ?? "model";
    return { group: empty, name: empty.name, features: [] };
  }

  const applyEdits = (list: SpecFeature[]): SpecFeature[] => {
    const edits = spec.edits ?? [];
    return list.map((f) => {
      const next = { ...f };
      for (const e of edits) {
        if (e.target === `${f.id}.dia`) next.dia = e.value;
        if (e.target === `${f.id}.depth`) next.depth = e.value;
        if (e.target === `${f.id}.dims.w`) next.dims = { ...next.dims, w: e.value };
        if (e.target === `${f.id}.dims.h`) next.dims = { ...next.dims, h: e.value };
      }
      return next;
    });
  };

  const features = applyEdits(spec.features ?? []);
  const sketch = features.find((f) => f.op === "sketch");
  const extrude = features.find((f) => f.op === "extrude");
  const holes = features.filter((f) => f.op === "hole");

  const w = sketch?.dims?.w ?? 120;
  const h = sketch?.dims?.h ?? 80;
  const depth = extrude?.depth ?? 12;

  const shape = new THREE.Shape();
  shape.moveTo(-w / 2, -h / 2);
  shape.lineTo(w / 2, -h / 2);
  shape.lineTo(w / 2, h / 2);
  shape.lineTo(-w / 2, h / 2);
  shape.closePath();

  const positions: Array<{ x: number; z: number; dia: number; id: string }> = [];
  for (const hole of holes) {
    const dia = hole.dia ?? 8;
    const n = hole.pattern?.n ?? [1, 1];
    const pitch = hole.pattern?.pitch ?? [0, 0];
    for (let i = 0; i < n[0]; i++) {
      for (let j = 0; j < n[1]; j++) {
        positions.push({
          x: (i - (n[0] - 1) / 2) * (pitch[0] ?? 0),
          z: (j - (n[1] - 1) / 2) * (pitch[1] ?? 0),
          dia,
          id: hole.id,
        });
      }
    }
  }
  for (const p of positions) {
    const path = new THREE.Path();
    path.absarc(p.x, p.z, p.dia / 2, 0, Math.PI * 2, true);
    shape.holes.push(path);
  }

  const geometry = new THREE.ExtrudeGeometry(shape, {
    depth,
    bevelEnabled: true,
    bevelThickness: 0.4,
    bevelSize: 0.4,
    bevelSegments: 1,
    curveSegments: 24,
  });
  // 拉伸方向由 +Z 转为 +Y（板料立在网格上）
  geometry.rotateX(-Math.PI / 2);
  geometry.computeVertexNormals();

  const material = new THREE.MeshStandardMaterial({
    color: 0x93a7c4,
    metalness: 0.35,
    roughness: 0.42,
  });
  const mesh = new THREE.Mesh(geometry, material);
  mesh.userData.featureId = extrude?.id ?? "F002";

  const edges = new THREE.LineSegments(
    new THREE.EdgesGeometry(geometry, 25),
    new THREE.LineBasicMaterial({ color: EDGE_COLOR })
  );
  edges.userData.featureId = extrude?.id ?? "F002";

  const group = new THREE.Group();
  group.name = spec.model ?? "model";
  group.add(mesh, edges);

  // 孔在板件网格里只是 shape.holes（空洞），没有独立几何体，拾取只能命中基体 F002。
  // 每个孔塞一个不可见的拾取代理：Raycaster 不看 visible，点击孔口即可定位到孔特征（G2）。
  // ponytail: 代理只解决「点到哪个特征」，不区分孔内壁/端面；需要面级拾取时改为按面分组材质。
  for (const p of positions) {
    const r = Math.max(p.dia / 2 - 0.15, p.dia / 4);
    const proxy = new THREE.Mesh(
      new THREE.CylinderGeometry(r, r, Math.max(depth - 0.2, depth * 0.8), 16),
      new THREE.MeshStandardMaterial({
        color: 0x2b6cb0,
        emissive: 0x2b6cb0,
        emissiveIntensity: 0.6,
        transparent: true,
        opacity: 0.5,
        depthWrite: false,
      })
    );
    proxy.position.set(p.x, depth / 2, -p.z);
    proxy.visible = false;
    proxy.userData.featureId = p.id;
    proxy.userData.pickProxy = true;
    group.add(proxy);
  }

  return {
    group,
    name: spec.model ?? "model",
    features: buildTree(features, w, h, depth),
  };
}

function buildTree(
  features: SpecFeature[],
  w: number,
  h: number,
  depth: number
): FeatureNode[] {
  return features.map((f): FeatureNode => {
    if (f.op === "sketch") {
      return {
        id: f.id,
        op: f.op,
        label: `草图 ${f.id.slice(1)}`,
        params: [
          ["轮廓", f.profile ?? "rect"],
          ["平面", f.plane ?? "TOP"],
          ["尺寸", `${w} × ${h}`],
        ],
      };
    }
    if (f.op === "extrude") {
      return {
        id: f.id,
        op: f.op,
        label: `拉伸 ${f.id.slice(1)}`,
        params: [
          ["草图", f.sketch ?? "-"],
          ["深度", depth],
          ["方向", f.side ?? "one_side"],
        ],
      };
    }
    if (f.op === "hole") {
      const n = f.pattern?.n ?? [1, 1];
      const pitch = f.pattern?.pitch ?? [0, 0];
      const count = n[0] * n[1];
      const params: Array<[string, string | number]> = [
        ["直径", f.dia ?? 0],
        ["位置", (f.on ?? "").replace("F002.face", "") || "顶面"],
      ];
      if (count > 1) {
        params.push(["阵列", `${n[0]} × ${n[1]}`], ["间距", `${pitch[0]} × ${pitch[1]}`]);
      }
      return { id: f.id, op: f.op, label: `孔 ${f.id.slice(1)}${count > 1 ? ` ×${count}` : ""}`, params };
    }
    return { id: f.id, op: f.op, label: `${f.op} ${f.id}`, params: [] };
  });
}
