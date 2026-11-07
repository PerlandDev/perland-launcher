// The 3D figure on the Skins stage, drawn with three.js. Faces are lit the way the game lights blocks (top brightest,
// sides darker), with no lights to ease between; the texture is nearest-filtered so every texel stays a crisp square.
// Kept behind this small adapter so the screen can be tested without WebGL.
import {
  BufferAttribute,
  BufferGeometry,
  DataTexture,
  DoubleSide,
  FrontSide,
  Group,
  Mesh,
  MeshBasicMaterial,
  NearestFilter,
  PerspectiveCamera,
  Raycaster,
  RGBAFormat,
  Scene,
  SRGBColorSpace,
  UnsignedByteType,
  Vector2,
  WebGLRenderer,
} from 'three';
import type { SkinModel } from '../../ipc/schema';
import { boxes, SKIN_SIZE, type Box, type Layer, type Part, type Vec3 } from './layout';

/** How bright each face direction is, like the game's block shading. */
function brightness([x, y, z]: Vec3): number {
  if (y > 0) return 1;
  if (y < 0) return 0.55;
  if (z !== 0) return 0.86;
  return x !== 0 ? 0.7 : 1;
}

function geometryFor(box: Box): BufferGeometry {
  const positions: number[] = [];
  const uvs: number[] = [];
  const colors: number[] = [];
  const index: number[] = [];
  box.faces.forEach((face, i) => {
    const [lt, rt, lb, rb] = face.corners;
    const u0 = face.x / SKIN_SIZE;
    const u1 = (face.x + face.w) / SKIN_SIZE;
    const v0 = face.y / SKIN_SIZE;
    const v1 = (face.y + face.h) / SKIN_SIZE;
    for (const p of [lt, rt, lb, rb]) positions.push(...p);
    uvs.push(u0, v0, u1, v0, u0, v1, u1, v1);
    const b = brightness(face.normal);
    for (let k = 0; k < 4; k++) colors.push(b, b, b);
    const base = i * 4;
    // Wind each quad so it faces outward: check the triangle's normal against the face's.
    const e1: Vec3 = [lb[0] - lt[0], lb[1] - lt[1], lb[2] - lt[2]];
    const e2: Vec3 = [rt[0] - lt[0], rt[1] - lt[1], rt[2] - lt[2]];
    const n: Vec3 = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
    const outward = n[0] * face.normal[0] + n[1] * face.normal[1] + n[2] * face.normal[2] > 0;
    if (outward) index.push(base, base + 2, base + 1, base + 1, base + 2, base + 3);
    else index.push(base, base + 1, base + 2, base + 1, base + 3, base + 2);
  });
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(new Float32Array(positions), 3));
  geometry.setAttribute('uv', new BufferAttribute(new Float32Array(uvs), 2));
  geometry.setAttribute('color', new BufferAttribute(new Float32Array(colors), 3));
  geometry.setIndex(index);
  return geometry;
}

/** The dirt plinth the figure stands on: a pixel block in the launcher's own art palette. */
function plinth(): Mesh {
  const size = 8;
  const data = new Uint8Array(size * size * 4);
  for (let i = 0; i < size * size; i++) {
    const deep = (i * 7 + (i >> 3) * 3) % 5 < 2;
    const [r, g, b] = deep ? [0x4e, 0x2e, 0x19] : [0x5e, 0x38, 0x20];
    data.set([r, g, b, 255], i * 4);
  }
  const texture = new DataTexture(data, size, size, RGBAFormat, UnsignedByteType);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.colorSpace = SRGBColorSpace;
  texture.needsUpdate = true;
  const h = 1.5;
  const w = 10;
  const corners = (y: number): [Vec3, Vec3, Vec3, Vec3] => [
    [-w, y, -w],
    [w, y, -w],
    [-w, y, w],
    [w, y, w],
  ];
  const [a, b, c, d] = corners(0);
  const [e, f, g, k] = corners(-h);
  const quads: [Vec3, Vec3, Vec3, Vec3, number][] = [
    [a, b, c, d, 1],
    [c, d, g, k, 0.62],
    [d, b, k, f, 0.5],
    [a, c, e, g, 0.5],
    [b, a, f, e, 0.45],
  ];
  const positions: number[] = [];
  const uvs: number[] = [];
  const colors: number[] = [];
  const index: number[] = [];
  quads.forEach(([p0, p1, p2, p3, light], i) => {
    for (const p of [p0, p1, p2, p3]) positions.push(...p);
    uvs.push(0, 0, 1, 0, 0, i === 0 ? 1 : 0.2, 1, i === 0 ? 1 : 0.2);
    for (let j = 0; j < 4; j++) colors.push(light, light, light);
    const base = i * 4;
    index.push(base, base + 2, base + 1, base + 1, base + 2, base + 3);
  });
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(new Float32Array(positions), 3));
  geometry.setAttribute('uv', new BufferAttribute(new Float32Array(uvs), 2));
  geometry.setAttribute('color', new BufferAttribute(new Float32Array(colors), 3));
  geometry.setIndex(index);
  return new Mesh(geometry, new MeshBasicMaterial({ map: texture, vertexColors: true, side: DoubleSide }));
}

export interface Hit {
  x: number;
  y: number;
  part: Part;
  layer: Layer;
}

export interface Stage {
  /** Shows `data` (64 × 64 RGBA, rows top first). */
  setTexture(data: Uint8ClampedArray): void;
  setModel(model: SkinModel): void;
  setVisible(part: Part, layer: Layer, visible: boolean): void;
  /** The layer `pick` paints on. */
  setPaintLayer(layer: Layer): void;
  /** The texel under a point of the canvas (client coordinates), on the paint layer; null over empty stage. */
  pick(clientX: number, clientY: number): Hit | null;
  orbit(dx: number, dy: number): void;
  zoom(factor: number): void;
  resetView(): void;
  resize(width: number, height: number): void;
  dispose(): void;
}

const TARGET: Vec3 = [0, 13, 0];
/** `zoom` scales the distance that fits the whole figure in the stage. */
const START = { yaw: -0.5, pitch: 0.14, zoom: 1 };
/** What must fit: the figure with its plinth, in texels. */
const FIT_HEIGHT = 50;
const FIT_WIDTH = 30;

export function createStage(canvas: HTMLCanvasElement): Stage {
  const renderer = new WebGLRenderer({ canvas, antialias: false, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.setClearColor(0x000000, 0);
  const scene = new Scene();
  const camera = new PerspectiveCamera(32, 1, 1, 400);
  const figure = new Group();
  scene.add(figure);
  scene.add(plinth());

  const data = new Uint8Array(SKIN_SIZE * SKIN_SIZE * 4);
  const texture = new DataTexture(data, SKIN_SIZE, SKIN_SIZE, RGBAFormat, UnsignedByteType);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.colorSpace = SRGBColorSpace;
  texture.needsUpdate = true;
  const baseMaterial = new MeshBasicMaterial({ map: texture, vertexColors: true, alphaTest: 0.5, side: FrontSide });
  const outerMaterial = new MeshBasicMaterial({ map: texture, vertexColors: true, alphaTest: 0.5, side: DoubleSide });

  let meshes: { mesh: Mesh; box: Box }[] = [];
  const hidden = new Set<string>();
  let paintLayer: Layer = 'base';
  const view = { ...START };
  let fitDistance = 80;
  let frame = 0;

  const draw = () => {
    frame = 0;
    const { yaw, pitch } = view;
    const distance = fitDistance * view.zoom;
    camera.position.set(
      TARGET[0] + distance * Math.cos(pitch) * Math.sin(yaw),
      TARGET[1] + distance * Math.sin(pitch),
      TARGET[2] + distance * Math.cos(pitch) * Math.cos(yaw),
    );
    camera.lookAt(TARGET[0], TARGET[1], TARGET[2]);
    renderer.render(scene, camera);
  };
  const invalidate = () => {
    if (!frame) frame = requestAnimationFrame(draw);
  };

  const build = (model: SkinModel) => {
    for (const { mesh } of meshes) {
      figure.remove(mesh);
      mesh.geometry.dispose();
    }
    meshes = boxes(model).map((box) => {
      const mesh = new Mesh(geometryFor(box), box.layer === 'base' ? baseMaterial : outerMaterial);
      mesh.visible = !hidden.has(`${box.part}:${box.layer}`);
      figure.add(mesh);
      return { mesh, box };
    });
    invalidate();
  };
  build('classic');

  const raycaster = new Raycaster();
  const pointer = new Vector2();

  return {
    setTexture(next) {
      data.set(next);
      texture.needsUpdate = true;
      invalidate();
    },
    setModel: build,
    setVisible(part, layer, visible) {
      const key = `${part}:${layer}`;
      if (visible) hidden.delete(key);
      else hidden.add(key);
      for (const { mesh, box } of meshes) if (box.part === part && box.layer === layer) mesh.visible = visible;
      invalidate();
    },
    setPaintLayer(layer) {
      paintLayer = layer;
    },
    pick(clientX, clientY) {
      const rect = canvas.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return null;
      pointer.set(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
      raycaster.setFromCamera(pointer, camera);
      const targets = meshes.filter(({ mesh, box }) => mesh.visible && box.layer === paintLayer);
      const hit = raycaster.intersectObjects(targets.map((t) => t.mesh), false)[0];
      if (!hit || !hit.uv || hit.faceIndex == null) return null;
      const owner = targets.find((t) => t.mesh === hit.object)!;
      const face = owner.box.faces[Math.floor(hit.faceIndex / 2)]!;
      const clamp = (v: number, lo: number, n: number) => Math.min(lo + n - 1, Math.max(lo, v));
      return {
        x: clamp(Math.floor(hit.uv.x * SKIN_SIZE), face.x, face.w),
        y: clamp(Math.floor(hit.uv.y * SKIN_SIZE), face.y, face.h),
        part: owner.box.part,
        layer: owner.box.layer,
      };
    },
    orbit(dx, dy) {
      view.yaw -= dx * 0.01;
      view.pitch = Math.max(-1.2, Math.min(1.2, view.pitch + dy * 0.01));
      invalidate();
    },
    zoom(factor) {
      view.zoom = Math.max(0.35, Math.min(1.8, view.zoom * factor));
      invalidate();
    },
    resetView() {
      Object.assign(view, START);
      invalidate();
    },
    resize(width, height) {
      if (width <= 0 || height <= 0) return;
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      const half = Math.tan((camera.fov * Math.PI) / 360);
      fitDistance = Math.max(FIT_HEIGHT / 2 / half, FIT_WIDTH / 2 / (half * camera.aspect));
      invalidate();
    },
    dispose() {
      if (frame) cancelAnimationFrame(frame);
      for (const { mesh } of meshes) mesh.geometry.dispose();
      texture.dispose();
      baseMaterial.dispose();
      outerMaterial.dispose();
      renderer.dispose();
    },
  };
}
