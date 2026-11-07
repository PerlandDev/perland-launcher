// The 3D figure on the Skins stage, drawn with three.js. Faces are lit the way the game lights blocks (top brightest,
// sides darker), with no lights to ease between; the texture is nearest-filtered so every texel stays a crisp square.
// The figure is a jointed rig that poses (walk, run, crouch, sit, wave, row a boat, ride a horse); the boat and the
// horse are block props shaped like the game's, drawn here in the launcher's own colours, not Mojang's textures. The
// outer layer is drawn in 3D: each painted texel a small block standing on the body (see outerWalls).
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
  RepeatWrapping,
  RGBAFormat,
  Scene,
  SRGBColorSpace,
  UnsignedByteType,
  Vector2,
  WebGLRenderer,
} from 'three';
import type { SkinModel } from '../../ipc/schema';
import { boxes, SKIN_SIZE, type Box, type Layer, type Part, type Vec3 } from './layout';
import { outerWalls, type Wall } from './outerWalls';
import { AnimationClock, celebration, CELEBRATE_S, POSE_STILL_T, rigAt, type Joint, type Pose } from './poses';

/** How bright each face direction is, like the game's block shading. */
function brightness([x, y, z]: Vec3): number {
  if (y > 0) return 1;
  if (y < 0) return 0.55;
  if (z !== 0) return 0.86;
  return x !== 0 ? 0.7 : 1;
}

function makeGeometry(positions: number[], uvs: number[], colors: number[], index: number[]) {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(new Float32Array(positions), 3));
  geometry.setAttribute('uv', new BufferAttribute(new Float32Array(uvs), 2));
  geometry.setAttribute('color', new BufferAttribute(new Float32Array(colors), 3));
  geometry.setIndex(index);
  return geometry;
}

/** A skin box's faces, placed relative to the joint it hangs from. */
function geometryFor(box: Box, pivot: Vec3): BufferGeometry {
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
    for (const p of [lt, rt, lb, rb]) positions.push(p[0] - pivot[0], p[1] - pivot[1], p[2] - pivot[2]);
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
  return makeGeometry(positions, uvs, colors, index);
}

/** The sides of the outer layer's texel blocks, each wall one flat colour: its texel's. */
function wallGeometry(walls: Wall[], pivot: Vec3): BufferGeometry {
  const positions: number[] = [];
  const uvs: number[] = [];
  const colors: number[] = [];
  const index: number[] = [];
  walls.forEach((wall, i) => {
    for (const p of wall.corners) positions.push(p[0] - pivot[0], p[1] - pivot[1], p[2] - pivot[2]);
    const u = (wall.texel[0] + 0.5) / SKIN_SIZE;
    const v = (wall.texel[1] + 0.5) / SKIN_SIZE;
    for (let k = 0; k < 4; k++) uvs.push(u, v);
    const b = brightness(wall.normal.map(Math.round) as Vec3);
    for (let k = 0; k < 4; k++) colors.push(b, b, b);
    const base = i * 4;
    index.push(base, base + 1, base + 2, base + 1, base + 3, base + 2);
  });
  return makeGeometry(positions, uvs, colors, index);
}

/** The pixel surfaces the props are made of, 16 × 16 texels of light and shade multiplied over each block's colour,
 *  one texel a model unit, so a prop reads at the same pixel scale as the skin. */
type Surface = 'grain' | 'planks' | 'hair' | 'flat';

/** A small deterministic hash, so every prop looks the same each time. */
function hash(x: number, y: number, seed: number): number {
  let h = (x * 374761393 + y * 668265263 + seed * 2147483647) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967295;
}

function surfaceTexture(kind: Surface): DataTexture {
  const n = 16;
  const data = new Uint8Array(n * n * 4);
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      const r = hash(x, y, 7);
      let v: number;
      switch (kind) {
        case 'grain':
          // Speckled: mostly mid, some darker pits and lighter flecks.
          v = r < 0.15 ? 0.74 : r > 0.88 ? 1 : 0.84 + hash(x >> 1, y >> 1, 3) * 0.1;
          break;
        case 'planks': {
          // Four planks a tile, a dark seam under each, staggered end joints, grain streaks along the boards.
          const row = y % 4;
          const board = y >> 2;
          const joint = (x + board * 5) % 16 === 0;
          v = row === 3 || joint ? 0.62 : 0.86 + hash(board, 0, 11) * 0.1 + (hash(x >> 2, y, 5) - 0.5) * 0.08;
          break;
        }
        case 'hair':
          // Strands: each column its own shade, a little variation down it.
          v = 0.72 + hash(x, 0, 17) * 0.24 + (r - 0.5) * 0.08;
          break;
        case 'flat':
          v = 1;
          break;
      }
      const c = Math.round(Math.max(0, Math.min(1, v)) * 255);
      data.set([c, c, c, 255], (y * n + x) * 4);
    }
  }
  const texture = new DataTexture(data, n, n, RGBAFormat, UnsignedByteType);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.wrapS = RepeatWrapping;
  texture.wrapT = RepeatWrapping;
  texture.needsUpdate = true;
  return texture;
}

const surfaces = new Map<Surface, MeshBasicMaterial>();
function surface(kind: Surface): MeshBasicMaterial {
  let material = surfaces.get(kind);
  if (!material) {
    material = new MeshBasicMaterial({ map: surfaceTexture(kind), vertexColors: true, side: FrontSide });
    surfaces.set(kind, material);
  }
  return material;
}
let blockSeed = 0;

/** A block prop: its six faces shaded like the figure's, in one colour (0xRRGGBB) over a pixel surface. */
function block(size: Vec3, center: Vec3, color: number, top = color, kind: Surface = 'grain'): Mesh {
  const [hx, hy, hz] = size.map((s) => s / 2) as Vec3;
  const [cx, cy, cz] = center;
  const p = (x: number, y: number, z: number): Vec3 => [cx + x * hx, cy + y * hy, cz + z * hz];
  const faces: [Vec3, Vec3, Vec3, Vec3, Vec3][] = [
    [p(-1, 1, -1), p(1, 1, -1), p(-1, 1, 1), p(1, 1, 1), [0, 1, 0]],
    [p(-1, -1, 1), p(1, -1, 1), p(-1, -1, -1), p(1, -1, -1), [0, -1, 0]],
    [p(-1, 1, 1), p(1, 1, 1), p(-1, -1, 1), p(1, -1, 1), [0, 0, 1]],
    [p(1, 1, -1), p(-1, 1, -1), p(1, -1, -1), p(-1, -1, -1), [0, 0, -1]],
    [p(-1, 1, -1), p(-1, 1, 1), p(-1, -1, -1), p(-1, -1, 1), [-1, 0, 0]],
    [p(1, 1, 1), p(1, 1, -1), p(1, -1, 1), p(1, -1, -1), [1, 0, 0]],
  ];
  const positions: number[] = [];
  const uvs: number[] = [];
  const colors: number[] = [];
  const index: number[] = [];
  blockSeed += 1;
  faces.forEach(([a, b, c, d, normal], i) => {
    for (const q of [a, b, c, d]) positions.push(...q);
    // One texel a unit, from a different spot of the tile on every face, so repeats never line up.
    const w = Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]) / 16;
    const h = Math.hypot(c[0] - a[0], c[1] - a[1], c[2] - a[2]) / 16;
    const u = Math.floor(hash(blockSeed, i, 1) * 16) / 16;
    const v = Math.floor(hash(i, blockSeed, 2) * 16) / 16;
    uvs.push(u, v, u + w, v, u, v + h, u + w, v + h);
    const hex = normal[1] > 0 ? top : color;
    const light = brightness(normal);
    const rgb = [(hex >> 16) & 255, (hex >> 8) & 255, hex & 255].map((v) => Math.pow(v / 255, 2.2) * light);
    for (let k = 0; k < 4; k++) colors.push(rgb[0]!, rgb[1]!, rgb[2]!);
    const base = i * 4;
    index.push(base, base + 2, base + 1, base + 1, base + 2, base + 3);
  });
  return new Mesh(makeGeometry(positions, uvs, colors, index), surface(kind));
}

/** A rowing boat shaped like the game's: a flat bottom, low sides and ends, and a paddle on each side. Oak-toned
 *  planks drawn here, not Mojang's texture. It floats on the empty stage, no water under it. */
function boat(): { group: Group; oars: Group[] } {
  const group = new Group();
  const plank = 0x9c7446;
  const plankTop = 0xb08550;
  const plankDark = 0x6e5030;
  group.add(block([16, 3, 28], [0, 1.5, 0], plankDark, plank, 'planks'));
  for (const side of [-1, 1]) group.add(block([2, 9, 28], [side * 9, 4.5, 0], plank, plankTop, 'planks'));
  group.add(block([16, 6, 2], [0, 6, 13], plank, plankTop, 'planks'));
  group.add(block([16, 6, 2], [0, 6, -13], plank, plankTop, 'planks'));
  const oars: Group[] = [];
  for (const side of [-1, 1]) {
    const oar = new Group();
    // Pinned on the rail, the shaft runs back and out, the blade at its end.
    oar.position.set(side * 10, 8, 3);
    oar.add(block([2, 2, 18], [0, 0, -8], plankTop, plankTop, 'planks'));
    oar.add(block([1, 6, 7], [0, 0, -18], plank, plankTop, 'planks'));
    oar.rotation.set(0, side * 0.35, side * 0.75);
    group.add(oar);
    oars.push(oar);
  }
  return { group, oars };
}

/** A horse shaped like the game's: legs, a long body, a neck raised forward with the head and muzzle at its top, a
 *  mane and a tail, and a saddle. A chestnut coat drawn here, not Mojang's texture. */
function horse(): { group: Group; legs: Group[]; head: Group; tail: Group } {
  const group = new Group();
  const coat = 0x8a5a34;
  const coatTop = 0x9c6a3e;
  const mane = 0x3a2516;
  const hoof = 0x2b1d14;
  const leather = 0x4a2c1c;
  group.add(block([10, 10, 22], [0, 16, 0], coat, coatTop, 'hair'));
  // The saddle on its back, with stirrups hanging either side.
  group.add(block([10.4, 1, 9], [0, 21.5, -1], leather, 0x5a3624, 'flat'));
  group.add(block([6, 1.5, 1.5], [0, 22.5, 2.6], leather, 0x5a3624, 'flat'));
  for (const side of [-1, 1]) {
    group.add(block([0.4, 6, 0.4], [side * 5.3, 18, -1], hoof, hoof, 'flat'));
    group.add(block([1, 0.8, 2], [side * 5.4, 14.8, -1], 0x9aa0a6, 0xb8bec4, 'flat'));
  }
  const legs: Group[] = [];
  for (const [x, z] of [[-3, 8.5], [3, 8.5], [-3, -8.5], [3, -8.5]] as const) {
    const leg = new Group();
    leg.position.set(x, 11, z);
    leg.add(block([4, 11, 4], [0, -5.5, 0], coat, coatTop, 'hair'));
    leg.add(block([4.2, 2, 4.2], [0, -10, 0], hoof, hoof));
    group.add(leg);
    legs.push(leg);
  }
  const head = new Group();
  head.position.set(0, 20, 10);
  head.add(block([4, 12, 7], [0, 5, -1.5], coat, coatTop, 'hair'));
  head.add(block([6, 5, 7], [0, 8.5, 3.5], coat, coatTop, 'hair'));
  head.add(block([4, 5, 5], [0, 8.5, 9.5], 0x7a4b2c, 0x8a5a34, 'hair'));
  head.add(block([2, 16, 2], [0, 3, -6], mane, mane, 'hair'));
  for (const side of [-1, 1]) {
    head.add(block([2, 3, 1], [side * 2, 12.5, 0.5], coat, coatTop));
    head.add(block([0.3, 1, 1], [side * 3.05, 9.5, 4.5], 0x14100c, 0x14100c, 'flat'));
    head.add(block([0.3, 0.3, 7], [side * 2.2, 7, 6], hoof, hoof, 'flat'));
  }
  group.add(head);
  const tail = new Group();
  tail.position.set(0, 20, -11);
  tail.add(block([3, 14, 4], [0, -7, -2], mane, mane, 'hair'));
  tail.rotation.x = 0.3;
  group.add(tail);
  return { group, legs, head, tail };
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
  setPose(pose: Pose): void;
  /** Holds the figure (and its boat or horse) on the current moment, or lets it move on from there. */
  setPaused(paused: boolean): void;
  /** A hop and a turn, when a skin goes on. */
  celebrate(): void;
  /** The texel under a point of the canvas (client coordinates), on the paint layer; null over empty stage. */
  pick(clientX: number, clientY: number): Hit | null;
  orbit(dx: number, dy: number): void;
  zoom(factor: number): void;
  resetView(): void;
  resize(width: number, height: number): void;
  dispose(): void;
}

/** Where each joint turns, in model space. */
function pivotOf(joint: Joint, model: SkinModel): Vec3 {
  const arm = model === 'slim' ? 5.5 : 6;
  switch (joint) {
    case 'torso':
      return [0, 12, 0];
    case 'head':
      return [0, 24, 0];
    case 'rightArm':
      return [-arm, 22, 0];
    case 'leftArm':
      return [arm, 22, 0];
    case 'rightLeg':
      return [-2, 12, 0];
    case 'leftLeg':
      return [2, 12, 0];
  }
}

const JOINT_OF: Record<Part, Joint> = {
  head: 'head',
  body: 'torso',
  rightArm: 'rightArm',
  leftArm: 'leftArm',
  rightLeg: 'rightLeg',
  leftLeg: 'leftLeg',
};
const JOINTS: Joint[] = ['torso', 'head', 'rightArm', 'leftArm', 'rightLeg', 'leftLeg'];
const PARENT: Record<Joint, Joint | null> = {
  torso: null,
  head: 'torso',
  rightArm: 'torso',
  leftArm: 'torso',
  rightLeg: null,
  leftLeg: null,
};

/** What each pose frames: the camera's target height and how much must fit. */
const FRAMING: Record<Pose, { target: number; height: number; width: number }> = {
  still: { target: 13, height: 50, width: 30 },
  walk: { target: 13, height: 50, width: 30 },
  run: { target: 13.5, height: 51, width: 32 },
  crouch: { target: 12, height: 48, width: 30 },
  sit: { target: 8, height: 42, width: 34 },
  wave: { target: 15, height: 54, width: 32 },
  boat: { target: 8, height: 44, width: 40 },
  horse: { target: 22, height: 60, width: 46 },
};

const START = { yaw: -0.55, pitch: 0.14, zoom: 1 };

export function createStage(canvas: HTMLCanvasElement): Stage {
  const renderer = new WebGLRenderer({ canvas, antialias: false, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.setClearColor(0x000000, 0);
  const scene = new Scene();
  const camera = new PerspectiveCamera(32, 1, 1, 600);
  const motion = window.matchMedia?.('(prefers-reduced-motion: reduce)');

  const world = new Group();
  scene.add(world);
  const ride = new Group();
  world.add(ride);
  const theBoat = boat();
  const theHorse = horse();
  ride.add(theBoat.group, theHorse.group);
  const figure = new Group();
  ride.add(figure);

  const joints = Object.fromEntries(JOINTS.map((j) => [j, new Group()])) as Record<Joint, Group>;
  for (const j of JOINTS) (PARENT[j] ? joints[PARENT[j]] : figure).add(joints[j]);

  const data = new Uint8Array(SKIN_SIZE * SKIN_SIZE * 4);
  const texture = new DataTexture(data, SKIN_SIZE, SKIN_SIZE, RGBAFormat, UnsignedByteType);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.colorSpace = SRGBColorSpace;
  texture.needsUpdate = true;
  const baseMaterial = new MeshBasicMaterial({ map: texture, vertexColors: true, alphaTest: 0.5, side: FrontSide });
  // The outer layer keeps part see-through paint part see-through, as the game draws it.
  const outerMaterial = new MeshBasicMaterial({ map: texture, vertexColors: true, alphaTest: 0.02, transparent: true, side: DoubleSide });

  let meshes: { mesh: Mesh; box: Box }[] = [];
  /** Each outer box's texel-block sides, rebuilt when what is painted on the outer layer changes. */
  let wallMeshes: { mesh: Mesh; box: Box; walls: Wall[] }[] = [];
  let wallKey = '';
  const hidden = new Set<string>();
  let paintLayer: Layer = 'base';
  let pose: Pose = 'still';
  let model: SkinModel = 'classic';
  const view = { ...START };
  let fit = { half: Math.tan((32 * Math.PI) / 360), aspect: 1 };
  let frame = 0;
  const clock = new AnimationClock(performance.now());
  let celebratedAt = -Infinity;

  const still = () => motion?.matches === true;

  const place = (now: number) => {
    const t = still() ? POSE_STILL_T : clock.at(now);
    const rig = rigAt(pose, t);
    for (const j of JOINTS) {
      const pivot = pivotOf(j, model);
      const parent = PARENT[j] ? pivotOf(PARENT[j], model) : ([0, 0, 0] as Vec3);
      joints[j].position.set(pivot[0] - parent[0], pivot[1] - parent[1], pivot[2] - parent[2]);
      joints[j].rotation.set(...rig.joints[j], 'XYZ');
    }
    figure.position.set(...rig.offset);
    ride.rotation.set(...rig.tilt);
    theBoat.group.visible = pose === 'boat';
    theHorse.group.visible = pose === 'horse';
    if (pose === 'boat') {
      const row = Math.sin(t * Math.PI * 2 * 0.6);
      theBoat.oars.forEach((oar, i) => (oar.rotation.y = (i === 0 ? -1 : 1) * (0.35 + row * 0.45)));
    }
    if (pose === 'horse') {
      const gait = Math.sin(t * Math.PI * 2 * 1.05);
      theHorse.legs.forEach((leg, i) => (leg.rotation.x = (i === 0 || i === 3 ? 1 : -1) * gait * 0.45));
      theHorse.head.rotation.x = 0.52 + Math.sin(t * Math.PI * 2 * 1.05 * 2) * 0.05;
      theHorse.group.position.y = Math.abs(gait) * 0.5;
      theHorse.tail.rotation.z = Math.sin(t * Math.PI * 2 * 0.8) * 0.18;
    }
    const party = still() ? null : celebration((now - celebratedAt) / 1000);
    world.rotation.y = party?.spin ?? 0;
    world.position.y = party?.lift ?? 0;
  };

  const draw = (now: number) => {
    frame = 0;
    place(now);
    const framing = FRAMING[pose];
    const distance = Math.max(framing.height / 2 / fit.half, framing.width / 2 / (fit.half * fit.aspect)) * view.zoom;
    const { yaw, pitch } = view;
    camera.position.set(
      distance * Math.cos(pitch) * Math.sin(yaw),
      framing.target + distance * Math.sin(pitch),
      distance * Math.cos(pitch) * Math.cos(yaw),
    );
    camera.lookAt(0, framing.target, 0);
    renderer.render(scene, camera);
    // The figure is always a little alive (breathing, looking around) until the player pauses it; the celebration
    // plays even then. Reduced motion holds it still. A still stage redraws only when something changes.
    const celebrating = now - celebratedAt < CELEBRATE_S * 1000;
    if (!still() && (!clock.paused || celebrating)) frame = requestAnimationFrame(draw);
  };
  const invalidate = () => {
    if (!frame) frame = requestAnimationFrame(draw);
  };

  const build = (next: SkinModel) => {
    model = next;
    for (const { mesh } of meshes) {
      mesh.removeFromParent();
      mesh.geometry.dispose();
    }
    meshes = boxes(next).map((box) => {
      const joint = JOINT_OF[box.part];
      const mesh = new Mesh(geometryFor(box, pivotOf(joint, next)), box.layer === 'base' ? baseMaterial : outerMaterial);
      mesh.visible = !hidden.has(`${box.part}:${box.layer}`);
      joints[joint].add(mesh);
      return { mesh, box };
    });
    wallKey = '';
    buildWalls();
    invalidate();
  };

  /** Rebuilds the outer layer's texel-block sides when what is painted on it changes. */
  function buildWalls() {
    const solid = (x: number, y: number) => data[(y * SKIN_SIZE + x) * 4 + 3]! > 0;
    const outer = boxes(model).filter((box) => box.layer === 'outer');
    let key = model;
    for (const box of outer) for (const f of box.faces) for (let y = f.y; y < f.y + f.h; y++) for (let x = f.x; x < f.x + f.w; x++) key += solid(x, y) ? '1' : '0';
    if (key === wallKey) return;
    wallKey = key;
    for (const { mesh } of wallMeshes) {
      mesh.removeFromParent();
      mesh.geometry.dispose();
    }
    wallMeshes = outer.map((box) => {
      const walls = outerWalls(box, solid);
      const joint = JOINT_OF[box.part];
      const mesh = new Mesh(wallGeometry(walls, pivotOf(joint, model)), outerMaterial);
      mesh.visible = !hidden.has(`${box.part}:${box.layer}`);
      joints[joint].add(mesh);
      return { mesh, box, walls };
    });
  }
  build('classic');

  const raycaster = new Raycaster();
  const pointer = new Vector2();
  const onMotion = () => invalidate();
  motion?.addEventListener?.('change', onMotion);

  return {
    setTexture(next) {
      data.set(next);
      texture.needsUpdate = true;
      buildWalls();
      invalidate();
    },
    setModel: build,
    setVisible(part, layer, visible) {
      const key = `${part}:${layer}`;
      if (visible) hidden.delete(key);
      else hidden.add(key);
      for (const { mesh, box } of [...meshes, ...wallMeshes]) if (box.part === part && box.layer === layer) mesh.visible = visible;
      invalidate();
    },
    setPaintLayer(layer) {
      paintLayer = layer;
    },
    setPose(next) {
      pose = next;
      invalidate();
    },
    setPaused(paused) {
      const now = performance.now();
      if (paused) clock.pause(now);
      else clock.resume(now);
      invalidate();
    },
    celebrate() {
      celebratedAt = performance.now();
      invalidate();
    },
    pick(clientX, clientY) {
      const rect = canvas.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return null;
      pointer.set(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
      scene.updateMatrixWorld();
      raycaster.setFromCamera(pointer, camera);
      const targets: { mesh: Mesh; box: Box; walls?: Wall[] }[] = [...meshes, ...wallMeshes].filter(({ mesh, box }) => mesh.visible && box.layer === paintLayer);
      const hit = raycaster.intersectObjects(targets.map((t) => t.mesh), false)[0];
      if (!hit || !hit.uv || hit.faceIndex == null) return null;
      const owner = targets.find((t) => t.mesh === hit.object)!;
      const quad = Math.floor(hit.faceIndex / 2);
      if (owner.walls) {
        const [x, y] = owner.walls[quad]!.texel;
        return { x, y, part: owner.box.part, layer: owner.box.layer };
      }
      const face = owner.box.faces[quad]!;
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
      fit = { half: Math.tan((camera.fov * Math.PI) / 360), aspect: camera.aspect };
      invalidate();
    },
    dispose() {
      if (frame) cancelAnimationFrame(frame);
      motion?.removeEventListener?.('change', onMotion);
      for (const { mesh } of [...meshes, ...wallMeshes]) mesh.geometry.dispose();
      texture.dispose();
      baseMaterial.dispose();
      outerMaterial.dispose();
      renderer.dispose();
    },
  };
}

