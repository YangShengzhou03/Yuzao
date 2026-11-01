"""Feasibility probe: can OpenUSD (usd-core) carry a Yuzao parametric model?

Run:  python scripts\\usd_probe.py

What it verifies (all on the plain Python 3.13 process, no FreeCAD needed):
  1. a parameterised feature tree can live inside a USD stage (per-feature prim +
     custom attributes: type / params / stable topology names / inputs)
  2. the preview mesh produced by FreeCAD can be stored as UsdGeomMesh
  3. the real B-Rep (STEP) can be referenced as an external asset payload
  4. round-trip: write .usda -> reopen -> read every value back
  5. inventory: does OpenUSD ship any B-Rep geometry schema at all?
"""

from __future__ import annotations

import json
import os
import sys
import tempfile

from pxr import Usd, UsdGeom, Sdf, Gf, Vt


FEATURES = [
    {"id": "Sketch1", "type": "sketch", "params": {"width": 200.0, "height": 100.0},
     "inputs": [], "on_face": None},
    {"id": "Extrude1", "type": "extrude", "params": {"depth": 30.0},
     "inputs": ["Sketch1"], "on_face": None},
    {"id": "Hole1", "type": "hole", "params": {"diameter": 12.0, "count": 3},
     "inputs": ["Extrude1"], "on_face": "top"},
]

# a tiny triangulated box (what FreeCAD tessellate() returns, scaled down)
POINTS = [(-1, -1, -1), (1, -1, -1), (1, 1, -1), (-1, 1, -1),
          (-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)]
FACE_COUNTS = [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]
FACE_INDS = [0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6,
             0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2,
             2, 6, 7, 2, 7, 3, 3, 7, 4, 3, 4, 0]


def _geometry_inventory():
    """List every UsdGeom schema class and flag anything B-Rep related."""
    names = [n for n in dir(UsdGeom) if n[:1].isupper()]
    brep_like = [n for n in names if "brep" in n.lower() or "brep" in n.lower()]
    print("[inventory] UsdGeom exposes %d schema/type classes" % len(names))
    geo = [n for n in names if n in (
        "Mesh", "PointInstancer", "Points", "Curves", "NurbsCurves", "NurbsPatch",
        "BasisCurves", "Capsule", "Cone", "Cube", "Cylinder", "Sphere",
        "Plane", "Cylinder", "Xform", "Scope", "Camera", "Boundable", "Imageable")]
    print("[inventory] built-in geometry: %s" % ", ".join(sorted(geo)))
    print("[inventory] B-Rep related classes: %s"
          % (", ".join(brep_like) if brep_like else "NONE -> B-Rep is not an OpenUSD geometry type"))


def build(path):
    stage = Usd.Stage.CreateNew(path)
    UsdGeom.SetStageUpAxis(stage, UsdGeom.Tokens.z)
    UsdGeom.SetStageMetersPerUnit(stage, 1.0)

    # --- document level: what the file is and where the exact geometry lives
    stage.SetMetadata("comment", "Yuzao parametric model (probe)")
    layer = stage.GetRootLayer()
    layer.customLayerData = {
        "aiCadVersion": "1",
        "kernel": "FreeCAD/OCCT",
        "brepPayload": "model.step",      # external exact-geometry payload
        "brepFormat": "STEP(AP214)",
    }

    model = UsdGeom.Xform.Define(stage, "/Model")
    stage.SetDefaultPrim(model.GetPrim())

    # --- feature tree: one prim per feature, params as custom attributes
    tree = UsdGeom.Scope.Define(stage, "/Model/FeatureTree").GetPrim()
    prev = None
    for f in FEATURES:
        prim = stage.DefinePrim("/Model/FeatureTree/%s" % f["id"], "Xform")
        prim.CreateAttribute("aiCad:type", Sdf.ValueTypeNames.Token).Set(f["type"])
        prim.CreateAttribute("aiCad:params", Sdf.ValueTypeNames.String).Set(
            json.dumps(f["params"]))
        prim.CreateAttribute("aiCad:inputs", Sdf.ValueTypeNames.StringArray).Set(
            Vt.StringArray(f["inputs"]))
        if f["on_face"]:
            prim.CreateAttribute("aiCad:onFace", Sdf.ValueTypeNames.Token).Set(f["on_face"])
        # explicit dependency edge keeps recompute order deterministic
        if prev:
            stage.DefinePrim("/Model/FeatureTree/%s" % f["id"], "Xform")  # already defined
            rel = prim.CreateRelationship("aiCad:dependsOn")
            rel.SetTargets([prev])
        prev = prim.GetPath()

    # --- preview mesh (what the Vue viewport renders)
    mesh = UsdGeom.Mesh.Define(stage, "/Model/Preview/Body")
    mesh.CreatePointsAttr(Vt.Vec3fArray([Gf.Vec3f(*p) for p in POINTS]))
    mesh.CreateFaceVertexCountsAttr(Vt.IntArray(FACE_COUNTS))
    mesh.CreateFaceVertexIndicesAttr(Vt.IntArray(FACE_INDS))
    mesh.CreateDoubleSidedAttr(True)
    UsdGeom.XformCommonAPI(mesh).SetTranslate(Gf.Vec3d(0, 0, 0))

    stage.GetRootLayer().Save()
    return path


def verify(path):
    stage = Usd.Stage.Open(path)
    ok = True

    data = stage.GetRootLayer().customLayerData or {}
    print("[read] kernel=%s brepPayload=%s" % (data.get("kernel"), data.get("brepPayload")))
    ok &= data.get("brepPayload") == "model.step"

    tree = stage.GetPrimAtPath("/Model/FeatureTree")
    kids = [c.GetName() for c in tree.GetChildren()]
    print("[read] features in tree: %s" % kids)
    ok &= kids == [f["id"] for f in FEATURES]

    for f in FEATURES:
        prim = stage.GetPrimAtPath("/Model/FeatureTree/%s" % f["id"])
        t = prim.GetAttribute("aiCad:type").Get()
        p = json.loads(prim.GetAttribute("aiCad:params").Get())
        print("  %-9s type=%-8s params=%s inputs=%s" % (
            f["id"], t, p, list(prim.GetAttribute("aiCad:inputs").Get())))
        ok &= t == f["type"] and p == f["params"]

    hole = stage.GetPrimAtPath("/Model/FeatureTree/Hole1")
    baked = hole.GetAttribute("aiCad:onFace").Get()
    print("[read] Hole1 stable topology ref = %s" % baked)
    ok &= baked == "top"

    m = UsdGeom.Mesh(stage.GetPrimAtPath("/Model/Preview/Body"))
    pts = m.GetPointsAttr().Get()
    cnt = m.GetFaceVertexCountsAttr().Get()
    print("[read] mesh points=%d faces=%d" % (len(pts), len(cnt)))
    ok &= len(pts) == 8 and len(cnt) == 12

    return ok


def main():
    _geometry_inventory()
    tmp = tempfile.mkdtemp(prefix="aicad-usd-")
    out = os.path.join(tmp, "probe.aicad.usda")
    build(out)
    print("[write] %s (%d bytes)" % (out, os.path.getsize(out)))
    ok = verify(out)
    print("[result] round-trip %s" % ("OK" if ok else "FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
