import * as THREE from "three";

import {
  createManifestBackedAssetRegistry,
  createMeshMaterial,
  createSpriteMaterial,
  DefaultPodThreeAssetRegistry,
  OVERLAY_PLANE_GEOMETRY,
  SPRITE_PLANE_GEOMETRY,
  type PodThreeAssetRegistry
} from "./assets";
import {
  DEFAULT_WORLD_CHUNK_SIZE,
  buildCameraPose,
  buildFramePlan,
  composeAnimatedInstanceMatrix,
  planningOptionsFromQuality,
  type PodThreeCameraRigOptions,
  type PlannedMeshBatch,
  type PlannedSpriteBatch
} from "./frame-plan";
import {
  legacyFrameToThreeJsFrame,
  type NetworkGameEvent,
  type RenderCommand,
  type RenderFrame,
  type TelemetryTrajectorySample,
  type ThreeJsEnvironment,
  type ThreeJsWebGpuFrame
} from "./contracts";
import {
  LANDSCAPE_PROFILE_ID,
  LANDSCAPE_WORLD_SIZE,
  WATER_CENTER,
  WATER_LEVEL,
  WATER_PROFILE_ID,
  WATER_RADII,
  clamp,
  describeEnvironmentPreset,
  fractalNoise,
  mixScalar,
  mixVec3,
  sampleLakeMask,
  sampleTerrainHeight,
  sampleTerrainSlope,
  sampleTimeLapseEnvironment,
  smoothstep
} from "./landscape";
import {
  resolveQualityProfile,
  type PodThreeQualityPreset,
  type PodThreeQualityProfile
} from "./quality";

type RuntimeRenderer = THREE.WebGLRenderer & {
  init?: () => Promise<void>;
  backend?: { isWebGPUBackend?: boolean };
  renderAsync?: (scene: THREE.Scene, camera: THREE.Camera) => Promise<void>;
};

type ThreeConsoleLevel = Parameters<typeof THREE.setConsoleFunction>[0] extends (
  level: infer Level,
  ...args: unknown[]
) => void
  ? Level
  : "log" | "warn" | "error";

interface InstancedEntry {
  capacity: number;
  mesh: THREE.InstancedMesh;
  material: THREE.Material;
}

const INLINE_TSL_FN_WARNING =
  "THREE.TSL: Return statement used in an inline 'Fn()'. Define a layout struct to allow return values.";
const DAYLIGHT_START_OFFSET_SECONDS = 86.25;
let installedThreeConsoleFilter = false;
let didReportInlineFnWarning = false;

function createPaintSurface(width: number, height: number): PaintSurface {
  if (typeof OffscreenCanvas === "function") {
    return new OffscreenCanvas(width, height);
  }

  if (typeof document === "object") {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    return canvas;
  }

  throw new Error("Canvas surface creation is unavailable in this environment");
}

function getPaintContext(surface: PaintSurface): OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D {
  const context = surface.getContext("2d");
  if (!context) {
    throw new Error("2D canvas context is unavailable");
  }
  return context;
}

function createTerrainGeometry(
  size = LANDSCAPE_WORLD_SIZE,
  segments = 168
): THREE.PlaneGeometry {
  const geometry = new THREE.PlaneGeometry(size, size, segments, segments);
  const positions = geometry.attributes.position;

  for (let index = 0; index < positions.count; index += 1) {
    const x = positions.getX(index);
    const y = positions.getY(index);
    const radialFalloff = clamp(1 - Math.hypot(x, y) / (size * 0.82), 0, 1);
    const height = sampleTerrainHeight(x, y) * (0.84 + (1 - radialFalloff) * 0.22);
    positions.setZ(index, height);
  }

  geometry.computeVertexNormals();
  return geometry;
}

function paintTerrainTexture(
  surface: PaintSurface,
  environment: ThreeJsEnvironment
): void {
  const context = getPaintContext(surface);
  const width = "width" in surface ? surface.width : 512;
  const height = "height" in surface ? surface.height : 512;
  const grass = mixVec3(
    environment.groundColor.slice(0, 3) as [number, number, number],
    [0.2, 0.35, 0.22],
    0.45
  );
  const moss = mixVec3(grass, [0.36, 0.49, 0.24], 0.52);
  const cliff = mixVec3(grass, [0.38, 0.35, 0.31], 0.78);
  const basalt: [number, number, number] = [0.22, 0.24, 0.28];
  const highland: [number, number, number] = [0.54, 0.52, 0.46];
  const sand: [number, number, number] = [0.72, 0.66, 0.48];
  const imageData = context.createImageData(width, height);
  const worldHalfSize = LANDSCAPE_WORLD_SIZE * 0.5;

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const worldX = (x / Math.max(width - 1, 1) - 0.5) * LANDSCAPE_WORLD_SIZE;
      const worldZ = (y / Math.max(height - 1, 1) - 0.5) * LANDSCAPE_WORLD_SIZE;
      const terrainHeight = sampleTerrainHeight(worldX, worldZ);
      const slope = sampleTerrainSlope(worldX, worldZ);
      const lake = sampleLakeMask(worldX, worldZ);
      const shore = lake * (1 - smoothstep(2.4, 8.4, Math.abs(terrainHeight - WATER_LEVEL)));
      const cliffMask = clamp((slope - 0.55) / 1.9 + Math.max(terrainHeight - 10, 0) / 18, 0, 1);
      const meadowNoise = fractalNoise(worldX * 0.12 + 8, worldZ * 0.12 - 12);
      const ridgeNoise = fractalNoise(worldX * 0.032 - 14, worldZ * 0.032 + 21);
      const distanceFalloff = 1 - clamp(Math.hypot(worldX, worldZ) / worldHalfSize, 0, 1);
      const highlandMask = clamp((terrainHeight - 16) / 16, 0, 1);
      const rockMask = clamp(cliffMask * 0.7 + highlandMask * 0.8 + ridgeNoise * 0.24, 0, 1);
      const foamMask = clamp(shore * 1.35, 0, 1);

      let tint = mixVec3(grass, moss, meadowNoise * 0.75 + distanceFalloff * 0.15);
      tint = mixVec3(tint, sand, shore * 0.7);
      tint = mixVec3(tint, cliff, cliffMask * 0.72);
      tint = mixVec3(tint, highland, highlandMask * 0.52);
      tint = mixVec3(tint, basalt, rockMask * 0.58);
      tint = mixVec3(tint, [0.92, 0.9, 0.82], foamMask * 0.14);

      const brightness = clamp(
        0.72 +
          terrainHeight * 0.011 -
          cliffMask * 0.06 +
          meadowNoise * 0.12 +
          foamMask * 0.08,
        0.34,
        1.18
      );
      const index = (y * width + x) * 4;
      imageData.data[index] = Math.round(tint[0] * brightness * 255);
      imageData.data[index + 1] = Math.round(tint[1] * brightness * 255);
      imageData.data[index + 2] = Math.round(tint[2] * brightness * 255);
      imageData.data[index + 3] = 255;
    }
  }

  context.putImageData(imageData, 0, 0);
}

function paintWaterTexture(surface: PaintSurface): void {
  const context = getPaintContext(surface);
  const width = "width" in surface ? surface.width : 512;
  const height = "height" in surface ? surface.height : 512;
  const gradient = context.createLinearGradient(0, 0, 0, height);
  gradient.addColorStop(0, "rgb(178, 235, 244)");
  gradient.addColorStop(0.3, "rgb(92, 182, 214)");
  gradient.addColorStop(0.68, "rgb(29, 104, 152)");
  gradient.addColorStop(1, "rgb(10, 42, 78)");
  context.fillStyle = gradient;
  context.fillRect(0, 0, width, height);

  const radial = context.createRadialGradient(
    width * 0.52,
    height * 0.42,
    width * 0.08,
    width * 0.5,
    height * 0.5,
    width * 0.56
  );
  radial.addColorStop(0, "rgba(255,255,255,0.16)");
  radial.addColorStop(0.45, "rgba(170,232,255,0.08)");
  radial.addColorStop(1, "rgba(0,0,0,0)");
  context.fillStyle = radial;
  context.fillRect(0, 0, width, height);

  context.strokeStyle = "rgba(255, 255, 255, 0.14)";
  context.lineWidth = 2.5;
  for (let stripe = 0; stripe < 22; stripe += 1) {
    const offsetY = (stripe / 22) * height;
    context.beginPath();
    for (let x = 0; x <= width; x += 16) {
      const waveY =
        offsetY +
        Math.sin((x / width) * Math.PI * 4 + stripe * 0.9) * (5 + (stripe % 3) * 1.6);
      if (x === 0) {
        context.moveTo(x, waveY);
      } else {
        context.lineTo(x, waveY);
      }
    }
    context.stroke();
  }

  context.strokeStyle = "rgba(210, 246, 255, 0.24)";
  context.lineWidth = 1.5;
  for (let ring = 0; ring < 5; ring += 1) {
    const radius = width * (0.16 + ring * 0.11);
    context.beginPath();
    context.ellipse(width * 0.52, height * 0.5, radius, radius * 0.62, 0, 0, Math.PI * 2);
    context.stroke();
  }
}

function buildLakeOutline(
  pointCount = 56,
  radialScale = 1
): Array<[number, number]> {
  const points = new Array<[number, number]>();

  for (let index = 0; index < pointCount; index += 1) {
    const angle = (index / pointCount) * Math.PI * 2;
    const noise = 0.86 + fractalNoise(Math.cos(angle) * 12, Math.sin(angle) * 12) * 0.24;
    points.push([
      WATER_CENTER[0] + Math.cos(angle) * WATER_RADII[0] * noise * radialScale,
      WATER_CENTER[1] + Math.sin(angle) * WATER_RADII[1] * noise * radialScale
    ]);
  }

  return points;
}

function createClosedShape(points: Array<[number, number]>): THREE.Shape {
  const shape = new THREE.Shape();

  for (let index = 0; index <= points.length; index += 1) {
    const [x, z] = points[index % points.length] ?? points[0] ?? [0, 0];
    if (index === 0) {
      shape.moveTo(x, z);
    } else {
      shape.lineTo(x, z);
    }
  }

  return shape;
}

function createLakeGeometry(pointCount = 56): THREE.ShapeGeometry {
  const shape = createClosedShape(buildLakeOutline(pointCount));

  const geometry = new THREE.ShapeGeometry(shape, 18);
  geometry.rotateX(-Math.PI / 2);
  return geometry;
}

function createShorelineGeometry(pointCount = 56): THREE.ShapeGeometry {
  const outerShape = createClosedShape(buildLakeOutline(pointCount, 1.08));
  const innerPath = new THREE.Path();
  const innerPoints = buildLakeOutline(pointCount, 0.98);

  for (let index = innerPoints.length; index >= 0; index -= 1) {
    const [x, z] = innerPoints[index % innerPoints.length] ?? innerPoints[0] ?? [0, 0];
    if (index === innerPoints.length) {
      innerPath.moveTo(x, z);
    } else {
      innerPath.lineTo(x, z);
    }
  }

  outerShape.holes.push(innerPath);
  const geometry = new THREE.ShapeGeometry(outerShape, 18);
  geometry.rotateX(-Math.PI / 2);
  return geometry;
}

function createMountainBackdropGeometry(
  radius = LANDSCAPE_WORLD_SIZE * 0.68,
  width = 42,
  segments = 88
): THREE.BufferGeometry {
  const positions = new Float32Array((segments + 1) * 2 * 3);
  const indices = new Array<number>();

  for (let index = 0; index <= segments; index += 1) {
    const angle = (index / segments) * Math.PI * 2;
    const ridgeNoise = fractalNoise(Math.cos(angle) * 7 + 4, Math.sin(angle) * 7 - 9);
    const peakHeight = 28 + ridgeNoise * 42;
    const innerRadius = radius;
    const outerRadius = radius + width + ridgeNoise * 18;
    const sin = Math.sin(angle);
    const cos = Math.cos(angle);
    const offset = index * 6;

    positions[offset] = cos * innerRadius;
    positions[offset + 1] = -6;
    positions[offset + 2] = sin * innerRadius;

    positions[offset + 3] = cos * outerRadius;
    positions[offset + 4] = peakHeight;
    positions[offset + 5] = sin * outerRadius;

    if (index < segments) {
      const base = index * 2;
      indices.push(base, base + 1, base + 2, base + 1, base + 3, base + 2);
    }
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

function paintSkyTexture(surface: PaintSurface, environment: ThreeJsEnvironment): void {
  const context = getPaintContext(surface);
  const width = "width" in surface ? surface.width : 512;
  const height = "height" in surface ? surface.height : 512;
  const top = mixVec3(environment.skyColor.slice(0, 3) as [number, number, number], [0.9, 0.97, 1], 0.28);
  const horizon = mixVec3(environment.fogColor.slice(0, 3) as [number, number, number], [0.97, 0.91, 0.72], 0.38);

  const gradient = context.createLinearGradient(0, 0, 0, height);
  gradient.addColorStop(0, `rgb(${Math.round(top[0] * 255)}, ${Math.round(top[1] * 255)}, ${Math.round(top[2] * 255)})`);
  gradient.addColorStop(0.68, `rgb(${Math.round(horizon[0] * 255)}, ${Math.round(horizon[1] * 255)}, ${Math.round(horizon[2] * 255)})`);
  gradient.addColorStop(1, `rgb(${Math.round(environment.groundColor[0] * 255)}, ${Math.round(environment.groundColor[1] * 255)}, ${Math.round(environment.groundColor[2] * 255)})`);
  context.fillStyle = gradient;
  context.fillRect(0, 0, width, height);

  const sunGradient = context.createRadialGradient(width * 0.72, height * 0.28, 0, width * 0.72, height * 0.28, width * 0.18);
  sunGradient.addColorStop(0, "rgba(255, 246, 220, 0.92)");
  sunGradient.addColorStop(0.22, "rgba(255, 229, 162, 0.48)");
  sunGradient.addColorStop(1, "rgba(255, 229, 162, 0)");
  context.fillStyle = sunGradient;
  context.fillRect(0, 0, width, height);
}

function environmentSignature(environment: ThreeJsEnvironment): string {
  return [
    ...environment.skyColor.slice(0, 3),
    ...environment.fogColor.slice(0, 3),
    ...environment.groundColor.slice(0, 3),
    ...environment.sunColor,
    environment.sunIntensity,
    environment.starfieldIntensity
  ]
    .map((value) => value.toFixed(3))
    .join("|");
}

export interface PodThreeRendererStats {
  backend: "webgpu" | "webgl2";
  renderThread: "main" | "worker";
  qualityPreset: PodThreeQualityPreset;
  environmentPreset: "daylight" | "twilight" | "night";
  landscapeMode: typeof LANDSCAPE_PROFILE_ID;
  waterMode: typeof WATER_PROFILE_ID;
  timeOfDayHours: number;
  pixelRatio: number;
  drawCalls: number;
  triangles: number;
  textures: number;
  frameMs: number;
  residentGeometryAssets: number;
  residentSpriteAssets: number;
  pendingGeometryAssets: number;
  pendingSpriteAssets: number;
  ambientInstances: number;
  visibleWorldChunks: number;
  preloadedWorldChunks: number;
}

export interface RenderSurfaceMetrics {
  width: number;
  height: number;
  devicePixelRatio: number;
}

export interface PodThreeWorldRendererOptions {
  assetRegistry?: PodThreeAssetRegistry;
  backendPreference?: "auto" | "webgpu" | "webgl2";
  cameraRig?: PodThreeCameraRigOptions;
  qualityPreset?: PodThreeQualityPreset;
  qualityProfile?: Partial<PodThreeQualityProfile>;
  clearColor?: number;
  enableShadows?: boolean;
  showGrid?: boolean;
  maxPixelRatio?: number;
  surfaceMetrics?: RenderSurfaceMetrics;
}

type PaintSurface = OffscreenCanvas | HTMLCanvasElement;

interface AmbientChunkDressingPlan {
  meshBatches: PlannedMeshBatch[];
  prewarmRequests: Array<{ batch: PlannedMeshBatch["batch"]; lodLevel: 0 | 1 | 2 }>;
  totalInstances: number;
}

export class PodThreeWorldRenderer {
  static async create(
    canvas: HTMLCanvasElement | OffscreenCanvas,
    options: PodThreeWorldRendererOptions = {}
  ): Promise<PodThreeWorldRenderer> {
    const { renderer, backend } = await createRenderer(canvas, options);
    const assetRegistry =
      options.assetRegistry ??
      (await createManifestBackedAssetRegistry({
        renderer,
        fallbackRegistry: new DefaultPodThreeAssetRegistry()
      }));
    return new PodThreeWorldRenderer(canvas, renderer, backend, {
      ...options,
      assetRegistry
    });
  }

  readonly scene = new THREE.Scene();
  readonly overlayScene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(55, 1, 0.1, 1024);
  readonly overlayCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, -10, 10);
  readonly assetRegistry: PodThreeAssetRegistry;
  readonly backend: "webgpu" | "webgl2";
  readonly quality: PodThreeQualityProfile;

  private readonly meshEntries = new Map<string, InstancedEntry>();
  private readonly ambientMeshEntries = new Map<string, InstancedEntry>();
  private readonly spriteEntries = new Map<string, InstancedEntry>();
  private readonly meshMaterialCache = new Map<string, THREE.Material>();
  private readonly spriteMaterialCache = new Map<string, THREE.Material>();
  private readonly overlayObjects = new Array<THREE.Object3D>();
  private readonly resizeObserver: ResizeObserver | null;
  private readonly options: PodThreeWorldRendererOptions;
  private hemisphereLight: THREE.HemisphereLight | null = null;
  private sunLight: THREE.DirectionalLight | null = null;
  private fillLight: THREE.DirectionalLight | null = null;
  private rimLight: THREE.PointLight | null = null;
  private groundMaterial: THREE.MeshStandardMaterial | null = null;
  private terrainMesh: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshStandardMaterial> | null =
    null;
  private terrainTexture: THREE.Texture | null = null;
  private terrainTextureSurface: PaintSurface | null = null;
  private waterMesh: THREE.Mesh<THREE.ShapeGeometry, THREE.MeshStandardMaterial> | null = null;
  private shorelineMesh: THREE.Mesh<THREE.ShapeGeometry, THREE.MeshStandardMaterial> | null = null;
  private waterTexture: THREE.Texture | null = null;
  private waterTextureSurface: PaintSurface | null = null;
  private mountainBackdrop:
    | THREE.Mesh<THREE.BufferGeometry, THREE.MeshStandardMaterial>
    | null = null;
  private skyDome: THREE.Mesh<THREE.SphereGeometry, THREE.MeshBasicMaterial> | null = null;
  private skyTexture: THREE.Texture | null = null;
  private skyTextureSurface: PaintSurface | null = null;
  private sunOrb: THREE.Mesh<THREE.SphereGeometry, THREE.MeshBasicMaterial> | null = null;
  private starfieldMaterial: THREE.PointsMaterial | null = null;
  private adaptivePixelRatio: number;
  private smoothedFrameMs = 16.7;
  private adjustmentCooldown = 0;
  private surfaceMetrics: RenderSurfaceMetrics | null;
  private ambientInstances = 0;
  private visibleWorldChunks = 0;
  private preloadedWorldChunks = 0;
  private environmentPreset: "daylight" | "twilight" | "night" = "daylight";
  private timeOfDayHours = 12;
  private lastEnvironmentSignature: string | null = null;
  private baseEnvironment: ThreeJsEnvironment | null = null;
  private readonly smoothedCameraTarget = new THREE.Vector3();
  private cameraPoseInitialized = false;
  private lastCameraUpdateAt = 0;
  private telemetryTrail: THREE.Line<THREE.BufferGeometry, THREE.LineBasicMaterial> | null =
    null;
  private readonly entityPulseUntilMs = new Map<number, number>();

  constructor(
    private readonly canvas: HTMLCanvasElement | OffscreenCanvas,
    private readonly renderer: RuntimeRenderer,
    backend: "webgpu" | "webgl2",
    options: PodThreeWorldRendererOptions
  ) {
    this.backend = backend;
    this.assetRegistry = options.assetRegistry ?? new DefaultPodThreeAssetRegistry();
    this.surfaceMetrics = options.surfaceMetrics ?? null;
    const deviceMemory = readNavigatorDeviceMemory();
    const baseQuality = resolveQualityProfile({
      backend,
      preferredPreset: options.qualityPreset,
      hardwareConcurrency: readNavigatorHardwareConcurrency(),
      deviceMemory,
      devicePixelRatio: this.surfaceMetrics?.devicePixelRatio ?? readDevicePixelRatio()
    });
    this.quality = {
      ...baseQuality,
      ...options.qualityProfile,
      maxPixelRatio: Math.min(
        options.maxPixelRatio ??
          options.qualityProfile?.maxPixelRatio ??
          Number.POSITIVE_INFINITY,
        options.qualityProfile?.maxPixelRatio ?? baseQuality.maxPixelRatio
      ),
      enableShadows:
        options.enableShadows ??
        options.qualityProfile?.enableShadows ?? baseQuality.enableShadows,
      showGrid:
        options.showGrid ??
        options.qualityProfile?.showGrid ?? baseQuality.showGrid
    };
    this.options = options;
    this.adaptivePixelRatio = this.quality.maxPixelRatio;

    this.bootstrapScenes();
    const htmlCanvas =
      isHtmlCanvasElement(canvas) && typeof ResizeObserver !== "undefined" ? canvas : null;
    this.resizeObserver = htmlCanvas ? new ResizeObserver(() => this.resize()) : null;
    if (htmlCanvas && this.resizeObserver) {
      this.resizeObserver.observe(htmlCanvas);
    }
    this.resize();
  }

  async applyFrame(frame: ThreeJsWebGpuFrame): Promise<void> {
    this.applyEnvironment(frame.environment);

    const planned = buildFramePlan(frame, {
      ...this.options.cameraRig,
      ...planningOptionsFromQuality(this.quality)
    });
    this.visibleWorldChunks = planned.visibleWorldChunks.length;
    this.preloadedWorldChunks = planned.preloadedWorldChunks.length;
    const ambientPlan = buildAmbientChunkDressingPlan({
      visibleChunkKeys: planned.visibleWorldChunks,
      preloadedChunkKeys: planned.preloadedWorldChunks,
      cameraPosition: planned.camera.position,
      qualityPreset: this.quality.preset,
      worldChunkSize: planned.worldChunkSize,
      highDetailDistance: this.quality.highDetailDistance,
      mediumDetailDistance: this.quality.mediumDetailDistance
    });
    this.ambientInstances = ambientPlan.totalInstances;
    this.applyCamera(planned.camera, frame.camera);
    await this.prewarmPlannedAssets(planned, ambientPlan);
    await this.syncMeshBatches(planned.meshBatches);
    await this.syncAmbientMeshBatches(ambientPlan.meshBatches);
    await this.syncSpriteBatches(planned.spriteBatches);
    await this.syncOverlay(frame.overlayCommands);
    await this.renderFrame();
  }

  async applyLegacyFrame(frame: RenderFrame): Promise<void> {
    await this.applyFrame(legacyFrameToThreeJsFrame(frame));
  }

  notifyWorldEvents(events: NetworkGameEvent[]): void {
    if (events.length === 0) {
      return;
    }

    const now =
      typeof performance !== "undefined" && typeof performance.now === "function"
        ? performance.now()
        : Date.now();

    for (const event of events) {
      const durationMs = pulseDurationForEvent(event);
      if (durationMs <= 0) {
        continue;
      }

      for (const entityId of event.entityIds) {
        const existing = this.entityPulseUntilMs.get(entityId) ?? 0;
        this.entityPulseUntilMs.set(entityId, Math.max(existing, now + durationMs));
      }
    }
  }

  dispose(): void {
    this.resizeObserver?.disconnect();

    for (const entry of this.meshEntries.values()) {
      disposeInstancedEntry(entry);
    }
    for (const entry of this.ambientMeshEntries.values()) {
      disposeInstancedEntry(entry);
    }
    for (const entry of this.spriteEntries.values()) {
      disposeInstancedEntry(entry);
    }
    for (const material of this.meshMaterialCache.values()) {
      material.dispose();
    }
    for (const material of this.spriteMaterialCache.values()) {
      material.dispose();
    }

    this.clearOverlay();
    this.clearTelemetryTrail();
    this.renderer.dispose();
  }

  setSurfaceMetrics(metrics: RenderSurfaceMetrics): void {
    this.surfaceMetrics = metrics;
    this.resize();
  }

  private bootstrapScenes(): void {
    this.scene.fog = new THREE.Fog(0x09111b, 28, 180);
    this.scene.backgroundIntensity = 0.8;

    const defaultEnvironment: ThreeJsEnvironment = {
      biomeId: "verdant-hollow",
      skyColor: [0.64, 0.8, 0.98, 1],
      fogColor: [0.73, 0.84, 0.78, 1],
      fogNear: 30,
      fogFar: 196,
      ambientColor: [0.82, 0.92, 0.88],
      ambientIntensity: 1.4,
      sunColor: [1, 0.96, 0.84],
      sunIntensity: 2.9,
      sunDirection: [30, 48, 18],
      fillColor: [0.44, 0.74, 0.94],
      fillIntensity: 0.88,
      fillDirection: [-18, 14, -10],
      rimColor: [0.42, 0.88, 0.78],
      rimIntensity: 9,
      groundColor: [0.19, 0.33, 0.21, 1],
      starfieldIntensity: 0.08
    };

    const hemisphere = new THREE.HemisphereLight(0xa8d1ff, 0x14263f, 1.2);
    this.scene.add(hemisphere);
    this.hemisphereLight = hemisphere;

    const sun = new THREE.DirectionalLight(0xfff0cf, 2.6);
    sun.position.set(24, 42, 18);
    sun.castShadow = this.quality.enableShadows;
    sun.shadow.mapSize.set(this.quality.shadowMapSize, this.quality.shadowMapSize);
    sun.shadow.camera.near = 1;
    sun.shadow.camera.far = 160;
    sun.shadow.camera.left = -60;
    sun.shadow.camera.right = 60;
    sun.shadow.camera.top = 60;
    sun.shadow.camera.bottom = -60;
    this.scene.add(sun);
    this.sunLight = sun;

    const fill = new THREE.DirectionalLight(0x6cbcff, 0.7);
    fill.position.set(-18, 14, -10);
    this.scene.add(fill);
    this.fillLight = fill;

    const rim = new THREE.PointLight(0x4bc1ff, 12, 180, 2.2);
    rim.position.set(0, 26, 0);
    this.scene.add(rim);
    this.rimLight = rim;

    this.skyTextureSurface = createPaintSurface(512, 512);
    paintSkyTexture(this.skyTextureSurface, defaultEnvironment);
    const skyTexture = new THREE.CanvasTexture(this.skyTextureSurface);
    skyTexture.colorSpace = THREE.SRGBColorSpace;
    this.skyTexture = skyTexture;
    const skyMaterial = new THREE.MeshBasicMaterial({
      map: skyTexture,
      side: THREE.BackSide,
      depthWrite: false,
      fog: false
    });
    const skyDome = new THREE.Mesh(new THREE.SphereGeometry(280, 40, 24), skyMaterial);
    this.scene.add(skyDome);
    this.skyDome = skyDome;

    const sunOrb = new THREE.Mesh(
      new THREE.SphereGeometry(8, 24, 24),
      new THREE.MeshBasicMaterial({
        color: new THREE.Color(1, 0.95, 0.82),
        transparent: true,
        opacity: 0.94,
        fog: false
      })
    );
    this.scene.add(sunOrb);
    this.sunOrb = sunOrb;

    this.terrainTextureSurface = createPaintSurface(512, 512);
    paintTerrainTexture(this.terrainTextureSurface, defaultEnvironment);
    const terrainTexture = new THREE.CanvasTexture(this.terrainTextureSurface);
    terrainTexture.colorSpace = THREE.SRGBColorSpace;
    terrainTexture.wrapS = THREE.RepeatWrapping;
    terrainTexture.wrapT = THREE.RepeatWrapping;
    terrainTexture.repeat.set(1, 1);
    this.terrainTexture = terrainTexture;
    const groundMaterial = new THREE.MeshStandardMaterial({
      color: 0xffffff,
      map: terrainTexture,
      roughness: 0.9,
      metalness: 0.03
    });
    this.groundMaterial = groundMaterial;
    const ground = new THREE.Mesh(createTerrainGeometry(), groundMaterial);
    ground.rotation.x = -Math.PI / 2;
    ground.receiveShadow = true;
    ground.castShadow = false;
    this.scene.add(ground);
    this.terrainMesh = ground;

    this.waterTextureSurface = createPaintSurface(384, 384);
    paintWaterTexture(this.waterTextureSurface);
    const waterTexture = new THREE.CanvasTexture(this.waterTextureSurface);
    waterTexture.colorSpace = THREE.SRGBColorSpace;
    waterTexture.wrapS = THREE.RepeatWrapping;
    waterTexture.wrapT = THREE.RepeatWrapping;
    waterTexture.repeat.set(1.4, 1.4);
    this.waterTexture = waterTexture;
    const waterMaterial = new THREE.MeshStandardMaterial({
      color: new THREE.Color(0.24, 0.62, 0.76),
      map: waterTexture,
      transparent: true,
      opacity: 0.9,
      roughness: 0.08,
      metalness: 0.12,
      emissive: new THREE.Color(0.04, 0.12, 0.18),
      emissiveIntensity: 0.56,
      side: THREE.DoubleSide,
      depthWrite: false
    });
    const water = new THREE.Mesh(createLakeGeometry(), waterMaterial);
    water.position.y = WATER_LEVEL + 0.06;
    water.receiveShadow = true;
    water.renderOrder = 2;
    this.scene.add(water);
    this.waterMesh = water;

    const shorelineMaterial = new THREE.MeshStandardMaterial({
      color: new THREE.Color(0.82, 0.79, 0.66),
      transparent: true,
      opacity: 0.54,
      roughness: 0.92,
      metalness: 0,
      emissive: new THREE.Color(0.12, 0.14, 0.12),
      emissiveIntensity: 0.12,
      side: THREE.DoubleSide,
      depthWrite: false
    });
    const shoreline = new THREE.Mesh(createShorelineGeometry(), shorelineMaterial);
    shoreline.position.y = WATER_LEVEL + 0.08;
    shoreline.receiveShadow = true;
    shoreline.renderOrder = 3;
    this.scene.add(shoreline);
    this.shorelineMesh = shoreline;

    const mountainBackdrop = new THREE.Mesh(
      createMountainBackdropGeometry(),
      new THREE.MeshStandardMaterial({
        color: new THREE.Color(0.17, 0.24, 0.32),
        roughness: 1,
        metalness: 0,
        fog: true
      })
    );
    mountainBackdrop.position.y = -4;
    mountainBackdrop.receiveShadow = false;
    mountainBackdrop.castShadow = false;
    this.scene.add(mountainBackdrop);
    this.mountainBackdrop = mountainBackdrop;

    if (this.quality.showGrid) {
      const grid = new THREE.GridHelper(180, 60, 0x5fa7ff, 0x173049);
      grid.position.y = 0.02;
      this.scene.add(grid);
    }

    const starfieldMaterial = new THREE.PointsMaterial({
      color: 0xcde7ff,
      size: 0.9,
      sizeAttenuation: true,
      transparent: true,
      opacity: 0.9,
      depthWrite: false
    });
    this.starfieldMaterial = starfieldMaterial;
    const skyline = new THREE.Points(createStarfieldGeometry(640, 220), starfieldMaterial);
    skyline.position.y = 36;
    this.scene.add(skyline);
    this.applyEnvironment(defaultEnvironment);

    this.overlayScene.background = null;
    this.overlayCamera.position.set(0, 0, 5);
    this.overlayCamera.lookAt(0, 0, 0);
  }

  private applyEnvironment(environment: ThreeJsEnvironment): void {
    this.baseEnvironment = environment;
    const signature = environmentSignature(environment);
    if (signature !== this.lastEnvironmentSignature) {
      if (this.skyTextureSurface && this.skyTexture) {
        paintSkyTexture(this.skyTextureSurface, environment);
        this.skyTexture.needsUpdate = true;
      }
      if (this.terrainTextureSurface && this.terrainTexture) {
        paintTerrainTexture(this.terrainTextureSurface, environment);
        this.terrainTexture.needsUpdate = true;
      }
      this.lastEnvironmentSignature = signature;
    }

    this.applyDynamicEnvironment(
      sampleTimeLapseEnvironment(environment, performance.now() / 1000).environment
    );
  }

  private applyDynamicEnvironment(environment: ThreeJsEnvironment): void {
    this.environmentPreset = describeEnvironmentPreset(environment);

    this.scene.background = new THREE.Color(...environment.skyColor.slice(0, 3));
    const fog = this.scene.fog;
    if (fog instanceof THREE.Fog) {
      fog.color.setRGB(
        environment.fogColor[0],
        environment.fogColor[1],
        environment.fogColor[2]
      );
      fog.near = environment.fogNear;
      fog.far = environment.fogFar;
    }

    this.hemisphereLight?.color.setRGB(
      environment.ambientColor[0],
      environment.ambientColor[1],
      environment.ambientColor[2]
    );
    this.hemisphereLight?.groundColor.setRGB(
      environment.groundColor[0] * 0.75,
      environment.groundColor[1] * 0.75,
      environment.groundColor[2] * 0.75
    );
    if (this.hemisphereLight) {
      this.hemisphereLight.intensity = environment.ambientIntensity;
    }

    this.sunLight?.color.setRGB(
      environment.sunColor[0],
      environment.sunColor[1],
      environment.sunColor[2]
    );
    this.sunLight?.position.set(...environment.sunDirection);
    if (this.sunLight) {
      this.sunLight.intensity = environment.sunIntensity;
    }

    this.fillLight?.color.setRGB(
      environment.fillColor[0],
      environment.fillColor[1],
      environment.fillColor[2]
    );
    this.fillLight?.position.set(...environment.fillDirection);
    if (this.fillLight) {
      this.fillLight.intensity = environment.fillIntensity;
    }

    this.rimLight?.color.setRGB(
      environment.rimColor[0],
      environment.rimColor[1],
      environment.rimColor[2]
    );
    if (this.rimLight) {
      this.rimLight.intensity = environment.rimIntensity;
    }

    this.groundMaterial?.color.setRGB(
      environment.groundColor[0],
      environment.groundColor[1],
      environment.groundColor[2]
    );
    if (this.waterMesh) {
      this.waterMesh.material.color.setRGB(0.18, 0.48 + environment.skyColor[1] * 0.18, 0.62 + environment.skyColor[2] * 0.1);
      this.waterMesh.material.emissive.setRGB(
        0.04 + environment.skyColor[0] * 0.05,
        0.08 + environment.skyColor[1] * 0.08,
        0.12 + environment.skyColor[2] * 0.08
      );
      this.waterMesh.material.emissiveIntensity = 0.4 + environment.sunIntensity * 0.03;
    }
    if (this.shorelineMesh) {
      this.shorelineMesh.material.color.setRGB(
        0.68 + environment.sunColor[0] * 0.1,
        0.62 + environment.sunColor[1] * 0.1,
        0.54 + environment.sunColor[2] * 0.04
      );
      this.shorelineMesh.material.opacity = 0.3 + environment.sunIntensity * 0.05;
      this.shorelineMesh.material.emissive.setRGB(
        0.08 + environment.skyColor[0] * 0.04,
        0.08 + environment.skyColor[1] * 0.04,
        0.07 + environment.skyColor[2] * 0.03
      );
    }
    if (this.mountainBackdrop) {
      this.mountainBackdrop.material.color.setRGB(
        environment.fogColor[0] * 0.5,
        environment.fogColor[1] * 0.48,
        environment.fogColor[2] * 0.58
      );
      this.mountainBackdrop.material.emissive.setRGB(
        environment.fillColor[0] * 0.05,
        environment.fillColor[1] * 0.05,
        environment.fillColor[2] * 0.06
      );
    }
    if (this.sunOrb) {
      const sunDirection = new THREE.Vector3(...environment.sunDirection)
        .normalize()
        .multiplyScalar(210);
      this.sunOrb.position.copy(sunDirection);
      this.sunOrb.material.color.setRGB(
        environment.sunColor[0],
        environment.sunColor[1],
        environment.sunColor[2]
      );
      this.sunOrb.material.opacity = clamp(0.18 + environment.sunIntensity * 0.22, 0.18, 0.96);
    }
    if (this.starfieldMaterial) {
      this.starfieldMaterial.opacity = clamp(environment.starfieldIntensity, 0, 1);
    }
  }

  private applyCamera(
    pose: ReturnType<typeof buildCameraPose>,
    frameCamera: ThreeJsWebGpuFrame["camera"]
  ): void {
    const targetPosition = new THREE.Vector3(...pose.position);
    const targetLookAt = new THREE.Vector3(...pose.target);
    const now =
      typeof performance !== "undefined" && typeof performance.now === "function"
        ? performance.now()
        : Date.now();
    const deltaSeconds =
      this.lastCameraUpdateAt === 0
        ? 1 / 60
        : clamp((now - this.lastCameraUpdateAt) / 1000, 1 / 240, 0.12);
    this.lastCameraUpdateAt = now;
    const positionAlpha = 1 - Math.exp(-deltaSeconds * 10);
    const targetAlpha = 1 - Math.exp(-deltaSeconds * 14);

    if (!this.cameraPoseInitialized) {
      this.camera.position.copy(targetPosition);
      this.smoothedCameraTarget.copy(targetLookAt);
      this.cameraPoseInitialized = true;
    } else {
      this.camera.position.lerp(targetPosition, positionAlpha);
      this.smoothedCameraTarget.lerp(targetLookAt, targetAlpha);
    }

    this.camera.fov = pose.fov;
    this.camera.near = pose.near;
    this.camera.far = pose.far;
    this.camera.lookAt(this.smoothedCameraTarget);
    this.camera.updateProjectionMatrix();

    const halfWidth = frameCamera.viewportWidth / 2;
    const halfHeight = frameCamera.viewportHeight / 2;
    this.overlayCamera.left = -halfWidth;
    this.overlayCamera.right = halfWidth;
    this.overlayCamera.top = halfHeight;
    this.overlayCamera.bottom = -halfHeight;
    this.overlayCamera.position.set(frameCamera.x, frameCamera.y, 5);
    this.overlayCamera.zoom = Math.max(frameCamera.zoom, 0.1);
    this.overlayCamera.rotation.z = frameCamera.rotation;
    this.overlayCamera.updateProjectionMatrix();
  }

  private async syncMeshBatches(batches: PlannedMeshBatch[]): Promise<void> {
    const activeKeys = new Set<string>();
    const now =
      typeof performance !== "undefined" && typeof performance.now === "function"
        ? performance.now()
        : Date.now();
    const elapsedSeconds = now / 1000;
    pruneExpiredPulses(this.entityPulseUntilMs, now);

    for (const planned of batches) {
      if (planned.visibleCount === 0) {
        continue;
      }

      activeKeys.add(planned.key);
      const geometry = await this.assetRegistry.resolveGeometry(
        planned.batch,
        planned.lodLevel
      );
      const material =
        (await this.assetRegistry.resolveMeshMaterial?.(
          planned.batch,
          planned.lodLevel,
          this.quality
        )) ??
        this.getOrCreateMeshMaterial(planned);
      const existing = this.meshEntries.get(planned.key);
      const entry = ensureInstancedEntry(
        this.scene,
        existing,
        geometry,
        material,
        planned.instances.length,
        planned.key
      );

      entry.mesh.castShadow = planned.batch.castShadows;
      entry.mesh.receiveShadow = planned.batch.receiveShadows;
      entry.mesh.count = planned.instances.length;
      entry.mesh.renderOrder = planned.batch.renderOrder;
      entry.mesh.visible = true;
      entry.mesh.name = planned.key;
      entry.mesh.frustumCulled = false;

      for (let index = 0; index < planned.instances.length; index += 1) {
        const instance = planned.instances[index];
        if (!instance) {
          continue;
        }
        entry.mesh.setMatrixAt(
          index,
          composeAnimatedInstanceMatrix(
            instance,
            elapsedSeconds,
            pulseStrengthForEntity(this.entityPulseUntilMs, instance.sourceEntity, now)
          )
        );
      }
      entry.mesh.instanceMatrix.needsUpdate = true;
      this.meshEntries.set(planned.key, entry);
    }

    cleanupUnusedEntries(this.scene, this.meshEntries, activeKeys);
  }

  private async syncSpriteBatches(batches: PlannedSpriteBatch[]): Promise<void> {
    const activeKeys = new Set<string>();
    const now =
      typeof performance !== "undefined" && typeof performance.now === "function"
        ? performance.now()
        : Date.now();
    const elapsedSeconds = now / 1000;

    for (const planned of batches) {
      if (planned.visibleCount === 0) {
        continue;
      }

      activeKeys.add(planned.key);
      const resolved = await this.assetRegistry.resolveSpriteTexture({
        texture: planned.batch.texture,
        frame: planned.batch.frame
      }, this.quality.anisotropy);
      const material = this.getOrCreateSpriteMaterial(planned, resolved);

      const existing = this.spriteEntries.get(planned.key);
      const entry = ensureInstancedEntry(
        this.scene,
        existing,
        SPRITE_PLANE_GEOMETRY,
        material,
        planned.instances.length,
        planned.key
      );

      entry.mesh.castShadow = false;
      entry.mesh.receiveShadow = false;
      entry.mesh.count = planned.instances.length;
      entry.mesh.renderOrder = planned.batch.renderOrder;
      entry.mesh.visible = true;
      entry.mesh.frustumCulled = false;

      for (let index = 0; index < planned.instances.length; index += 1) {
        const instance = planned.instances[index];
        if (!instance) {
          continue;
        }
        entry.mesh.setMatrixAt(
          index,
          composeAnimatedInstanceMatrix(
            instance,
            elapsedSeconds,
            pulseStrengthForEntity(this.entityPulseUntilMs, instance.sourceEntity, now),
            planned.batch.billboard ? this.camera.quaternion : undefined
          )
        );
      }
      entry.mesh.instanceMatrix.needsUpdate = true;
      this.spriteEntries.set(planned.key, entry);
    }

    cleanupUnusedEntries(this.scene, this.spriteEntries, activeKeys);
  }

  private async syncAmbientMeshBatches(batches: PlannedMeshBatch[]): Promise<void> {
    const activeKeys = new Set<string>();

    for (const planned of batches) {
      if (planned.visibleCount === 0) {
        continue;
      }

      activeKeys.add(planned.key);
      const geometry = await this.assetRegistry.resolveGeometry(planned.batch, planned.lodLevel);
      const existing = this.ambientMeshEntries.get(planned.key);
      const material =
        existing?.material ??
        createMeshMaterial(planned.batch, planned.lodLevel, this.quality);
      const entry = ensureInstancedEntry(
        this.scene,
        existing,
        geometry,
        material,
        planned.instances.length,
        planned.key
      );

      entry.mesh.castShadow = planned.batch.castShadows;
      entry.mesh.receiveShadow = planned.batch.receiveShadows;
      entry.mesh.count = planned.instances.length;
      entry.mesh.renderOrder = planned.batch.renderOrder;
      entry.mesh.visible = true;
      entry.mesh.name = planned.key;
      entry.mesh.frustumCulled = false;

      for (let index = 0; index < planned.instances.length; index += 1) {
        const instance = planned.instances[index];
        if (!instance) {
          continue;
        }
        entry.mesh.setMatrixAt(index, composeAnimatedInstanceMatrix(instance, 0, 0));
      }
      entry.mesh.instanceMatrix.needsUpdate = true;
      this.ambientMeshEntries.set(planned.key, entry);
    }

    cleanupUnusedEntries(this.scene, this.ambientMeshEntries, activeKeys);
  }

  private async syncOverlay(commands: RenderCommand[]): Promise<void> {
    this.clearOverlay();

    for (const command of commands) {
      if (!command.visible) {
        continue;
      }

      if (command.type === "rect") {
        const rect = new THREE.Mesh(
          OVERLAY_PLANE_GEOMETRY,
          new THREE.MeshBasicMaterial({
            color: new THREE.Color(
              command.color[0],
              command.color[1],
              command.color[2]
            ),
            transparent: command.alpha < 0.999,
            opacity: command.alpha,
            depthWrite: false
          })
        );
        rect.position.set(command.x, command.y, command.layer * 0.01);
        rect.rotation.z = command.rotation;
        rect.scale.set(command.width * command.scaleX, command.height * command.scaleY, 1);
        this.overlayScene.add(rect);
        this.overlayObjects.push(rect);
        continue;
      }

      if (command.type === "sprite" && command.texture) {
        const resolved = await this.assetRegistry.resolveSpriteTexture({
          texture: command.texture,
          frame: command.frame ?? 0
        });
        const sprite = new THREE.Mesh(
          OVERLAY_PLANE_GEOMETRY,
          createSpriteMaterial(
            resolved,
            command.color,
            command.alpha < 0.999,
            false,
            true,
            THREE.DoubleSide
          )
        );
        sprite.position.set(command.x, command.y, command.layer * 0.01);
        sprite.rotation.z = command.rotation;
        sprite.scale.set(
          Math.max(command.width * command.scaleX, 0.0001),
          Math.max(command.height * command.scaleY, 0.0001),
          1
        );
        this.overlayScene.add(sprite);
        this.overlayObjects.push(sprite);
      }
    }
  }

  private clearOverlay(): void {
    for (const object of this.overlayObjects) {
      this.overlayScene.remove(object);
      disposeObject(object);
    }
    this.overlayObjects.length = 0;
  }

  setTelemetryTrail(samples: TelemetryTrajectorySample[]): void {
    this.clearTelemetryTrail();
    if (samples.length < 2) {
      return;
    }

    const points = samples.map(
      (sample, index) =>
        new THREE.Vector3(
          sample.position[0],
          sampleTerrainHeight(sample.position[0], sample.position[1]) + 0.22 + index * 0.0035,
          sample.position[1]
        )
    );
    const geometry = new THREE.BufferGeometry().setFromPoints(points);
    const material = new THREE.LineBasicMaterial({
      color: new THREE.Color(0.54, 0.96, 0.81),
      transparent: true,
      opacity: 0.9,
      depthWrite: false
    });
    const line = new THREE.Line(geometry, material);
    line.renderOrder = 10_000;
    line.frustumCulled = false;
    this.scene.add(line);
    this.telemetryTrail = line;
  }

  clearTelemetryTrail(): void {
    if (!this.telemetryTrail) {
      return;
    }
    this.scene.remove(this.telemetryTrail);
    this.telemetryTrail.geometry.dispose();
    this.telemetryTrail.material.dispose();
    this.telemetryTrail = null;
  }

  private async renderFrame(): Promise<void> {
    const frameStart = performance.now();
    if (this.baseEnvironment) {
      const timeLapse = sampleTimeLapseEnvironment(
        this.baseEnvironment,
        frameStart / 1000 + DAYLIGHT_START_OFFSET_SECONDS
      );
      this.timeOfDayHours = timeLapse.timeOfDayHours;
      this.applyDynamicEnvironment(timeLapse.environment);
    }
    if (this.waterTexture) {
      this.waterTexture.offset.set(
        (frameStart * 0.000045) % 1,
        (frameStart * 0.00003) % 1
      );
    }
    const previousAutoClear = this.renderer.autoClear;
    this.renderer.autoClear = true;
    await renderWithFallback(this.renderer, this.scene, this.camera);
    this.renderer.autoClear = false;
    this.renderer.clearDepth();
    await renderWithFallback(this.renderer, this.overlayScene, this.overlayCamera);
    this.renderer.autoClear = previousAutoClear;
    this.updateAdaptiveResolution(performance.now() - frameStart);
  }

  private resize(): void {
    const { width, height } = readRenderSurfaceSize(this.canvas, this.surfaceMetrics);
    const pixelRatio = Math.min(
      this.surfaceMetrics?.devicePixelRatio ?? readDevicePixelRatio(),
      this.adaptivePixelRatio
    );
    this.renderer.setPixelRatio(pixelRatio);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / Math.max(height, 1);
    this.camera.updateProjectionMatrix();

    const rendererWithCapabilities = this.renderer as RuntimeRenderer & {
      capabilities?: { getMaxAnisotropy?: () => number };
    };
    const anisotropy =
      typeof rendererWithCapabilities.capabilities?.getMaxAnisotropy === "function"
        ? Math.min(rendererWithCapabilities.capabilities.getMaxAnisotropy(), 8)
        : 1;
    if (this.terrainTexture) {
      this.terrainTexture.anisotropy = anisotropy;
    }
    if (this.skyTexture) {
      this.skyTexture.anisotropy = anisotropy;
    }
    if (this.waterTexture) {
      this.waterTexture.anisotropy = anisotropy;
    }
  }

  getStats(): PodThreeRendererStats {
    const residency = this.assetRegistry.getResidencyStats?.() ?? {
      residentGeometryAssets: 0,
      residentSpriteAssets: 0,
      pendingGeometryAssets: 0,
      pendingSpriteAssets: 0
    };
    const landscapeMode =
      this.terrainMesh && this.skyDome
        ? LANDSCAPE_PROFILE_ID
        : LANDSCAPE_PROFILE_ID;

    return {
      backend: this.backend,
      renderThread: "main",
      qualityPreset: this.quality.preset,
      environmentPreset: this.environmentPreset,
      landscapeMode,
      waterMode: WATER_PROFILE_ID,
      timeOfDayHours: Number(this.timeOfDayHours.toFixed(2)),
      pixelRatio: this.renderer.getPixelRatio(),
      drawCalls: this.renderer.info.render.calls,
      triangles: this.renderer.info.render.triangles,
      textures: this.renderer.info.memory.textures,
      frameMs: Number(this.smoothedFrameMs.toFixed(2)),
      residentGeometryAssets: residency.residentGeometryAssets,
      residentSpriteAssets: residency.residentSpriteAssets,
      pendingGeometryAssets: residency.pendingGeometryAssets,
      pendingSpriteAssets: residency.pendingSpriteAssets,
      ambientInstances: this.ambientInstances,
      visibleWorldChunks: this.visibleWorldChunks,
      preloadedWorldChunks: this.preloadedWorldChunks
    };
  }

  private async prewarmPlannedAssets(
    planned: ReturnType<typeof buildFramePlan>,
    ambientPlan: AmbientChunkDressingPlan
  ): Promise<void> {
    await Promise.all([
      this.assetRegistry.prefetchMeshes?.(
        dedupeMeshPrefetchRequests([
          ...planned.prewarmMeshRequests.map((request) => ({
            batch: request.batch,
            lodLevel: request.lodLevel
          })),
          ...ambientPlan.prewarmRequests
        ])
      ),
      this.assetRegistry.prefetchSprites?.(
        planned.prewarmSpriteRequests.map((request) => ({
            batch: {
              texture: request.batch.texture,
              frame: request.batch.frame
            },
            anisotropy: this.quality.anisotropy
          }))
      )
    ]);
  }

  private getOrCreateMeshMaterial(planned: PlannedMeshBatch): THREE.Material {
    const key = [
      planned.batch.mesh,
      planned.batch.material,
      planned.lodLevel,
      planned.batch.transparent,
      planned.batch.doubleSided,
      planned.batch.tint.join(","),
      planned.batch.emissive.join(","),
      planned.batch.roughness,
      planned.batch.metallic,
      planned.batch.depthWrite,
      planned.batch.depthTest,
      this.quality.environmentIntensity
    ].join("|");
    const cached = this.meshMaterialCache.get(key);
    if (cached) {
      return cached;
    }

    const material = createMeshMaterial(planned.batch, planned.lodLevel, this.quality);
    this.meshMaterialCache.set(key, material);
    return material;
  }

  private getOrCreateSpriteMaterial(
    planned: PlannedSpriteBatch,
    resolved: Parameters<typeof createSpriteMaterial>[0]
  ): THREE.Material {
    const key = [
      planned.batch.texture,
      planned.batch.frame,
      planned.batch.transparent,
      planned.batch.depthWrite,
      planned.batch.depthTest,
      planned.tint.join(",")
    ].join("|");
    const cached = this.spriteMaterialCache.get(key);
    if (cached) {
      return cached;
    }

    const material = createSpriteMaterial(
      resolved,
      planned.tint,
      planned.batch.transparent,
      planned.batch.depthWrite,
      planned.batch.depthTest,
      THREE.DoubleSide
    );
    this.spriteMaterialCache.set(key, material);
    return material;
  }

  private updateAdaptiveResolution(frameMs: number): void {
    if (!this.quality.enableAdaptiveResolution) {
      this.smoothedFrameMs = frameMs;
      return;
    }

    this.smoothedFrameMs = this.smoothedFrameMs * 0.9 + frameMs * 0.1;

    if (this.adjustmentCooldown > 0) {
      this.adjustmentCooldown -= 1;
      return;
    }

    const upperBound = this.quality.targetFrameMs * 1.08;
    const lowerBound = this.quality.targetFrameMs * 0.8;

    if (
      this.smoothedFrameMs > upperBound &&
      this.adaptivePixelRatio > this.quality.minPixelRatio
    ) {
      this.adaptivePixelRatio = Math.max(
        this.quality.minPixelRatio,
        this.adaptivePixelRatio - this.quality.adaptiveResolutionStep
      );
      this.adjustmentCooldown = 18;
      this.resize();
      return;
    }

    if (
      this.smoothedFrameMs < lowerBound &&
      this.adaptivePixelRatio < this.quality.maxPixelRatio
    ) {
      this.adaptivePixelRatio = Math.min(
        this.quality.maxPixelRatio,
        this.adaptivePixelRatio + this.quality.adaptiveResolutionStep
      );
      this.adjustmentCooldown = 24;
      this.resize();
    }
  }
}

interface AmbientChunkDressingInput {
  visibleChunkKeys: string[];
  preloadedChunkKeys: string[];
  cameraPosition: [number, number, number];
  qualityPreset: PodThreeQualityPreset;
  worldChunkSize?: number;
  highDetailDistance: number;
  mediumDetailDistance: number;
}

interface AmbientChunkArchetype {
  id: string;
  mesh: string;
  material: string;
  tint: [number, number, number, number];
  emissive: [number, number, number];
  roughness: number;
  metallic: number;
  castShadows: boolean;
  receiveShadows: boolean;
  renderOrder: number;
  baseScale: [number, number, number];
  halfHeight: number;
  densities: Record<PodThreeQualityPreset, number>;
  lakeMaskMax: number;
  minSlope: number;
  maxSlope: number;
  minHeight: number;
  maxHeight: number;
  minRadiusFromOrigin: number;
  regionBias?: (chunkX: number, chunkZ: number) => boolean;
}

const AMBIENT_CHUNK_ARCHETYPES: AmbientChunkArchetype[] = [
  {
    id: "canopy-tree",
    mesh: "canopy-tree",
    material: "foliage-canopy",
    tint: [0.82, 0.92, 0.8, 1],
    emissive: [0.03, 0.06, 0.03],
    roughness: 0.96,
    metallic: 0,
    castShadows: true,
    receiveShadows: true,
    renderOrder: 4,
    baseScale: [1.9, 2.5, 1.9],
    halfHeight: 1.7,
    densities: {
      ultra: 4,
      high: 3,
      balanced: 2,
      performance: 1
    },
    lakeMaskMax: 0.14,
    minSlope: 0,
    maxSlope: 0.58,
    minHeight: -20,
    maxHeight: 19,
    minRadiusFromOrigin: 18
  },
  {
    id: "weathered-boulder",
    mesh: "weathered-boulder",
    material: "weathered-stone",
    tint: [0.88, 0.9, 0.94, 1],
    emissive: [0.01, 0.015, 0.02],
    roughness: 1,
    metallic: 0.02,
    castShadows: true,
    receiveShadows: true,
    renderOrder: 5,
    baseScale: [1.2, 1, 1.15],
    halfHeight: 1,
    densities: {
      ultra: 3,
      high: 2,
      balanced: 2,
      performance: 1
    },
    lakeMaskMax: 0.2,
    minSlope: 0.04,
    maxSlope: 1.12,
    minHeight: -20,
    maxHeight: 28,
    minRadiusFromOrigin: 12
  },
  {
    id: "basalt-column",
    mesh: "basalt-column",
    material: "basalt",
    tint: [0.72, 0.76, 0.84, 1],
    emissive: [0.015, 0.02, 0.03],
    roughness: 0.9,
    metallic: 0.04,
    castShadows: true,
    receiveShadows: true,
    renderOrder: 5,
    baseScale: [0.82, 1.25, 0.82],
    halfHeight: 2.6,
    densities: {
      ultra: 2,
      high: 2,
      balanced: 1,
      performance: 1
    },
    lakeMaskMax: 0.1,
    minSlope: 0.12,
    maxSlope: 1.55,
    minHeight: -18,
    maxHeight: 38,
    minRadiusFromOrigin: 22,
    regionBias: (chunkX, chunkZ) => chunkX >= 0 || chunkZ <= -1
  },
  {
    id: "glass-spire",
    mesh: "glass-spire",
    material: "aether-glass",
    tint: [0.72, 0.9, 1, 1],
    emissive: [0.12, 0.18, 0.24],
    roughness: 0.24,
    metallic: 0.22,
    castShadows: false,
    receiveShadows: true,
    renderOrder: 6,
    baseScale: [1.05, 1.55, 1.05],
    halfHeight: 2.4,
    densities: {
      ultra: 1,
      high: 1,
      balanced: 1,
      performance: 0
    },
    lakeMaskMax: 0.06,
    minSlope: 0,
    maxSlope: 0.72,
    minHeight: -18,
    maxHeight: 42,
    minRadiusFromOrigin: 34,
    regionBias: (_chunkX, chunkZ) => chunkZ >= 1
  }
];

export function buildAmbientChunkDressingPlan(
  input: AmbientChunkDressingInput
): AmbientChunkDressingPlan {
  const worldChunkSize = input.worldChunkSize ?? DEFAULT_WORLD_CHUNK_SIZE;
  const camera = new THREE.Vector3(...input.cameraPosition);
  const visibleGroups = new Map<string, { batch: PlannedMeshBatch["batch"]; lodLevel: 0 | 1 | 2; instances: PlannedMeshBatch["instances"] }>();
  const prewarmRequests = new Map<string, { batch: PlannedMeshBatch["batch"]; lodLevel: 0 | 1 | 2 }>();

  for (const chunkKey of input.visibleChunkKeys) {
    const instances = sampleAmbientChunkInstances(
      chunkKey,
      worldChunkSize,
      input.qualityPreset
    );

    for (const entry of instances) {
      const distance = Math.hypot(
        entry.instance.position[0] - camera.x,
        entry.instance.position[2] - camera.z
      );
      const lodLevel =
        distance <= input.highDetailDistance
          ? 0
          : distance <= input.mediumDetailDistance
            ? 1
            : 2;
      const key = `ambient:${entry.archetype.id}:lod:${lodLevel}`;
      const group = visibleGroups.get(key) ?? {
        batch: createAmbientBatch(entry.archetype),
        lodLevel,
        instances: []
      };
      group.instances.push(entry.instance);
      visibleGroups.set(key, group);
    }
  }

  for (const chunkKey of input.preloadedChunkKeys) {
    const instances = sampleAmbientChunkInstances(
      chunkKey,
      worldChunkSize,
      input.qualityPreset
    );
    for (const entry of instances) {
      const distance = Math.hypot(
        entry.instance.position[0] - camera.x,
        entry.instance.position[2] - camera.z
      );
      const lodLevel =
        distance <= input.highDetailDistance
          ? 0
          : distance <= input.mediumDetailDistance
            ? 1
            : 2;
      const key = `${entry.archetype.mesh}:lod:${lodLevel}`;
      if (!prewarmRequests.has(key)) {
        prewarmRequests.set(key, {
          batch: createAmbientBatch(entry.archetype),
          lodLevel
        });
      }
    }
  }

  const meshBatches = Array.from(visibleGroups.entries())
    .map(([key, group]) => ({
      key,
      batch: group.batch,
      lodLevel: group.lodLevel,
      visibleCount: group.instances.length,
      instances: group.instances,
      matrices: group.instances.map((instance) => composeAnimatedInstanceMatrix(instance, 0, 0))
    }))
    .sort((left, right) => {
      if (left.batch.renderOrder !== right.batch.renderOrder) {
        return left.batch.renderOrder - right.batch.renderOrder;
      }
      return left.key.localeCompare(right.key);
    });

  return {
    meshBatches,
    prewarmRequests: Array.from(prewarmRequests.values()),
    totalInstances: meshBatches.reduce((total, batch) => total + batch.visibleCount, 0)
  };
}

function createAmbientBatch(archetype: AmbientChunkArchetype): PlannedMeshBatch["batch"] {
  return {
    mesh: archetype.mesh,
    material: archetype.material,
    layer: 0,
    phase: "opaque",
    sortDepth: 0,
    renderOrder: archetype.renderOrder,
    transparent: false,
    doubleSided: false,
    castShadows: archetype.castShadows,
    receiveShadows: archetype.receiveShadows,
    tint: archetype.tint,
    roughness: archetype.roughness,
    metallic: archetype.metallic,
    emissive: archetype.emissive,
    depthWrite: true,
    depthTest: true,
    instances: []
  };
}

function sampleAmbientChunkInstances(
  chunkKey: string,
  worldChunkSize: number,
  qualityPreset: PodThreeQualityPreset
): Array<{ archetype: AmbientChunkArchetype; instance: PlannedMeshBatch["instances"][number] }> {
  const [chunkX, chunkZ] = parseAmbientChunkKey(chunkKey);
  const originX = chunkX * worldChunkSize;
  const originZ = chunkZ * worldChunkSize;
  const instances = new Array<{
    archetype: AmbientChunkArchetype;
    instance: PlannedMeshBatch["instances"][number];
  }>();

  for (const archetype of AMBIENT_CHUNK_ARCHETYPES) {
    const density = archetype.densities[qualityPreset];
    if (density <= 0) {
      continue;
    }
    if (archetype.regionBias && !archetype.regionBias(chunkX, chunkZ)) {
      continue;
    }

    for (let slotIndex = 0; slotIndex < density; slotIndex += 1) {
      const x =
        originX +
        2.4 +
        ambientNoise(chunkX, chunkZ, slotIndex, `${archetype.id}:x`) *
          Math.max(worldChunkSize - 4.8, 1);
      const z =
        originZ +
        2.4 +
        ambientNoise(chunkX, chunkZ, slotIndex, `${archetype.id}:z`) *
          Math.max(worldChunkSize - 4.8, 1);
      const radialDistance = Math.hypot(x, z);
      const lakeMask = sampleLakeMask(x, z);
      const slope = sampleTerrainSlope(x, z);
      const height = sampleTerrainHeight(x, z);

      if (radialDistance < archetype.minRadiusFromOrigin) {
        continue;
      }
      if (lakeMask > archetype.lakeMaskMax) {
        continue;
      }
      if (slope < archetype.minSlope || slope > archetype.maxSlope) {
        continue;
      }
      if (height < archetype.minHeight || height > archetype.maxHeight) {
        continue;
      }

      const scaleVariance = 0.82 + ambientNoise(chunkX, chunkZ, slotIndex, `${archetype.id}:s`) * 0.58;
      const yaw = ambientNoise(chunkX, chunkZ, slotIndex, `${archetype.id}:r`) * Math.PI * 2;
      const rotation = new THREE.Quaternion().setFromAxisAngle(
        new THREE.Vector3(0, 1, 0),
        yaw
      );
      const scale: [number, number, number] = [
        archetype.baseScale[0] * scaleVariance,
        archetype.baseScale[1] * scaleVariance,
        archetype.baseScale[2] * scaleVariance
      ];

      instances.push({
        archetype,
        instance: {
          position: [x, height + archetype.halfHeight * scale[1], z],
          rotation: [rotation.x, rotation.y, rotation.z, rotation.w],
          scale,
          animationSetId: "static-prop"
        }
      });
    }
  }

  return instances;
}

function parseAmbientChunkKey(chunkKey: string): [number, number] {
  const [rawX = "0", rawZ = "0"] = chunkKey.split(":");
  return [Number.parseInt(rawX, 10) || 0, Number.parseInt(rawZ, 10) || 0];
}

function ambientNoise(chunkX: number, chunkZ: number, slotIndex: number, salt: string): number {
  const saltValue = salt.split("").reduce((total, char) => total + char.charCodeAt(0), 0);
  const seed =
    chunkX * 127.1 + chunkZ * 311.7 + slotIndex * 74.7 + saltValue * 0.61803398875;
  const value = Math.sin(seed) * 43758.5453123;
  return value - Math.floor(value);
}

function dedupeMeshPrefetchRequests(
  requests: Array<{ batch: PlannedMeshBatch["batch"]; lodLevel: 0 | 1 | 2 }>
): Array<{ batch: PlannedMeshBatch["batch"]; lodLevel: 0 | 1 | 2 }> {
  const unique = new Map<string, { batch: PlannedMeshBatch["batch"]; lodLevel: 0 | 1 | 2 }>();

  for (const request of requests) {
    const key = `${request.batch.mesh}|${request.batch.material}|${request.lodLevel}`;
    if (!unique.has(key)) {
      unique.set(key, request);
    }
  }

  return Array.from(unique.values());
}

async function createRenderer(
  canvas: HTMLCanvasElement | OffscreenCanvas,
  options: PodThreeWorldRendererOptions
): Promise<{ renderer: RuntimeRenderer; backend: "webgpu" | "webgl2" }> {
  installThreeConsoleFilter();

  if (options.backendPreference !== "webgl2" && "gpu" in navigator) {
    try {
      const webgpuModule = await import("three/webgpu");
      const renderer = new webgpuModule.WebGPURenderer({
        canvas,
        antialias: true,
        alpha: false,
        powerPreference: "high-performance"
      }) as RuntimeRenderer;
      await renderer.init?.();
      renderer.shadowMap.enabled = options.enableShadows ?? true;
      renderer.shadowMap.type = THREE.VSMShadowMap;
      renderer.toneMapping = THREE.ACESFilmicToneMapping;
      renderer.toneMappingExposure = options.qualityProfile?.toneMappingExposure ?? 1.05;
      renderer.outputColorSpace = THREE.SRGBColorSpace;
      return { renderer, backend: "webgpu" };
    } catch (error) {
      console.warn("Falling back to WebGL2 renderer", error);
    }
  }

  const renderer = new THREE.WebGLRenderer({
    canvas,
    antialias: true,
    alpha: false,
    powerPreference: "high-performance"
  }) as RuntimeRenderer;
  renderer.shadowMap.enabled = options.enableShadows ?? true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = options.qualityProfile?.toneMappingExposure ?? 1.0;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  return { renderer, backend: "webgl2" };
}

async function renderWithFallback(
  renderer: RuntimeRenderer,
  scene: THREE.Scene,
  camera: THREE.Camera
): Promise<void> {
  await Promise.resolve(renderer.render(scene, camera));
}

function ensureInstancedEntry(
  scene: THREE.Scene,
  existing: InstancedEntry | undefined,
  geometry: THREE.BufferGeometry,
  material: THREE.Material,
  requiredCapacity: number,
  name: string
): InstancedEntry {
  if (
    existing &&
    existing.capacity >= Math.max(requiredCapacity, 1) &&
    existing.mesh.geometry === geometry &&
    existing.material === material
  ) {
    return existing;
  }

  if (existing) {
    scene.remove(existing.mesh);
    disposeInstancedEntry(existing);
  }

  const mesh = new THREE.InstancedMesh(
    geometry,
    material,
    Math.max(requiredCapacity, 1)
  );
  mesh.name = name;
  mesh.matrixAutoUpdate = false;
  scene.add(mesh);

  return {
    capacity: Math.max(requiredCapacity, 1),
    mesh,
    material
  };
}

function cleanupUnusedEntries(
  scene: THREE.Scene,
  entries: Map<string, InstancedEntry>,
  activeKeys: Set<string>
): void {
  for (const [key, entry] of entries) {
    if (activeKeys.has(key)) {
      continue;
    }

    scene.remove(entry.mesh);
    disposeInstancedEntry(entry);
    entries.delete(key);
  }
}

function pruneExpiredPulses(pulses: Map<number, number>, now: number): void {
  for (const [entityId, untilMs] of pulses) {
    if (untilMs <= now) {
      pulses.delete(entityId);
    }
  }
}

function pulseStrengthForEntity(
  pulses: Map<number, number>,
  entityId: number | undefined,
  now: number
): number {
  if (entityId == null) {
    return 0;
  }

  const untilMs = pulses.get(entityId);
  if (!untilMs || untilMs <= now) {
    return 0;
  }

  return clamp((untilMs - now) / 260, 0, 1);
}

function pulseDurationForEvent(event: NetworkGameEvent): number {
  const kind = event.kind.toLowerCase();
  if (
    kind.includes("damage") ||
    kind.includes("attack") ||
    kind.includes("hit") ||
    kind.includes("capture") ||
    kind.includes("defeat")
  ) {
    return 260;
  }

  if (
    kind.includes("loot") ||
    kind.includes("gather") ||
    kind.includes("summon") ||
    kind.includes("command")
  ) {
    return 180;
  }

  return 0;
}

function disposeInstancedEntry(entry: InstancedEntry): void {
  entry.mesh.dispose();
  entry.material.dispose();
}

function disposeObject(object: THREE.Object3D): void {
  object.traverse((child: THREE.Object3D) => {
    if (!(child instanceof THREE.Mesh)) {
      return;
    }

    if (Array.isArray(child.material)) {
      for (const material of child.material) {
        material.dispose();
      }
    } else {
      child.material.dispose();
    }
  });
}

function createStarfieldGeometry(count: number, radius: number): THREE.BufferGeometry {
  const positions = new Float32Array(count * 3);

  for (let index = 0; index < count; index += 1) {
    const theta = (index * 2.399963229728653) % (Math.PI * 2);
    const phi = Math.acos(1 - 2 * ((index + 0.5) / count));
    const distance = radius * (0.65 + (index % 17) / 32);
    const sinPhi = Math.sin(phi);
    positions[index * 3] = Math.cos(theta) * sinPhi * distance;
    positions[index * 3 + 1] = Math.cos(phi) * distance;
    positions[index * 3 + 2] = Math.sin(theta) * sinPhi * distance;
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  return geometry;
}

function readNavigatorDeviceMemory(): number | undefined {
  if (typeof navigator !== "object") {
    return undefined;
  }
  const navigatorWithMemory = navigator as Navigator & { deviceMemory?: number };
  return navigatorWithMemory.deviceMemory;
}

function readNavigatorHardwareConcurrency(): number {
  return typeof navigator === "object" && typeof navigator.hardwareConcurrency === "number"
    ? navigator.hardwareConcurrency
    : 4;
}

function readDevicePixelRatio(): number {
  if (typeof window === "object" && typeof window.devicePixelRatio === "number") {
    return window.devicePixelRatio;
  }

  const scopeWithDevicePixelRatio = globalThis as typeof globalThis & {
    devicePixelRatio?: number;
  };
  return scopeWithDevicePixelRatio.devicePixelRatio ?? 1;
}

function readRenderSurfaceSize(
  canvas: HTMLCanvasElement | OffscreenCanvas,
  surfaceMetrics: RenderSurfaceMetrics | null = null
): { width: number; height: number } {
  if (surfaceMetrics) {
    return {
      width: Math.max(Math.round(surfaceMetrics.width), 1),
      height: Math.max(Math.round(surfaceMetrics.height), 1)
    };
  }

  if (isHtmlCanvasElement(canvas)) {
    return {
      width:
        canvas.clientWidth ||
        (typeof window === "object" ? window.innerWidth : canvas.width || 1),
      height:
        canvas.clientHeight ||
        (typeof window === "object" ? window.innerHeight : canvas.height || 1)
    };
  }

  return {
    width: Math.max(canvas.width, 1),
    height: Math.max(canvas.height, 1)
  };
}

function isHtmlCanvasElement(
  canvas: HTMLCanvasElement | OffscreenCanvas
): canvas is HTMLCanvasElement {
  return "clientWidth" in canvas;
}

function installThreeConsoleFilter(): void {
  if (installedThreeConsoleFilter) {
    return;
  }

  const previous = THREE.getConsoleFunction();
  THREE.setConsoleFunction((level, message, ...params) => {
    if (level === "warn" && message === INLINE_TSL_FN_WARNING) {
      if (didReportInlineFnWarning) {
        return;
      }

      didReportInlineFnWarning = true;
      forwardThreeConsoleMessage(
        level,
        `${message} Duplicate warnings suppressed by POD; this is currently an upstream Three.js WebGPU material compilation warning.`,
        params,
        previous
      );
      return;
    }

    forwardThreeConsoleMessage(level, message, params, previous);
  });
  installedThreeConsoleFilter = true;
}

function forwardThreeConsoleMessage(
  level: ThreeConsoleLevel,
  message: string,
  params: unknown[],
  previous: ReturnType<typeof THREE.getConsoleFunction> | undefined
): void {
  if (previous) {
    previous(level, message, ...params);
    return;
  }

  const consoleMethod =
    level === "warn" ? console.warn : level === "error" ? console.error : console.log;
  consoleMethod(message, ...params);
}
