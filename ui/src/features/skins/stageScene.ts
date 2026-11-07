// The 3D figure on the Skins stage, drawn with three.js. Faces are lit the way the game lights blocks (top brightest,
// sides darker), with no lights to ease between; the texture is nearest-filtered so every texel stays a crisp square.
// The figure is a jointed rig that poses (walk, run, crouch, sit, wave, row a boat, ride a horse); the boat and the
// horse are box models built like the game's entities, each texel of them painted here in the launcher's own colours
// (see props), not Mojang's textures. The
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
  RGBAFormat,
  Scene,
  SRGBColorSpace,
  UnsignedByteType,
  Vector2,
  WebGLRenderer,
} from 'three';
import type { SkinModel } from '../../ipc/schema';
import { boxes, SKIN_SIZE, type Box, type Face, type Layer, type Part, type Vec3 } from './layout';
import { outerWalls, type Wall } from './outerWalls';
import { AnimationClock, celebration, CELEBRATE_S, HANDLE, OARLOCK, paddleAt, POSE_STILL_T, rigAt, type Joint, type Pose } from './poses';
import { bake, fromFront, fur, hair, hash, hex, leather, metal, mul, over, planks, type Painter, type Piece } from './props';

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

/** A box's faces, placed relative to the joint it hangs from, textured from a `texW` × `texH` texture. */
function geometryFor(box: { faces: Face[] }, pivot: Vec3, texW = SKIN_SIZE, texH = SKIN_SIZE): BufferGeometry {
  const positions: number[] = [];
  const uvs: number[] = [];
  const colors: number[] = [];
  const index: number[] = [];
  box.faces.forEach((face, i) => {
    const [lt, rt, lb, rb] = face.corners;
    const u0 = face.x / texW;
    const u1 = (face.x + face.w) / texW;
    const v0 = face.y / texH;
    const v1 = (face.y + face.h) / texH;
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

/** A prop under construction: each piece hangs from a group (a joint that animates, or the prop itself). */
type PropParts = { group: Group; piece: Piece }[];

/** Bakes a prop's pieces onto one painted atlas and hangs their meshes from their groups. */
function buildProp(parts: PropParts): void {
  const { atlas, faces } = bake(parts.map((p) => p.piece));
  const texture = new DataTexture(atlas.data, atlas.width, atlas.height, RGBAFormat, UnsignedByteType);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.colorSpace = SRGBColorSpace;
  texture.needsUpdate = true;
  const material = new MeshBasicMaterial({ map: texture, vertexColors: true, side: FrontSide });
  parts.forEach(({ group }, n) => group.add(new Mesh(geometryFor({ faces: faces[n]! }, [0, 0, 0], atlas.width, atlas.height), material)));
}

const OAK = hex(0xa47b4a);
const OAK_LIGHT = hex(0xb8915c);

/** The paddle's grip: the last few texels at its inboard (+z) end, wrapped darker. */
const grip: Painter = (face, put) => {
  for (let j = 0; j < face.h; j++) {
    for (let i = 0; i < face.w; i++) {
      const end =
        face.name === 'front' ||
        ((face.name === 'top' || face.name === 'bottom') && j >= face.h - 3) ||
        ((face.name === 'right' || face.name === 'left') && fromFront(face, i) < 3);
      if (end) put(i, j, mul(OAK, 0.62 + ((i + j) % 2) * 0.06));
    }
  }
};

/** A rowing boat shaped like the game's: a flat bottom, low sides and ends of oak planks, and a paddle pinned on each
 *  rail that the rider pulls. Painted here, not Mojang's texture; no water under it. */
function boat(): { group: Group; paddles: Group[] } {
  const group = new Group();
  const parts: PropParts = [
    { group, piece: { size: [16, 3, 28], center: [0, 1.5, 0], paint: planks(mul(OAK, 0.88), 5) } },
    { group, piece: { size: [2, 6, 28], center: [-9, 6, 0], paint: planks(OAK, 6) } },
    { group, piece: { size: [2, 6, 28], center: [9, 6, 0], paint: planks(OAK, 7) } },
    { group, piece: { size: [20, 6, 2], center: [0, 6, -15], paint: planks(OAK, 8) } },
    { group, piece: { size: [20, 6, 2], center: [0, 6, 15], paint: planks(OAK, 9) } },
  ];
  const paddles: Group[] = [];
  for (const side of [-1, 1] as const) {
    parts.push({ group, piece: { size: [1.6, 1, 1.6], center: [side * OARLOCK.x, 9.5, OARLOCK.z], paint: metal(hex(0x8e9398)) } });
    const paddle = new Group();
    paddle.position.set(side * OARLOCK.x, OARLOCK.y, OARLOCK.z);
    paddle.rotation.order = 'YXZ';
    // The shaft crosses the rail: the handle reaches HANDLE inboard (+z), the rest runs out to the blade.
    parts.push({ group: paddle, piece: { size: [2, 2, 18], center: [0, 0, HANDLE - 9], paint: over(planks(OAK_LIGHT, 10 + side), grip) } });
    parts.push({ group: paddle, piece: { size: [1, 6, 7], center: [0, 0, HANDLE - 21.5], paint: planks(OAK, 12 + side) } });
    group.add(paddle);
    paddles.push(paddle);
  }
  buildProp(parts);
  return { group, paddles };
}

const COAT = hex(0xa0673a);
const MANE = hex(0x3a2416);
const HOOF = hex(0x3b332d);
const SOCK = hex(0xe9e2d3);
const BLAZE = hex(0xf0e9dc);
const TACK = hex(0x5c3722);
const IRON = hex(0x9ea3a8);

/** A leg: coat down to a hoof (its top row a lighter band), with a white sock above it on some legs. */
function leg(sock: boolean): Painter {
  return over(fur(COAT, 21), (face, put) => {
    for (let j = 0; j < face.h; j++) {
      for (let i = 0; i < face.w; i++) {
        const up = face.name === 'bottom' ? 0 : face.name === 'top' ? 99 : face.h - 1 - j;
        if (up < 3) put(i, j, mul(HOOF, up === 2 ? 1.25 : 0.95 + hash(i, j, 22) * 0.1));
        else if (sock && up < 6) put(i, j, mul(SOCK, 0.94 + hash(face.x + i, j, 23) * 0.08));
      }
    }
  });
}

/** The head: eyes on its sides near the front, a white blaze down the face. */
const skull: Painter = over(fur(COAT, 24), (face, put) => {
  if (face.name === 'right' || face.name === 'left') {
    for (let i = 0; i < face.w; i++) {
      const k = fromFront(face, i);
      if (k === 2) put(i, 1, hex(0x1a130e));
      if (k === 3) put(i, 1, hex(0x2e2118));
      if (k === 2 || k === 3) put(i, 2, mul(COAT, 0.8));
    }
  }
  const mid = (i: number) => Math.abs(i - (face.w - 1) / 2) <= 0.5;
  if (face.name === 'front') for (let j = 0; j < face.h; j++) for (let i = 0; i < face.w; i++) if (mid(i)) put(i, j, BLAZE);
  if (face.name === 'top') for (let j = face.h - 3; j < face.h; j++) for (let i = 0; i < face.w; i++) if (mid(i)) put(i, j, BLAZE);
});

/** The muzzle: the blaze over its top, nostrils and a mouth line in front, the mouth carried along its sides. */
const muzzle: Painter = over(fur(mul(COAT, 0.92), 25), (face, put) => {
  const mid = (i: number) => Math.abs(i - (face.w - 1) / 2) <= 0.5;
  if (face.name === 'top') for (let j = 0; j < face.h; j++) for (let i = 0; i < face.w; i++) if (mid(i)) put(i, j, BLAZE);
  if (face.name === 'front') {
    for (let i = 0; i < face.w; i++) {
      if (mid(i)) put(i, 0, BLAZE);
      put(i, 3, mul(COAT, 0.62));
    }
    put(0, 1, hex(0x24180f));
    put(face.w - 1, 1, hex(0x24180f));
  }
  if (face.name === 'right' || face.name === 'left') for (let i = 0; i < face.w; i++) if (fromFront(face, i) < 3) put(i, 3, mul(COAT, 0.62));
});

/** An ear: its inside shaded. */
const ear: Painter = over(fur(COAT, 26), (face, put) => {
  if (face.name === 'front') for (let j = 1; j < face.h; j++) for (let i = 0; i < face.w; i++) put(i, j, mul(COAT, 0.66));
});

/** The saddle's seat: leather wrapped over the back, with the girth strap down each side. */
const saddle: Painter = over(leather(TACK, 27), (face, put) => {
  if (face.name === 'right' || face.name === 'left') {
    for (let j = 0; j < face.h; j++) for (let i = 0; i < face.w; i++) if (Math.abs(fromFront(face, i) - (face.w - 1) / 2) <= 1) put(i, j, mul(TACK, 0.68));
  }
});

/** A horse shaped like the game's: a long body on four legs, a raised neck with its mane, a head with a muzzle and
 *  ears, and a tail; saddled and bridled. A chestnut coat with a white blaze and two white socks, painted here, not
 *  Mojang's texture. */
function horse(): { group: Group; legs: Group[]; head: Group; tail: Group } {
  const group = new Group();
  const parts: PropParts = [
    { group, piece: { size: [10, 10, 22], center: [0, 16, 0], paint: fur(COAT, 20) } },
    { group, piece: { size: [10.8, 6.2, 9], center: [0, 19, -1], paint: saddle } },
    { group, piece: { size: [3, 2, 1.6], center: [0, 23, 2.6], paint: leather(mul(TACK, 1.1), 28) } },
    { group, piece: { size: [7, 2, 1.6], center: [0, 23, -4.6], paint: leather(mul(TACK, 1.1), 29) } },
  ];
  for (const side of [-1, 1]) {
    parts.push({ group, piece: { size: [0.4, 4, 0.8], center: [side * 5.6, 14, -1], paint: leather(mul(TACK, 0.8), 30) } });
    parts.push({ group, piece: { size: [1, 1, 2.4], center: [side * 5.7, 11.5, -1], paint: metal(IRON) } });
  }
  const legs: Group[] = [];
  for (const [x, z, sock] of [[-3, 8.5, true], [3, 8.5, false], [-3, -8.5, false], [3, -8.5, true]] as const) {
    const g = new Group();
    g.position.set(x, 11, z);
    parts.push({ group: g, piece: { size: [4, 11, 4], center: [0, -5.5, 0], paint: leg(sock) } });
    group.add(g);
    legs.push(g);
  }
  const head = new Group();
  head.position.set(0, 20, 10);
  parts.push(
    { group: head, piece: { size: [4, 12, 7], center: [0, 5, -1.5], paint: fur(COAT, 31) } },
    { group: head, piece: { size: [2, 16, 2], center: [0, 4, -6], paint: hair(MANE, 32) } },
    { group: head, piece: { size: [6, 5, 7], center: [0, 8.5, 3.5], paint: skull } },
    { group: head, piece: { size: [4, 5, 5], center: [0, 8.5, 9.5], paint: muzzle } },
    { group: head, piece: { size: [3, 1.5, 2.5], center: [0, 11.75, 1.4], paint: hair(MANE, 33) } },
    // The bridle: a band round the head behind the eyes, a noseband round the muzzle, the bit's rings.
    { group: head, piece: { size: [6.4, 5.4, 0.8], center: [0, 8.5, 1], paint: leather(TACK, 34) } },
    { group: head, piece: { size: [4.4, 1.2, 5], center: [0, 8, 9.8], paint: leather(TACK, 35) } },
  );
  for (const side of [-1, 1]) {
    parts.push({ group: head, piece: { size: [2, 3, 1], center: [side * 2, 12.5, 0.5], paint: ear } });
    parts.push({ group: head, piece: { size: [0.6, 1.2, 1.2], center: [side * 2.4, 7.4, 11], paint: metal(IRON) } });
  }
  group.add(head);
  const tail = new Group();
  tail.position.set(0, 20, -11);
  parts.push({ group: tail, piece: { size: [3, 14, 4], center: [0, -7, -2], paint: hair(MANE, 36) } });
  tail.rotation.x = 0.4;
  group.add(tail);
  buildProp(parts);
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
  boat: { target: 9, height: 44, width: 56 },
  horse: { target: 22, height: 60, width: 50 },
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
    if (pose === 'boat') {
      theBoat.paddles.forEach((paddle, i) => {
        const { yaw, dip } = paddleAt(t, i === 0 ? -1 : 1);
        paddle.rotation.set(dip, yaw, 0, 'YXZ');
      });
    }
    theHorse.group.visible = pose === 'horse';
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

