import * as THREE from "three";

import {
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
  type PodThreeCameraRigOptions,
  type PlannedMeshBatch,
  type PlannedSpriteBatch
} from "./frame-plan";
import {
  legacyFrameToThreeJsFrame,
  type RenderCommand,
  type RenderFrame,
  type ThreeJsWebGpuFrame
} from "./contracts";

type RuntimeRenderer = THREE.WebGLRenderer & {
  init?: () => Promise<void>;
  backend?: { isWebGPUBackend?: boolean };
  renderAsync?: (scene: THREE.Scene, camera: THREE.Camera) => Promise<void>;
};

interface InstancedEntry {
  capacity: number;
  mesh: THREE.InstancedMesh;
  material: THREE.Material;
}

export interface PodThreeWorldRendererOptions {
  assetRegistry?: PodThreeAssetRegistry;
  cameraRig?: PodThreeCameraRigOptions;
  clearColor?: number;
  enableShadows?: boolean;
  showGrid?: boolean;
  maxPixelRatio?: number;
}

export class PodThreeWorldRenderer {
  static async create(
    canvas: HTMLCanvasElement,
    options: PodThreeWorldRendererOptions = {}
  ): Promise<PodThreeWorldRenderer> {
    const { renderer, backend } = await createRenderer(canvas, options);
    return new PodThreeWorldRenderer(canvas, renderer, backend, options);
  }

  readonly scene = new THREE.Scene();
  readonly overlayScene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(55, 1, 0.1, 1024);
  readonly overlayCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, -10, 10);
  readonly assetRegistry: PodThreeAssetRegistry;
  readonly backend: "webgpu" | "webgl2";

  private readonly meshEntries = new Map<string, InstancedEntry>();
  private readonly spriteEntries = new Map<string, InstancedEntry>();
  private readonly overlayObjects = new Array<THREE.Object3D>();
  private readonly resizeObserver: ResizeObserver;
  private readonly options: Required<Pick<PodThreeWorldRendererOptions, "showGrid" | "enableShadows">> &
    Omit<PodThreeWorldRendererOptions, "showGrid" | "enableShadows">;

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly renderer: RuntimeRenderer,
    backend: "webgpu" | "webgl2",
    options: PodThreeWorldRendererOptions
  ) {
    this.backend = backend;
    this.assetRegistry = options.assetRegistry ?? new DefaultPodThreeAssetRegistry();
    this.options = {
      ...options,
      showGrid: options.showGrid ?? true,
      enableShadows: options.enableShadows ?? true
    };

    this.bootstrapScenes();
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(canvas);
    this.resize();
  }

  async applyFrame(frame: ThreeJsWebGpuFrame): Promise<void> {
    this.scene.background = new THREE.Color(...frame.backgroundColor.slice(0, 3));

    const planned = buildFramePlan(frame, this.options.cameraRig);
    this.applyCamera(planned.camera, frame.camera);
    await this.syncMeshBatches(planned.meshBatches);
    await this.syncSpriteBatches(planned.spriteBatches);
    await this.syncOverlay(frame.overlayCommands);
    await this.renderFrame();
  }

  async applyLegacyFrame(frame: RenderFrame): Promise<void> {
    await this.applyFrame(legacyFrameToThreeJsFrame(frame));
  }

  dispose(): void {
    this.resizeObserver.disconnect();

    for (const entry of this.meshEntries.values()) {
      disposeInstancedEntry(entry);
    }
    for (const entry of this.spriteEntries.values()) {
      disposeInstancedEntry(entry);
    }

    this.clearOverlay();
    this.renderer.dispose();
  }

  private bootstrapScenes(): void {
    this.scene.fog = new THREE.Fog(0x09111b, 28, 180);

    const hemisphere = new THREE.HemisphereLight(0xa8d1ff, 0x14263f, 1.2);
    this.scene.add(hemisphere);

    const sun = new THREE.DirectionalLight(0xfff0cf, 2.6);
    sun.position.set(24, 42, 18);
    sun.castShadow = this.options.enableShadows;
    sun.shadow.mapSize.set(2048, 2048);
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

    if (this.options.showGrid) {
      const grid = new THREE.GridHelper(180, 60, 0x5fa7ff, 0x173049);
      grid.position.y = 0.02;
      this.scene.add(grid);
    }

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
      activeKeys.add(planned.key);
      const geometry = await this.assetRegistry.resolveGeometry(planned.batch);
      const material =
        (await this.assetRegistry.resolveMeshMaterial?.(planned.batch)) ??
        createMeshMaterial(planned.batch);
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
      activeKeys.add(planned.key);
      const resolved = await this.assetRegistry.resolveSpriteTexture({
        texture: planned.batch.texture,
        frame: planned.batch.frame
      });
      const material = createSpriteMaterial(
        resolved,
        planned.tint,
        planned.batch.transparent,
        planned.batch.depthWrite,
        planned.batch.depthTest,
        THREE.DoubleSide
      );

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

  private async renderFrame(): Promise<void> {
    await renderWithFallback(this.renderer, this.scene, this.camera);
    this.renderer.clearDepth();
    await renderWithFallback(this.renderer, this.overlayScene, this.overlayCamera);
  }

  private resize(): void {
    const width = this.canvas.clientWidth || window.innerWidth;
    const height = this.canvas.clientHeight || window.innerHeight;
    const pixelRatio = Math.min(window.devicePixelRatio, this.options.maxPixelRatio ?? 2);
    this.renderer.setPixelRatio(pixelRatio);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / Math.max(height, 1);
    this.camera.updateProjectionMatrix();
  }
}

async function createRenderer(
  canvas: HTMLCanvasElement,
  options: PodThreeWorldRendererOptions
): Promise<{ renderer: RuntimeRenderer; backend: "webgpu" | "webgl2" }> {
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
