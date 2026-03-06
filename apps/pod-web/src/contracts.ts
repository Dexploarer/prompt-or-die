export type Vec3Tuple = [number, number, number];
export type Vec4Tuple = [number, number, number, number];
export type RgbaTuple = [number, number, number, number];

export interface CameraState {
  x: number;
  y: number;
  zoom: number;
  rotation: number;
  viewportWidth: number;
  viewportHeight: number;
}

export interface RenderCommand {
  type: string;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  scaleX: number;
  scaleY: number;
  color: RgbaTuple;
  alpha: number;
  texture?: string;
  frame?: number;
  mesh?: string;
  material?: string;
  z?: number;
  transform3d?: {
    position: Vec3Tuple;
    rotation: Vec4Tuple;
    scale: Vec3Tuple;
  };
  billboard?: boolean;
  castShadows?: boolean;
  receiveShadows?: boolean;
  transparent?: boolean;
  doubleSided?: boolean;
  roughness?: number;
  metallic?: number;
  emissive?: Vec3Tuple;
  layer: number;
  visible: boolean;
  sourceEntity?: number;
}

export interface RenderFrame {
  camera: CameraState;
  commands: RenderCommand[];
  backgroundColor: RgbaTuple;
}

export interface ThreeJsInstance {
  position: Vec3Tuple;
  rotation: Vec4Tuple;
  scale: Vec3Tuple;
  color?: RgbaTuple;
  sourceEntity?: number;
}

export type ThreeJsRenderPhase = "opaque" | "transparent";

export interface ThreeJsMeshBatch {
  mesh: string;
  material: string;
  layer: number;
  phase: ThreeJsRenderPhase;
  sortDepth: number;
  renderOrder: number;
  transparent: boolean;
  doubleSided: boolean;
  castShadows: boolean;
  receiveShadows: boolean;
  tint: RgbaTuple;
  roughness: number;
  metallic: number;
  emissive: Vec3Tuple;
  depthWrite: boolean;
  depthTest: boolean;
  instances: ThreeJsInstance[];
}

export interface ThreeJsSpriteBatch {
  texture: string;
  frame: number;
  layer: number;
  billboard: boolean;
  phase: ThreeJsRenderPhase;
  sortDepth: number;
  renderOrder: number;
  transparent: boolean;
  depthWrite: boolean;
  depthTest: boolean;
  instances: ThreeJsInstance[];
}

export interface ThreeJsWebGpuHints {
  renderer: string;
  preferredBackend: string;
  fallbackBackend: string;
  useInstancing: boolean;
  sortMetric: string;
  sortOpaqueFrontToBack: boolean;
  preserveInstanceOrder: boolean;
  sortTransparentBackToFront: boolean;
  transparentInstancingStrategy: string;
  opaqueDepthWrite: boolean;
  transparentDepthWrite: boolean;
  maxPixelRatio: number;
}

export interface ThreeJsWebGpuFrame {
  camera: CameraState;
  backgroundColor: RgbaTuple;
  overlayCommands: RenderCommand[];
  meshBatches: ThreeJsMeshBatch[];
  spriteBatches: ThreeJsSpriteBatch[];
  hints: ThreeJsWebGpuHints;
}

export function parseThreeJsWebGpuFrame(
  frame: string | ThreeJsWebGpuFrame
): ThreeJsWebGpuFrame {
  return typeof frame === "string"
    ? (JSON.parse(frame) as ThreeJsWebGpuFrame)
    : frame;
}

export function parseRenderFrame(frame: string | RenderFrame): RenderFrame {
  return typeof frame === "string" ? (JSON.parse(frame) as RenderFrame) : frame;
}

export function legacyFrameToThreeJsFrame(frame: RenderFrame): ThreeJsWebGpuFrame {
  return {
    camera: frame.camera,
    backgroundColor: frame.backgroundColor,
    overlayCommands: frame.commands.filter((command) => command.visible),
    meshBatches: [],
    spriteBatches: [],
    hints: {
      renderer: "three/webgpu",
      preferredBackend: "webgpu",
      fallbackBackend: "webgl2",
      useInstancing: true,
      sortMetric: "world-z",
      sortOpaqueFrontToBack: true,
      preserveInstanceOrder: true,
      sortTransparentBackToFront: true,
      transparentInstancingStrategy: "shared-sort-depth",
      opaqueDepthWrite: true,
      transparentDepthWrite: false,
      maxPixelRatio: 2
    }
  };
}
