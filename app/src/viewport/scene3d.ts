import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { ViewHelper } from "three/addons/helpers/ViewHelper.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

/** 标准视图方向（three.js 为 Y 向上，故「顶视」看向 -Y）。 */
export type ViewName =
  | "iso"
  | "front"
  | "back"
  | "left"
  | "right"
  | "top"
  | "bottom";

export const VIEW_DIRECTIONS: Record<ViewName, THREE.Vector3> = {
  iso: new THREE.Vector3(1, 0.85, 1),
  front: new THREE.Vector3(0, 0, 1),
  back: new THREE.Vector3(0, 0, -1),
  left: new THREE.Vector3(-1, 0, 0),
  right: new THREE.Vector3(1, 0, 0),
  top: new THREE.Vector3(0, 1, 0),
  bottom: new THREE.Vector3(0, -1, 0),
};

const VIEW_LABELS: Record<ViewName, string> = {
  iso: "等轴测",
  front: "前视",
  back: "后视",
  left: "左视",
  right: "右视",
  top: "顶视",
  bottom: "底视",
};

export function viewLabel(v: ViewName): string {
  return VIEW_LABELS[v];
}

/**
 * 3D 视口：相机、网格、坐标轴、视角立方、拾取、标准视图。
 * 只负责渲染与交互，模型由调用方通过 setModel 注入。
 */
export class Viewport3D {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  private viewHelper: ViewHelper;
  private grid: THREE.GridHelper;
  private axes: THREE.AxesHelper;
  private model: THREE.Group | null = null;
  private solidMaterials: THREE.MeshStandardMaterial[] = [];
  private edges: THREE.LineSegments[] = [];
  private raf = 0;
  private lastTime = 0;
  private raycaster = new THREE.Raycaster();
  private groundPlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  private hitPoint = new THREE.Vector3();
  /** 上次上报给 UI 的光标点；去抖避免每次 pointermove 都触发 Vue 重渲染。 */
  private cursorPoint = new THREE.Vector3();
  private cursorValid = false;
  /** 左键按下时的屏幕坐标，用于区分「点击选中」与「拖动旋转」。 */
  private pickStart: { x: number; y: number } | null = null;
  private resizeObserver: ResizeObserver;

  onCursor: ((point: THREE.Vector3 | null) => void) | null = null;
  onPick: ((featureId: string | null) => void) | null = null;
  /** 选中特征的屏幕锚点（画布像素坐标），供悬浮工具条跟随；null 表示隐藏。 */
  onAnchor: ((point: { x: number; y: number } | null) => void) | null = null;
  private anchor: THREE.Vector3 | null = null;
  private anchorProjected = new THREE.Vector3();
  private anchorScreen = new THREE.Vector2(Number.NaN, Number.NaN);
  /** 渲染致命错误（上下文创建失败/丢失），由 UI 显示常驻浮层。 */
  onError: ((message: string) => void) | null = null;
  /** 仅视角立方点击动画期间驱动相机（ViewHelper.update 无守卫，每帧无条件重置相机位置）。 */
  private helperAnimating = false;

  constructor(
    private canvas: HTMLCanvasElement,
    private container: HTMLElement
  ) {
    // three r187+ 仅支持 WebGL2；先显式探测，失败时给出可读原因而非黑屏。
    if (!canvas.getContext("webgl2")) {
      throw new Error(
        "WebGL2 不可用：WebView2 可能运行在禁用 GPU 加速的环境（远程桌面/虚拟机常见）。" +
          "请尝试在 WebView2 命令行附加 --use-angle=swiftshader 或启用硬件加速。"
      );
    }
    // alpha:false 保证不透明画布；preserveDrawingBuffer 规避 WebView2 偶发的画布呈现（合成）失败导致整块空白。
    try {
      this.renderer = new THREE.WebGLRenderer({
        canvas,
        antialias: true,
        alpha: false,
        preserveDrawingBuffer: true,
      });
    } catch {
      this.renderer = new THREE.WebGLRenderer({
        canvas,
        antialias: false,
        alpha: false,
        preserveDrawingBuffer: true,
      });
    }
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    // ViewHelper 标准用法（同 three.js Editor）：关闭自动清屏，由主循环手动 clear。
    // 否则 viewHelper.render 内部的 renderer.render 会 gl.clear 整个颜色缓冲（clear 不受视口限制），
    // 把主渲染画面整块擦掉，画布只剩右上角 Gizmo。
    this.renderer.autoClear = false;
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    this.renderer.setClearColor(0xccd5e1, 1);

    // 环境贴图（IBL）为锦上添花，失败时回退到直接光照，绝不应让画布黑屏。
    try {
      const pmrem = new THREE.PMREMGenerator(this.renderer);
      this.scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
      pmrem.dispose();
    } catch (e) {
      console.warn("[viewport] 环境贴图生成失败，回退到直接光照：", e);
    }

    this.scene.background = new THREE.Color(0xccd5e1);

    this.camera = new THREE.PerspectiveCamera(45, 1, 0.5, 8000);
    this.camera.up.set(0, 1, 0);
    this.camera.position.set(180, 150, 190);

    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.1;
    this.controls.screenSpacePanning = false;
    this.controls.minDistance = 15;
    this.controls.maxDistance = 4000;
    // CAD 习惯：左键旋转、中键平移、右键平移、滚轮缩放
    this.controls.mouseButtons = {
      LEFT: THREE.MOUSE.ROTATE,
      MIDDLE: THREE.MOUSE.PAN,
      RIGHT: THREE.MOUSE.PAN,
    };
    this.controls.target.set(0, 8, 0);

    const key = new THREE.DirectionalLight(0xffffff, 1.9);
    key.position.set(160, 260, 140);
    key.castShadow = true;
    key.shadow.mapSize.set(2048, 2048);
    key.shadow.camera.left = -220;
    key.shadow.camera.right = 220;
    key.shadow.camera.top = 220;
    key.shadow.camera.bottom = -220;
    key.shadow.bias = -0.0008;
    this.scene.add(key);

    const fill = new THREE.HemisphereLight(0xdfe8ff, 0x8a94a6, 0.65);
    this.scene.add(fill);
    // 环境贴图（IBL）本身很亮，单独调低其贡献，避免整体过曝
    this.scene.environmentIntensity = 0.6;

    const catcher = new THREE.Mesh(
      new THREE.PlaneGeometry(1200, 1200),
      new THREE.ShadowMaterial({ opacity: 0.28 })
    );
    catcher.rotation.x = -Math.PI / 2;
    catcher.receiveShadow = true;
    this.scene.add(catcher);

    this.grid = new THREE.GridHelper(600, 60, 0x6f8299, 0x9dabc0);
    const gm = this.grid.material as THREE.Material;
    gm.transparent = true;
    gm.opacity = 0.9;
    gm.depthWrite = false;
    this.scene.add(this.grid);

    this.axes = new THREE.AxesHelper(90);
    this.scene.add(this.axes);

    this.viewHelper = new ViewHelper(this.camera, container);
    this.viewHelper.location = { top: 12, right: 12, bottom: 12, left: null };
    this.viewHelper.setLabels("X", "Y", "Z");
    this.scene.add(this.viewHelper);

    canvas.addEventListener("pointerdown", this.handleViewHelperClick);
    canvas.addEventListener("pointermove", this.handlePointerMove);
    canvas.addEventListener("pointerleave", this.handlePointerLeave);
    // 拾取用「按下 → 抬起」判定点击，而非按下即选中：否则按住旋转也会弹出悬浮工具条
    canvas.addEventListener("pointerdown", this.handlePickStart);
    canvas.addEventListener("pointerup", this.handlePick);
    canvas.addEventListener("webglcontextlost", (e) => {
      e.preventDefault();
      console.error("[viewport] WebGL 上下文丢失，渲染已暂停");
      this.onError?.("WebGL 上下文丢失，渲染已暂停。请重启窗口；若反复出现，请检查 GPU 加速设置。");
    });

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.resize();
    this.animate();
  }

  private handleViewHelperClick = (event: PointerEvent): void => {
    if (this.viewHelper.handleClick(event)) {
      this.helperAnimating = true;
      event.stopImmediatePropagation();
      event.preventDefault();
    }
  };

  private handlePointerLeave = (): void => {
    this.cursorValid = false;
    this.onCursor?.(null);
  };

  private handlePointerMove = (event: PointerEvent): void => {
    const rect = this.canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    );
    this.raycaster.setFromCamera(ndc, this.camera);
    const hit = this.raycaster.ray.intersectPlane(this.groundPlane, this.hitPoint);
    if (!hit) {
      this.cursorValid = false;
      this.onCursor?.(null);
      return;
    }
    // 位移不足 0.5 世界单位不通知 UI：光标读数无需逐像素刷新，省去每帧 Vue 更新
    if (this.cursorValid && this.cursorPoint.distanceToSquared(this.hitPoint) < 0.25) return;
    this.cursorPoint.copy(this.hitPoint);
    this.cursorValid = true;
    this.onCursor?.(this.hitPoint.clone());
  };

  /** 左键按下时记录起点；视角立方命中会 stopImmediatePropagation，故本处理器不会执行。 */
  private handlePickStart = (event: PointerEvent): void => {
    this.pickStart = event.button === 0 ? { x: event.clientX, y: event.clientY } : null;
  };

  private handlePick = (event: PointerEvent): void => {
    const start = this.pickStart;
    this.pickStart = null;
    if (!start || event.button !== 0) return;
    // 位移超过 4px 视为拖动（旋转/平移），不选中——避免旋转时冒出悬浮工具条
    if (Math.hypot(event.clientX - start.x, event.clientY - start.y) > 4) return;
    const rect = this.canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    );
    this.raycaster.setFromCamera(ndc, this.camera);
    const meshes = this.model ? this.model.children.filter(isMesh) : [];
    const hits = this.raycaster.intersectObjects(meshes, true);
    const first = hits[0]?.object.userData.featureId as string | undefined;
    // 锚点取真实命中点（而非孔代理中心），悬浮工具条才会贴在被点的面上。
    this.setAnchor(first && hits[0] ? hits[0].point.clone() : null);
    this.onPick?.(first ?? null);
  };

  private setAnchor(point: THREE.Vector3 | null): void {
    this.anchor = point;
    this.anchorScreen.set(Number.NaN, Number.NaN);
    if (!point) this.onAnchor?.(null);
  }

  /** 世界坐标锚点 → 画布像素坐标；只在位移超过半像素时通知 UI，避免每帧触发 Vue 更新。 */
  private syncAnchor(): void {
    if (!this.anchor || !this.onAnchor) return;
    const v = this.anchorProjected.copy(this.anchor).project(this.camera);
    const w = this.container.clientWidth || 1;
    const h = this.container.clientHeight || 1;
    const x = (v.x * 0.5 + 0.5) * w;
    const y = (-v.y * 0.5 + 0.5) * h;
    if (Math.abs(x - this.anchorScreen.x) < 0.5 && Math.abs(y - this.anchorScreen.y) < 0.5) return;
    this.anchorScreen.set(x, y);
    this.onAnchor({ x, y });
  }

  /** 清除锚点（从模型树选特征时用，避免工具条停在旧位置）。 */
  clearAnchor(): void {
    this.setAnchor(null);
  }

  private animate = (): void => {
    this.raf = requestAnimationFrame(this.animate);
    const now = performance.now();
    const delta = this.lastTime ? (now - this.lastTime) / 1000 : 0.016;
    this.lastTime = now;
    // 任何一帧异常都不允许静默吞掉——主渲染在 try 内，抛错意味着画布空白，必须上浮到 UI。
    try {
      this.controls.update();
      if (this.helperAnimating) {
        this.viewHelper.update(delta);
        if (!this.viewHelper.animating) this.helperAnimating = false;
      }
      this.renderer.clear();
      this.renderer.render(this.scene, this.camera);
      this.viewHelper.render(this.renderer);
      this.syncAnchor();
    } catch (e) {
      console.error("[viewport] 渲染帧失败：", e);
      this.onError?.(
        `渲染帧失败：${e instanceof Error ? e.message : String(e)}`
      );
    }
  };

  resize(): void {
    const w = this.container.clientWidth || 1;
    const h = this.container.clientHeight || 1;
    this.renderer.setSize(w, h, false);
    this.canvas.style.width = `${w}px`;
    this.canvas.style.height = `${h}px`;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  /** 注入模型并整体适应视图。 */
  setModel(group: THREE.Group): void {
    if (this.model) this.scene.remove(this.model);
    this.solidMaterials = [];
    this.edges = [];
    this.model = group;
    group.traverse((obj) => {
      if (isMesh(obj)) {
        if (obj.userData.pickProxy) return; // 孔拾取代理：不投影、不参与线框切换
        obj.castShadow = true;
        obj.receiveShadow = true;
        this.solidMaterials.push(obj.material as THREE.MeshStandardMaterial);
      }
      if (obj instanceof THREE.LineSegments) this.edges.push(obj);
    });
    this.scene.add(group);
    this.fit();
  }

  setView(view: ViewName): void {
    const dir = VIEW_DIRECTIONS[view].clone().normalize();
    const radius = this.modelRadius();
    const distance = Math.max(radius / Math.sin(THREE.MathUtils.degToRad(22)), 60);
    this.camera.position.copy(this.controls.target.clone().add(dir.multiplyScalar(distance)));
    this.camera.up.set(0, 1, 0);
    if (view === "top" || view === "bottom") this.camera.up.set(0, 0, view === "top" ? 1 : -1);
    this.controls.update();
  }

  fit(): void {
    const box = this.modelBox();
    if (box.isEmpty()) return;
    const center = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3());
    const radius = Math.max(size.x, size.y, size.z) * 0.5 || 50;
    this.controls.target.copy(center);
    const distance = radius / Math.sin(THREE.MathUtils.degToRad(22)) * 1.25;
    const dir = this.camera.position.clone().sub(center);
    if (dir.lengthSq() < 1e-6) dir.set(1, 0.85, 1);
    dir.normalize().multiplyScalar(distance);
    this.camera.position.copy(center.clone().add(dir));
    this.camera.near = Math.max(distance / 1000, 0.5);
    this.camera.far = distance * 100;
    this.camera.updateProjectionMatrix();
    this.controls.update();
  }

  setWireframe(on: boolean): void {
    for (const m of this.solidMaterials) m.wireframe = on;
    for (const e of this.edges) e.visible = !on;
  }

  /** 按特征 id 高亮实体（配合右侧模型树的点选）。 */
  setHighlight(featureId: string | null): void {
    if (!featureId) this.setAnchor(null);
    if (!this.model) return;
    this.model.traverse((obj) => {
      if (!isMesh(obj)) return;
      const on = obj.userData.featureId === featureId;
      // 孔没有独立实体，选中时把半透明拾取代理显出来作为画布反馈。
      if (obj.userData.pickProxy) {
        obj.visible = on;
        return;
      }
      const mat = obj.material as THREE.MeshStandardMaterial;
      mat.emissive.setHex(on ? 0x2b6cb0 : 0x000000);
      mat.emissiveIntensity = on ? 0.35 : 0;
    });
  }

  setGrid(on: boolean): void {
    this.grid.visible = on;
  }

  setAxes(on: boolean): void {
    this.axes.visible = on;
  }

  private modelRadius(): number {
    const box = this.modelBox();
    if (box.isEmpty()) return 50;
    const size = box.getSize(new THREE.Vector3());
    return Math.max(size.x, size.y, size.z) * 0.5;
  }

  private modelBox(): THREE.Box3 {
    const box = new THREE.Box3();
    if (this.model) box.setFromObject(this.model);
    return box;
  }

  dispose(): void {
    cancelAnimationFrame(this.raf);
    this.resizeObserver.disconnect();
    this.canvas.removeEventListener("pointerdown", this.handleViewHelperClick);
    this.canvas.removeEventListener("pointermove", this.handlePointerMove);
    this.canvas.removeEventListener("pointerleave", this.handlePointerLeave);
    this.canvas.removeEventListener("pointerdown", this.handlePickStart);
    this.canvas.removeEventListener("pointerup", this.handlePick);
    this.controls.dispose();
    this.viewHelper.dispose();
    this.scene.traverse((obj) => {
      if (isMesh(obj)) {
        obj.geometry.dispose();
        (obj.material as THREE.Material).dispose();
      }
    });
    this.renderer.dispose();
  }
}

function isMesh(obj: THREE.Object3D): obj is THREE.Mesh {
  return obj instanceof THREE.Mesh;
}
