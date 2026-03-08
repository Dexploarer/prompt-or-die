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
  buildCameraPose,
  buildFramePlan,
  planningOptionsFromQuality,
  type PodThreeCameraRigOptions,
  type PlannedMeshBatch,
  type PlannedSpriteBatch
} from "./frame-plan";
import {
  legacyFrameToThreeJsFrame,
  type RenderCommand,
  type RenderFrame,
  type TelemetryTrajectorySample,
  type ThreeJsWebGpuFrame
} from "./contracts";
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
let installedThreeConsoleFilter = false;
let didReportInlineFnWarning = false;

export interface PodThreeRendererStats {
  backend: "webgpu" | "webgl2";
  renderThread: "main" | "worker";
  qualityPreset: PodThreeQualityPreset;
  pixelRatio: number;
  drawCalls: number;
  triangles: number;
  textures: number;
  frameMs: number;
  residentGeometryAssets: number;
  residentSpriteAssets: number;
  pendingGeometryAssets: number;
  pendingSpriteAssets: number;
}

export interface PodThreeWorldRendererOptions {
  assetRegistry?: PodThreeAssetRegistry;
  cameraRig?: PodThreeCameraRigOptions;
  qualityPreset?: PodThreeQualityPreset;
  qualityProfile?: Partial<PodThreeQualityProfile>;
  clearColor?: number;
  enableShadows?: boolean;
  showGrid?: boolean;
  maxPixelRatio?: number;
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
  private readonly spriteEntries = new Map<string, InstancedEntry>();
  private readonly meshMaterialCache = new Map<string, THREE.Material>();
  private readonly spriteMaterialCache = new Map<string, THREE.Material>();
  private readonly overlayObjects = new Array<THREE.Object3D>();
  private readonly resizeObserver: ResizeObserver | null;
  private readonly options: PodThreeWorldRendererOptions;
  private adaptivePixelRatio: number;
  private smoothedFrameMs = 16.7;
  private adjustmentCooldown = 0;
  private telemetryTrail: THREE.Line<THREE.BufferGeometry, THREE.LineBasicMaterial> | null =
    null;

  constructor(
    private readonly canvas: HTMLCanvasElement | OffscreenCanvas,
    private readonly renderer: RuntimeRenderer,
    backend: "webgpu" | "webgl2",
    options: PodThreeWorldRendererOptions
  ) {
    this.backend = backend;
    this.assetRegistry = options.assetRegistry ?? new DefaultPodThreeAssetRegistry();
    const deviceMemory = readNavigatorDeviceMemory();
    const baseQuality = resolveQualityProfile({
      backend,
      preferredPreset: options.qualityPreset,
      hardwareConcurrency: readNavigatorHardwareConcurrency(),
      deviceMemory,
      devicePixelRatio: readDevicePixelRatio()
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
    this.scene.background = new THREE.Color(...frame.backgroundColor.slice(0, 3));

    const planned = buildFramePlan(frame, {
      ...this.options.cameraRig,
      ...planningOptionsFromQuality(this.quality)
    });
    this.applyCamera(planned.camera, frame.camera);
    await this.prewarmPlannedAssets(planned);
    await this.syncMeshBatches(planned.meshBatches);
    await this.syncSpriteBatches(planned.spriteBatches);
    await this.syncOverlay(frame.overlayCommands);
    await this.renderFrame();
  }

  async applyLegacyFrame(frame: RenderFrame): Promise<void> {
    await this.applyFrame(legacyFrameToThreeJsFrame(frame));
  }

  dispose(): void {
    this.resizeObserver?.disconnect();

    for (const entry of this.meshEntries.values()) {
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

  private bootstrapScenes(): void {
    this.scene.fog = new THREE.Fog(0x09111b, 28, 180);
    this.scene.backgroundIntensity = 0.8;

    const hemisphere = new THREE.HemisphereLight(0xa8d1ff, 0x14263f, 1.2);
    this.scene.add(hemisphere);

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

    const fill = new THREE.DirectionalLight(0x6cbcff, 0.7);
    fill.position.set(-18, 14, -10);
    this.scene.add(fill);

    const rim = new THREE.PointLight(0x4bc1ff, 12, 180, 2.2);
    rim.position.set(0, 26, 0);
    this.scene.add(rim);

    const ground = new THREE.Mesh(
      new THREE.CircleGeometry(140, 64),
      new THREE.MeshStandardMaterial({
        color: 0x0e1724,
        roughness: 0.95,
        metalness: 0.02
      })
    );
    ground.rotation.x = -Math.PI / 2;
    ground.receiveShadow = true;
    this.scene.add(ground);

    if (this.quality.showGrid) {
      const grid = new THREE.GridHelper(180, 60, 0x5fa7ff, 0x173049);
      grid.position.y = 0.02;
      this.scene.add(grid);
    }

    const skyline = new THREE.Points(
      createStarfieldGeometry(640, 220),
      new THREE.PointsMaterial({
        color: 0xcde7ff,
        size: 0.9,
        sizeAttenuation: true,
        transparent: true,
        opacity: 0.9,
        depthWrite: false
      })
    );
    skyline.position.y = 36;
    this.scene.add(skyline);

    this.overlayScene.background = null;
    this.overlayCamera.position.set(0, 0, 5);
    this.overlayCamera.lookAt(0, 0, 0);
  }

  private applyCamera(
    pose: ReturnType<typeof buildCameraPose>,
    frameCamera: ThreeJsWebGpuFrame["camera"]
  ): void {
    this.camera.position.set(...pose.position);
    this.camera.quaternion.copy(pose.quaternion);
    this.camera.fov = pose.fov;
    this.camera.near = pose.near;
    this.camera.far = pose.far;
    this.camera.lookAt(...pose.target);
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
        planned.matrices.length,
        planned.key
      );

      entry.mesh.castShadow = planned.batch.castShadows;
      entry.mesh.receiveShadow = planned.batch.receiveShadows;
      entry.mesh.count = planned.matrices.length;
      entry.mesh.renderOrder = planned.batch.renderOrder;
      entry.mesh.visible = true;
      entry.mesh.name = planned.key;
      entry.mesh.frustumCulled = false;

      for (let index = 0; index < planned.matrices.length; index += 1) {
        entry.mesh.setMatrixAt(index, planned.matrices[index]);
      }
      entry.mesh.instanceMatrix.needsUpdate = true;
      this.meshEntries.set(planned.key, entry);
    }

    cleanupUnusedEntries(this.scene, this.meshEntries, activeKeys);
  }

  private async syncSpriteBatches(batches: PlannedSpriteBatch[]): Promise<void> {
    const activeKeys = new Set<string>();

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
        planned.matrices.length,
        planned.key
      );

      entry.mesh.castShadow = false;
      entry.mesh.receiveShadow = false;
      entry.mesh.count = planned.matrices.length;
      entry.mesh.renderOrder = planned.batch.renderOrder;
      entry.mesh.visible = true;
      entry.mesh.frustumCulled = false;

      for (let index = 0; index < planned.matrices.length; index += 1) {
        entry.mesh.setMatrixAt(index, planned.matrices[index]);
      }
      entry.mesh.instanceMatrix.needsUpdate = true;
      this.spriteEntries.set(planned.key, entry);
    }

    cleanupUnusedEntries(this.scene, this.spriteEntries, activeKeys);
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
          0.22 + index * 0.0035,
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
    const { width, height } = readRenderSurfaceSize(this.canvas);
    const pixelRatio = Math.min(readDevicePixelRatio(), this.adaptivePixelRatio);
    this.renderer.setPixelRatio(pixelRatio);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / Math.max(height, 1);
    this.camera.updateProjectionMatrix();
  }

  getStats(): PodThreeRendererStats {
    const residency = this.assetRegistry.getResidencyStats?.() ?? {
      residentGeometryAssets: 0,
      residentSpriteAssets: 0,
      pendingGeometryAssets: 0,
      pendingSpriteAssets: 0
    };

    return {
      backend: this.backend,
      renderThread: "main",
      qualityPreset: this.quality.preset,
      pixelRatio: this.renderer.getPixelRatio(),
      drawCalls: this.renderer.info.render.calls,
      triangles: this.renderer.info.render.triangles,
      textures: this.renderer.info.memory.textures,
      frameMs: Number(this.smoothedFrameMs.toFixed(2)),
      residentGeometryAssets: residency.residentGeometryAssets,
      residentSpriteAssets: residency.residentSpriteAssets,
      pendingGeometryAssets: residency.pendingGeometryAssets,
      pendingSpriteAssets: residency.pendingSpriteAssets
    };
  }

  private async prewarmPlannedAssets(
    planned: ReturnType<typeof buildFramePlan>
  ): Promise<void> {
    await Promise.all([
      this.assetRegistry.prefetchMeshes?.(
        planned.meshBatches
          .filter((batch) => batch.visibleCount > 0)
          .map((batch) => ({ batch: batch.batch, lodLevel: batch.lodLevel }))
      ),
      this.assetRegistry.prefetchSprites?.(
        planned.spriteBatches
          .filter((batch) => batch.visibleCount > 0)
          .map((batch) => ({
            batch: {
              texture: batch.batch.texture,
              frame: batch.batch.frame
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

async function createRenderer(
  canvas: HTMLCanvasElement | OffscreenCanvas,
  options: PodThreeWorldRendererOptions
): Promise<{ renderer: RuntimeRenderer; backend: "webgpu" | "webgl2" }> {
  installThreeConsoleFilter();

  if ("gpu" in navigator) {
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
  canvas: HTMLCanvasElement | OffscreenCanvas
): { width: number; height: number } {
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
