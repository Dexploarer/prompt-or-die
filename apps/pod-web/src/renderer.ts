import * as THREE from "three";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";

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
  sampleAnimatedInstanceTransform,
  planningOptionsFromQuality,
  type PodThreeCameraRigOptions,
  type PlannedMeshBatch,
  type PlannedSpriteBatch
} from "./frame-plan";
import { meshGroundAnchorHeight } from "./mesh-bounds";
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
  mixVec3,
  sampleBackcountryMask,
  sampleLakeMask,
  sampleTerrainHeight,
  sampleTerrainMaterial,
  sampleTerrainSlope,
  sampleTimeLapseEnvironment,
  sampleRiverChannelMask,
  sampleValleyFloorMask,
  sampleWaterSurfaceStyle,
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

interface SmoothedInstanceTransform {
  position: THREE.Vector3;
  rotation: THREE.Quaternion;
  scale: THREE.Vector3;
  updatedAt: number;
}

interface ResolvedMeshBatchResources {
  planned: PlannedMeshBatch;
  geometry: THREE.BufferGeometry;
  material: THREE.Material;
}

interface ResolvedSpriteBatchResources {
  planned: PlannedSpriteBatch;
  resolved: Awaited<ReturnType<PodThreeAssetRegistry["resolveSpriteTexture"]>>;
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

function monotonicPerfNowMs(): number {
  return typeof performance !== "undefined" && typeof performance.now === "function"
    ? performance.now()
    : Date.now();
}

function resolveLandscapeSurfaceSize(
  quality: PodThreeQualityProfile,
  kind: "terrain" | "water" | "sky"
): number {
  if (kind === "terrain") {
    return quality.terrainTextureSize;
  }
  if (kind === "water") {
    return quality.waterTextureSize;
  }
  return quality.skyTextureSize;
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
    const height = sampleTerrainHeight(x, y);
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
  const imageData = context.createImageData(width, height);

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const worldX = (x / Math.max(width - 1, 1) - 0.5) * LANDSCAPE_WORLD_SIZE;
      const worldZ = (y / Math.max(height - 1, 1) - 0.5) * LANDSCAPE_WORLD_SIZE;
      const material = sampleTerrainMaterial(environment, worldX, worldZ);
      const microNoise = fractalNoise(worldX * 0.22 + 17, worldZ * 0.22 - 9) - 0.5;
      const macroNoise = fractalNoise(worldX * 0.055 - 21, worldZ * 0.055 + 13) - 0.5;
      const dx = sampleTerrainHeight(worldX + 0.8, worldZ) - sampleTerrainHeight(worldX - 0.8, worldZ);
      const dz = sampleTerrainHeight(worldX, worldZ + 0.8) - sampleTerrainHeight(worldX, worldZ - 0.8);
      const normalX = -dx;
      const normalY = 1.6;
      const normalZ = -dz;
      const normalLength = Math.hypot(normalX, normalY, normalZ) || 1;
      const lightX = 0.48;
      const lightY = 0.8;
      const lightZ = 0.34;
      const lambert = clamp(
        (normalX * lightX + normalY * lightY + normalZ * lightZ) / normalLength,
        0.18,
        1
      );
      const contourShade =
        0.84 +
        lambert * 0.22 +
        microNoise * 0.08 +
        macroNoise * 0.05 -
        material.cliffMask * 0.08 +
        material.foamMask * 0.04;
      const shadedBrightness = clamp(
        material.brightness * contourShade,
        0.4,
        1.24
      );
      const index = (y * width + x) * 4;
      imageData.data[index] = Math.round(material.tint[0] * shadedBrightness * 255);
      imageData.data[index + 1] = Math.round(material.tint[1] * shadedBrightness * 255);
      imageData.data[index + 2] = Math.round(material.tint[2] * shadedBrightness * 255);
      imageData.data[index + 3] = 255;
    }
  }

  context.putImageData(imageData, 0, 0);
}

function paintWaterTexture(
  surface: PaintSurface,
  environment: ThreeJsEnvironment,
  elapsedSeconds: number
): void {
  const context = getPaintContext(surface);
  const width = "width" in surface ? surface.width : 512;
  const height = "height" in surface ? surface.height : 512;
  const style = sampleWaterSurfaceStyle(environment, elapsedSeconds);
  const gradient = context.createLinearGradient(0, 0, 0, height);
  gradient.addColorStop(
    0,
    `rgb(${Math.round(style.highlightColor[0] * 255)}, ${Math.round(
      style.highlightColor[1] * 255
    )}, ${Math.round(style.highlightColor[2] * 255)})`
  );
  gradient.addColorStop(
    0.34,
    `rgb(${Math.round(style.shallowColor[0] * 255)}, ${Math.round(
      style.shallowColor[1] * 255
    )}, ${Math.round(style.shallowColor[2] * 255)})`
  );
  gradient.addColorStop(
    1,
    `rgb(${Math.round(style.deepColor[0] * 255)}, ${Math.round(
      style.deepColor[1] * 255
    )}, ${Math.round(style.deepColor[2] * 255)})`
  );
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
  radial.addColorStop(0, `rgba(255,255,255,${(0.11 + style.waveStrength * 0.08).toFixed(3)})`);
  radial.addColorStop(0.45, `rgba(170,232,255,${(0.04 + style.waveStrength * 0.06).toFixed(3)})`);
  radial.addColorStop(1, "rgba(0,0,0,0)");
  context.fillStyle = radial;
  context.fillRect(0, 0, width, height);

  const reflectionBand = context.createLinearGradient(
    width * 0.08,
    height * 0.16,
    width * 0.92,
    height * 0.84
  );
  reflectionBand.addColorStop(0, "rgba(255,255,255,0)");
  reflectionBand.addColorStop(
    0.34,
    `rgba(236, 248, 255, ${(0.08 + style.waveStrength * 0.08).toFixed(3)})`
  );
  reflectionBand.addColorStop(
    0.58,
    `rgba(178, 228, 242, ${(0.06 + style.waveStrength * 0.06).toFixed(3)})`
  );
  reflectionBand.addColorStop(1, "rgba(255,255,255,0)");
  context.fillStyle = reflectionBand;
  context.fillRect(0, 0, width, height);

  context.strokeStyle = `rgba(255, 255, 255, ${(0.05 + style.waveStrength * 0.06).toFixed(3)})`;
  for (let stripe = 0; stripe < 6; stripe += 1) {
    const offsetY = height * (0.18 + stripe * 0.12);
    context.lineWidth = 1.4 + stripe * 0.18;
    context.beginPath();
    for (let x = 0; x <= width; x += 20) {
      const waveY =
        offsetY +
        Math.sin(
          (x / width) * Math.PI * (2.1 + stripe * 0.22) +
            stripe * 0.9 +
            elapsedSeconds * (0.38 + style.waveStrength * 0.16)
        ) *
          (2.8 + stripe * 0.35 + style.waveStrength * 1.1) +
        Math.cos(
          (x / width) * Math.PI * 1.35 +
            stripe * 0.44 +
            elapsedSeconds * 0.22
        ) *
          1.1;
      if (x === 0) {
        context.moveTo(x, waveY);
      } else {
        context.lineTo(x, waveY);
      }
    }
    context.stroke();
  }

  context.strokeStyle = `rgba(214, 245, 255, ${(0.1 + style.waveStrength * 0.08).toFixed(3)})`;
  for (let streak = 0; streak < 4; streak += 1) {
    const radius = width * (0.12 + streak * 0.1);
    context.lineWidth = 0.9 + streak * 0.22;
    context.beginPath();
    context.ellipse(
      width * (0.32 + streak * 0.14),
      height * (0.46 - streak * 0.04),
      radius * 1.22,
      radius * 0.26,
      -0.32,
      0,
      Math.PI * 2
    );
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

function buildRiverProfile(pointCount = 44): Array<{ x: number; z: number; width: number }> {
  const points = new Array<{ x: number; z: number; width: number }>();

  for (let index = 0; index <= pointCount; index += 1) {
    const t = index / pointCount;
    const x =
      76 -
      t * 52 +
      Math.sin(t * Math.PI * 1.35 + 0.28) * 10 +
      (fractalNoise(t * 9.5 + 11, t * 7.1 - 3) - 0.5) * 8;
    const z =
      74 -
      t * 176 +
      Math.cos(t * Math.PI * 2.15 - 0.44) * 6 +
      (fractalNoise(t * 8.3 - 7, t * 6.4 + 15) - 0.5) * 6;
    const width =
      10.5 -
      t * 4.4 +
      Math.sin(t * Math.PI * 3.2 + 0.6) * 0.8 +
      (fractalNoise(t * 10.2 + 23, t * 4.8 - 17) - 0.5) * 0.9;
    points.push({
      x,
      z,
      width: Math.max(width, 4.6)
    });
  }

  return points;
}

function createRiverRibbonGeometry(widthScale = 1): THREE.BufferGeometry {
  const profile = buildRiverProfile();
  const positions = new Float32Array(profile.length * 2 * 3);
  const uvs = new Float32Array(profile.length * 2 * 2);
  const indices = new Array<number>();

  for (let index = 0; index < profile.length; index += 1) {
    const current = profile[index];
    const previous = profile[Math.max(index - 1, 0)] ?? current;
    const next = profile[Math.min(index + 1, profile.length - 1)] ?? current;
    const tangentX = next.x - previous.x;
    const tangentZ = next.z - previous.z;
    const tangentLength = Math.hypot(tangentX, tangentZ) || 1;
    const normalX = -tangentZ / tangentLength;
    const normalZ = tangentX / tangentLength;
    const width = current.width * widthScale;
    const leftX = current.x + normalX * width;
    const leftZ = current.z + normalZ * width;
    const rightX = current.x - normalX * width;
    const rightZ = current.z - normalZ * width;
    const positionOffset = index * 6;
    const uvOffset = index * 4;

    positions[positionOffset] = leftX;
    positions[positionOffset + 1] = 0;
    positions[positionOffset + 2] = leftZ;
    positions[positionOffset + 3] = rightX;
    positions[positionOffset + 4] = 0;
    positions[positionOffset + 5] = rightZ;

    uvs[uvOffset] = 0;
    uvs[uvOffset + 1] = index / Math.max(profile.length - 1, 1);
    uvs[uvOffset + 2] = 1;
    uvs[uvOffset + 3] = index / Math.max(profile.length - 1, 1);

    if (index < profile.length - 1) {
      const base = index * 2;
      indices.push(base, base + 1, base + 2, base + 1, base + 3, base + 2);
    }
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.BufferAttribute(uvs, 2));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

function wrappedAngleDistance(left: number, right: number): number {
  return Math.abs(Math.atan2(Math.sin(left - right), Math.cos(left - right)));
}

function createMountainBackdropGeometry(
  radius = LANDSCAPE_WORLD_SIZE * 0.58,
  width = 56,
  segments = 88
): THREE.BufferGeometry {
  const positions = new Float32Array((segments + 1) * 2 * 3);
  const colors = new Float32Array((segments + 1) * 2 * 3);
  const indices = new Array<number>();

  for (let index = 0; index <= segments; index += 1) {
    const angle = (index / segments) * Math.PI * 2;
    const ridgeNoise = fractalNoise(Math.cos(angle) * 7 + 4, Math.sin(angle) * 7 - 9);
    const sharpNoise = Math.pow(fractalNoise(Math.cos(angle) * 11 - 3, Math.sin(angle) * 11 + 5), 1.6);
    const backdropMask =
      1 - smoothstep(0.22, 1.18, wrappedAngleDistance(angle, Math.PI * 1.5));
    const westPeakMask =
      1 - smoothstep(0.12, 0.46, wrappedAngleDistance(angle, Math.PI * 1.34));
    const eastPeakMask =
      1 - smoothstep(0.12, 0.42, wrappedAngleDistance(angle, Math.PI * 1.73));
    const foregroundOpening =
      1 - smoothstep(0.14, 0.54, wrappedAngleDistance(angle, Math.PI * 0.5));
    const scenicBias = backdropMask * 0.9 + westPeakMask * 0.55 + eastPeakMask * 0.48;
    const peakHeight =
      34 +
      ridgeNoise * 42 +
      sharpNoise * 34 +
      backdropMask * 72 +
      westPeakMask * 54 +
      eastPeakMask * 48 -
      foregroundOpening * 22;
    const innerRadius = radius - foregroundOpening * 18 + backdropMask * 8;
    const outerRadius =
      radius +
      width +
      ridgeNoise * 18 +
      sharpNoise * 8 +
      scenicBias * 24 -
      foregroundOpening * 10;
    const sin = Math.sin(angle);
    const cos = Math.cos(angle);
    const offset = index * 6;
    const snowBlend = clamp((peakHeight - 76) / 46, 0, 1);
    const ridgeBlend = clamp((peakHeight - 44) / 52, 0, 1);
    const lowerColor = mixVec3([0.28, 0.24, 0.2], [0.38, 0.34, 0.28], backdropMask * 0.42);
    const upperRock = mixVec3([0.66, 0.58, 0.42], [0.84, 0.76, 0.56], ridgeBlend * 0.72);
    const upperColor = mixVec3(upperRock, [0.98, 0.96, 0.92], snowBlend * 0.9);

    positions[offset] = cos * innerRadius;
    positions[offset + 1] = -12;
    positions[offset + 2] = sin * innerRadius;

    positions[offset + 3] = cos * outerRadius;
    positions[offset + 4] = peakHeight;
    positions[offset + 5] = sin * outerRadius;

    colors[offset] = lowerColor[0];
    colors[offset + 1] = lowerColor[1];
    colors[offset + 2] = lowerColor[2];
    colors[offset + 3] = upperColor[0];
    colors[offset + 4] = upperColor[1];
    colors[offset + 5] = upperColor[2];

    if (index < segments) {
      const base = index * 2;
      indices.push(base, base + 1, base + 2, base + 1, base + 3, base + 2);
    }
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

function createCloudLayerGeometry(
  radius = LANDSCAPE_WORLD_SIZE * 0.74,
  clusterCount = 52
): THREE.BufferGeometry {
  const pieces = new Array<THREE.BufferGeometry>();

  for (let clusterIndex = 0; clusterIndex < clusterCount; clusterIndex += 1) {
    const angle = (clusterIndex / clusterCount) * Math.PI * 2;
    const ringNoise = fractalNoise(Math.cos(angle) * 5.4 + 7, Math.sin(angle) * 5.4 - 3);
    const centerRadius = radius + ringNoise * 18;
    const centerX = Math.cos(angle) * centerRadius;
    const centerZ = Math.sin(angle) * centerRadius;
    const centerY = 68 + ringNoise * 14;
    const puffCount = 4 + (clusterIndex % 4);

    for (let puffIndex = 0; puffIndex < puffCount; puffIndex += 1) {
      const puffNoise = fractalNoise(
        clusterIndex * 0.8 + puffIndex * 1.7,
        clusterIndex * 1.1 - puffIndex * 0.6
      );
      const puff = new THREE.IcosahedronGeometry(6.2 + puffNoise * 5.4, 1);
      puff.scale(2.05 + puffNoise * 1.24, 0.74 + puffNoise * 0.24, 1.18 + puffNoise * 0.48);
      puff.translate(
        centerX + (puffIndex - puffCount * 0.5) * (5.8 + puffNoise * 2.4),
        centerY + Math.sin(puffIndex * 1.7 + angle) * 2.8,
        centerZ + Math.cos(puffIndex * 1.9 - angle) * 4.6
      );
      pieces.push(puff);
    }
  }

  return mergeGeometries(pieces, false) ?? new THREE.IcosahedronGeometry(6, 1);
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
  requestedRenderThread: "auto" | "main" | "worker";
  renderThreadFallbackReason:
    | "missing-worker-constructor"
    | "missing-offscreen-canvas"
    | "missing-canvas-transfer-control"
    | null;
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
  geometryLoadsCompleted: number;
  spriteLoadsCompleted: number;
  averageGeometryLoadMs: number;
  averageSpriteLoadMs: number;
  slowestGeometryLoadMs: number;
  slowestSpriteLoadMs: number;
  mainThreadPerf: PodThreeMainThreadPerfStats;
  runtimePerf: PodThreeRuntimePerfStats;
  ambientInstances: number;
  visibleWorldChunks: number;
  preloadedWorldChunks: number;
}

export interface PodThreeMainThreadPerfStats {
  warmupMs: number | null;
  submissionsCompleted: number;
  averageSubmissionMs: number;
  slowestSubmissionMs: number;
  byKind: Record<PodThreeMainThreadSubmissionKind, PodThreeMainThreadPerfBucketStats>;
}

export type PodThreeMainThreadSubmissionKind = "frame" | "control" | "resize";

export interface PodThreeMainThreadPerfBucketStats {
  submissionsCompleted: number;
  averageSubmissionMs: number;
  slowestSubmissionMs: number;
}

export interface PodThreeMainThreadPerfBucketTracker {
  submissionsCompleted: number;
  totalSubmissionMs: number;
  slowestSubmissionMs: number;
}

export interface PodThreeMainThreadPerfTracker {
  startedAtMs: number;
  warmupMs: number | null;
  submissionsCompleted: number;
  totalSubmissionMs: number;
  slowestSubmissionMs: number;
  byKind: Record<PodThreeMainThreadSubmissionKind, PodThreeMainThreadPerfBucketTracker>;
}

export function createPodThreeMainThreadPerfTracker(
  startedAtMs = 0
): PodThreeMainThreadPerfTracker {
  return {
    startedAtMs: Math.max(startedAtMs, 0),
    warmupMs: null,
    submissionsCompleted: 0,
    totalSubmissionMs: 0,
    slowestSubmissionMs: 0,
    byKind: {
      frame: createPodThreeMainThreadPerfBucketTracker(),
      control: createPodThreeMainThreadPerfBucketTracker(),
      resize: createPodThreeMainThreadPerfBucketTracker()
    }
  };
}

export function recordPodThreeMainThreadSubmission(
  tracker: PodThreeMainThreadPerfTracker,
  submissionMs: number,
  nowMs: number,
  kind: PodThreeMainThreadSubmissionKind = "frame"
): void {
  const normalizedSubmissionMs = Math.max(submissionMs, 0);
  const normalizedNowMs = Math.max(nowMs, tracker.startedAtMs);
  const bucket = tracker.byKind[kind];

  tracker.submissionsCompleted += 1;
  tracker.totalSubmissionMs += normalizedSubmissionMs;
  tracker.slowestSubmissionMs = Math.max(
    tracker.slowestSubmissionMs,
    normalizedSubmissionMs
  );
  bucket.submissionsCompleted += 1;
  bucket.totalSubmissionMs += normalizedSubmissionMs;
  bucket.slowestSubmissionMs = Math.max(
    bucket.slowestSubmissionMs,
    normalizedSubmissionMs
  );
  if (tracker.warmupMs == null) {
    tracker.warmupMs = Math.max(normalizedNowMs - tracker.startedAtMs, 0);
  }
}

export function resetPodThreeMainThreadPerfTracker(
  tracker: PodThreeMainThreadPerfTracker,
  startedAtMs = 0
): void {
  tracker.startedAtMs = Math.max(startedAtMs, 0);
  tracker.warmupMs = null;
  tracker.submissionsCompleted = 0;
  tracker.totalSubmissionMs = 0;
  tracker.slowestSubmissionMs = 0;
  tracker.byKind.frame = createPodThreeMainThreadPerfBucketTracker();
  tracker.byKind.control = createPodThreeMainThreadPerfBucketTracker();
  tracker.byKind.resize = createPodThreeMainThreadPerfBucketTracker();
}

export function snapshotPodThreeMainThreadPerfStats(
  tracker: PodThreeMainThreadPerfTracker
): PodThreeMainThreadPerfStats {
  return {
    warmupMs:
      tracker.warmupMs == null ? null : Number(tracker.warmupMs.toFixed(2)),
    submissionsCompleted: tracker.submissionsCompleted,
    averageSubmissionMs:
      tracker.submissionsCompleted === 0
        ? 0
        : Number(
            (tracker.totalSubmissionMs / tracker.submissionsCompleted).toFixed(2)
          ),
    slowestSubmissionMs: Number(tracker.slowestSubmissionMs.toFixed(2)),
    byKind: {
      frame: snapshotPodThreeMainThreadPerfBucketStats(tracker.byKind.frame),
      control: snapshotPodThreeMainThreadPerfBucketStats(tracker.byKind.control),
      resize: snapshotPodThreeMainThreadPerfBucketStats(tracker.byKind.resize)
    }
  };
}

function createPodThreeMainThreadPerfBucketTracker(): PodThreeMainThreadPerfBucketTracker {
  return {
    submissionsCompleted: 0,
    totalSubmissionMs: 0,
    slowestSubmissionMs: 0
  };
}

function snapshotPodThreeMainThreadPerfBucketStats(
  tracker: PodThreeMainThreadPerfBucketTracker
): PodThreeMainThreadPerfBucketStats {
  return {
    submissionsCompleted: tracker.submissionsCompleted,
    averageSubmissionMs:
      tracker.submissionsCompleted === 0
        ? 0
        : Number((tracker.totalSubmissionMs / tracker.submissionsCompleted).toFixed(2)),
    slowestSubmissionMs: Number(tracker.slowestSubmissionMs.toFixed(2))
  };
}

export interface PodThreeRuntimePerfStats {
  warmupMs: number | null;
  frameBudgetMs: number;
  framesRendered: number;
  stableFrames: number;
  slowFrames: number;
  stableFramePercent: number;
  slowestFrameMs: number;
}

export interface PodThreeRuntimePerfTracker {
  startedAtMs: number;
  warmupMs: number | null;
  framesRendered: number;
  stableFrames: number;
  slowFrames: number;
  slowestFrameMs: number;
}

export const POD_THREE_FRAME_STABILITY_BUDGET_MS = 1000 / 60;

export function createPodThreeRuntimePerfTracker(
  startedAtMs = 0
): PodThreeRuntimePerfTracker {
  return {
    startedAtMs: Math.max(startedAtMs, 0),
    warmupMs: null,
    framesRendered: 0,
    stableFrames: 0,
    slowFrames: 0,
    slowestFrameMs: 0
  };
}

export function recordPodThreeRuntimePerfFrame(
  tracker: PodThreeRuntimePerfTracker,
  frameMs: number,
  nowMs: number,
  frameBudgetMs = POD_THREE_FRAME_STABILITY_BUDGET_MS
): void {
  const normalizedFrameMs = Math.max(frameMs, 0);
  const normalizedNowMs = Math.max(nowMs, tracker.startedAtMs);

  tracker.framesRendered += 1;
  tracker.slowestFrameMs = Math.max(tracker.slowestFrameMs, normalizedFrameMs);
  if (normalizedFrameMs <= frameBudgetMs) {
    tracker.stableFrames += 1;
  } else {
    tracker.slowFrames += 1;
  }
  if (tracker.warmupMs == null) {
    tracker.warmupMs = Math.max(normalizedNowMs - tracker.startedAtMs, 0);
  }
}

export function resetPodThreeRuntimePerfTracker(
  tracker: PodThreeRuntimePerfTracker,
  startedAtMs = 0
): void {
  tracker.startedAtMs = Math.max(startedAtMs, 0);
  tracker.warmupMs = null;
  tracker.framesRendered = 0;
  tracker.stableFrames = 0;
  tracker.slowFrames = 0;
  tracker.slowestFrameMs = 0;
}

export function snapshotPodThreeRuntimePerfStats(
  tracker: PodThreeRuntimePerfTracker,
  frameBudgetMs = POD_THREE_FRAME_STABILITY_BUDGET_MS
): PodThreeRuntimePerfStats {
  const stableFramePercent =
    tracker.framesRendered === 0
      ? 0
      : (tracker.stableFrames / tracker.framesRendered) * 100;

  return {
    warmupMs:
      tracker.warmupMs == null ? null : Number(tracker.warmupMs.toFixed(2)),
    frameBudgetMs: Number(frameBudgetMs.toFixed(2)),
    framesRendered: tracker.framesRendered,
    stableFrames: tracker.stableFrames,
    slowFrames: tracker.slowFrames,
    stableFramePercent: Number(stableFramePercent.toFixed(1)),
    slowestFrameMs: Number(tracker.slowestFrameMs.toFixed(2))
  };
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
  fixedTimeMs?: number;
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
        fallbackRegistry: new DefaultPodThreeAssetRegistry(),
        preferNonBlockingFallbacks: true,
        lazyManifestLoad: true
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
  private waterMesh: THREE.Mesh<THREE.BufferGeometry, THREE.MeshStandardMaterial> | null = null;
  private shorelineMesh: THREE.Mesh<THREE.BufferGeometry, THREE.MeshStandardMaterial> | null = null;
  private waterTexture: THREE.Texture | null = null;
  private waterTextureSurface: PaintSurface | null = null;
  private mountainBackdrop:
    | THREE.Mesh<THREE.BufferGeometry, THREE.MeshStandardMaterial>
    | null = null;
  private cloudLayer:
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
  private waterGeometryMode: "lake" | "river" = "lake";
  private lakeWaterGeometry: THREE.BufferGeometry | null = null;
  private riverWaterGeometry: THREE.BufferGeometry | null = null;
  private lakeShorelineGeometry: THREE.BufferGeometry | null = null;
  private riverShorelineGeometry: THREE.BufferGeometry | null = null;
  private readonly runtimePerf = createPodThreeRuntimePerfTracker(monotonicPerfNowMs());
  private telemetryTrail: THREE.Line<THREE.BufferGeometry, THREE.LineBasicMaterial> | null =
    null;
  private readonly entityPulseUntilMs = new Map<number, number>();
  private readonly smoothedInstanceTransforms = new Map<string, SmoothedInstanceTransform>();

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

  private sceneTimeMs(): number {
    if (typeof this.options.fixedTimeMs === "number") {
      return this.options.fixedTimeMs;
    }

    return typeof performance !== "undefined" && typeof performance.now === "function"
      ? performance.now()
      : Date.now();
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
    void this.prewarmPlannedAssets(planned, ambientPlan).catch((error) => {
      console.warn("Continuing without blocking on asset prewarm", error);
    });
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

    const now = this.sceneTimeMs();

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
    sun.shadow.bias = 0.00008;
    sun.shadow.normalBias = 0.5;
    sun.shadow.blurSamples = 6;
    sun.shadow.radius = 1.4;
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

    const skyTextureSize = resolveLandscapeSurfaceSize(this.quality, "sky");
    this.skyTextureSurface = createPaintSurface(skyTextureSize, skyTextureSize);
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

    const terrainTextureSize = resolveLandscapeSurfaceSize(this.quality, "terrain");
    this.terrainTextureSurface = createPaintSurface(
      terrainTextureSize,
      terrainTextureSize
    );
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
      metalness: 0.03,
      emissive: new THREE.Color(0.09, 0.11, 0.1),
      emissiveIntensity: 0.18
    });
    this.groundMaterial = groundMaterial;
    const ground = new THREE.Mesh(createTerrainGeometry(), groundMaterial);
    ground.rotation.x = -Math.PI / 2;
    ground.receiveShadow = true;
    ground.castShadow = false;
    this.scene.add(ground);
    this.terrainMesh = ground;

    const waterTextureSize = resolveLandscapeSurfaceSize(this.quality, "water");
    this.waterTextureSurface = createPaintSurface(waterTextureSize, waterTextureSize);
    paintWaterTexture(
      this.waterTextureSurface,
      defaultEnvironment,
      DAYLIGHT_START_OFFSET_SECONDS
    );
    const waterTexture = new THREE.CanvasTexture(this.waterTextureSurface);
    waterTexture.colorSpace = THREE.SRGBColorSpace;
    waterTexture.wrapS = THREE.RepeatWrapping;
    waterTexture.wrapT = THREE.RepeatWrapping;
    const defaultWaterStyle = sampleWaterSurfaceStyle(
      defaultEnvironment,
      DAYLIGHT_START_OFFSET_SECONDS
    );
    waterTexture.repeat.set(
      defaultWaterStyle.textureRepeat[0],
      defaultWaterStyle.textureRepeat[1]
    );
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
    this.lakeWaterGeometry = createLakeGeometry();
    this.riverWaterGeometry = createRiverRibbonGeometry(1);
    const water = new THREE.Mesh(this.lakeWaterGeometry, waterMaterial);
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
    this.lakeShorelineGeometry = createShorelineGeometry();
    this.riverShorelineGeometry = createRiverRibbonGeometry(1.18);
    const shoreline = new THREE.Mesh(this.lakeShorelineGeometry, shorelineMaterial);
    shoreline.position.y = WATER_LEVEL + 0.08;
    shoreline.receiveShadow = true;
    shoreline.renderOrder = 3;
    this.scene.add(shoreline);
    this.shorelineMesh = shoreline;

    const mountainBackdrop = new THREE.Mesh(
      createMountainBackdropGeometry(),
      new THREE.MeshStandardMaterial({
        color: new THREE.Color(1, 1, 1),
        roughness: 1,
        metalness: 0,
        flatShading: true,
        vertexColors: true,
        fog: true
      })
    );
    mountainBackdrop.position.y = -4;
    mountainBackdrop.receiveShadow = false;
    mountainBackdrop.castShadow = false;
    this.scene.add(mountainBackdrop);
    this.mountainBackdrop = mountainBackdrop;

    const cloudLayer = new THREE.Mesh(
      createCloudLayerGeometry(),
      new THREE.MeshStandardMaterial({
        color: new THREE.Color(0.96, 0.96, 0.94),
        roughness: 0.98,
        metalness: 0,
        emissive: new THREE.Color(0.06, 0.06, 0.06),
        emissiveIntensity: 0.22,
        flatShading: true,
        fog: false
      })
    );
    cloudLayer.receiveShadow = false;
    cloudLayer.castShadow = false;
    this.scene.add(cloudLayer);
    this.cloudLayer = cloudLayer;

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
      const elapsedSeconds =
        (this.sceneTimeMs() / 1000 + DAYLIGHT_START_OFFSET_SECONDS) % 100000;
      if (this.skyTextureSurface && this.skyTexture) {
        paintSkyTexture(this.skyTextureSurface, environment);
        this.skyTexture.needsUpdate = true;
      }
      if (this.terrainTextureSurface && this.terrainTexture) {
        paintTerrainTexture(this.terrainTextureSurface, environment);
        this.terrainTexture.needsUpdate = true;
      }
      if (this.waterTextureSurface && this.waterTexture) {
        paintWaterTexture(this.waterTextureSurface, environment, elapsedSeconds);
        this.waterTexture.needsUpdate = true;
      }
      this.lastEnvironmentSignature = signature;
    }

    const elapsedSeconds =
      (this.sceneTimeMs() / 1000 + DAYLIGHT_START_OFFSET_SECONDS) % 100000;
    this.applyDynamicEnvironment(
      sampleTimeLapseEnvironment(environment, elapsedSeconds).environment,
      elapsedSeconds
    );
  }

  private ensureWaterGeometryMode(mode: "lake" | "river"): void {
    if (this.waterGeometryMode === mode) {
      return;
    }
    const waterGeometry =
      mode === "river" ? this.riverWaterGeometry : this.lakeWaterGeometry;
    const shorelineGeometry =
      mode === "river" ? this.riverShorelineGeometry : this.lakeShorelineGeometry;
    if (!waterGeometry || !shorelineGeometry) {
      return;
    }
    if (this.waterMesh) {
      this.waterMesh.geometry = waterGeometry;
    }
    if (this.shorelineMesh) {
      this.shorelineMesh.geometry = shorelineGeometry;
    }
    this.waterGeometryMode = mode;
  }

  private applyDynamicEnvironment(
    environment: ThreeJsEnvironment,
    elapsedSeconds: number
  ): void {
    this.environmentPreset = describeEnvironmentPreset(environment);
    const waterStyle = sampleWaterSurfaceStyle(environment, elapsedSeconds);
    this.ensureWaterGeometryMode(
      environment.biomeId === "resonant-shore" ? "river" : "lake"
    );

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
      this.waterMesh.material.color.setRGB(
        waterStyle.shallowColor[0],
        waterStyle.shallowColor[1],
        waterStyle.shallowColor[2]
      );
      this.waterMesh.material.emissive.setRGB(
        waterStyle.emissiveColor[0],
        waterStyle.emissiveColor[1],
        waterStyle.emissiveColor[2]
      );
      this.waterMesh.material.emissiveIntensity = waterStyle.emissiveIntensity;
      this.waterMesh.material.opacity = waterStyle.opacity;
    }
    if (this.shorelineMesh) {
      this.shorelineMesh.material.color.setRGB(
        waterStyle.shorelineColor[0],
        waterStyle.shorelineColor[1],
        waterStyle.shorelineColor[2]
      );
      this.shorelineMesh.material.opacity = waterStyle.shorelineOpacity;
      this.shorelineMesh.material.emissive.setRGB(
        waterStyle.shorelineEmissive[0],
        waterStyle.shorelineEmissive[1],
        waterStyle.shorelineEmissive[2]
      );
    }
    if (this.mountainBackdrop) {
      this.mountainBackdrop.material.color.setRGB(1, 1, 1);
      this.mountainBackdrop.material.emissive.setRGB(
        environment.sunColor[0] * 0.04,
        environment.sunColor[1] * 0.035,
        environment.sunColor[2] * 0.025
      );
    }
    if (this.cloudLayer) {
      this.cloudLayer.material.color.setRGB(
        0.92 + environment.sunColor[0] * 0.04,
        0.92 + environment.sunColor[1] * 0.035,
        0.93 + environment.fillColor[2] * 0.025
      );
      this.cloudLayer.material.emissive.setRGB(
        environment.sunColor[0] * 0.06,
        environment.sunColor[1] * 0.05,
        environment.sunColor[2] * 0.04
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
    const now = this.sceneTimeMs();
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
    const activeTransformKeys = new Set<string>();
    const now = this.sceneTimeMs();
    const elapsedSeconds = now / 1000;
    pruneExpiredPulses(this.entityPulseUntilMs, now);
    const resolvedBatches = await resolveVisibleMeshBatchResources(
      batches,
      this.assetRegistry,
      this.quality
    );

    for (const { planned, geometry, material } of resolvedBatches) {
      activeKeys.add(planned.key);
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
        const transformKey = `${planned.key}:mesh:${instance.sourceEntity ?? index}`;
        activeTransformKeys.add(transformKey);
        entry.mesh.setMatrixAt(
          index,
          composeSmoothedInstanceMatrix(
            this.smoothedInstanceTransforms,
            transformKey,
            instance,
            elapsedSeconds,
            pulseStrengthForEntity(this.entityPulseUntilMs, instance.sourceEntity, now),
            now
          )
        );
      }
      entry.mesh.instanceMatrix.needsUpdate = true;
      this.meshEntries.set(planned.key, entry);
    }

    cleanupUnusedEntries(this.scene, this.meshEntries, activeKeys);
    pruneInactiveTransforms(this.smoothedInstanceTransforms, activeTransformKeys, now);
  }

  private async syncSpriteBatches(batches: PlannedSpriteBatch[]): Promise<void> {
    const activeKeys = new Set<string>();
    const activeTransformKeys = new Set<string>();
    const now = this.sceneTimeMs();
    const elapsedSeconds = now / 1000;
    const resolvedBatches = await resolveVisibleSpriteBatchResources(
      batches,
      this.assetRegistry,
      this.quality.anisotropy
    );

    for (const { planned, resolved } of resolvedBatches) {
      activeKeys.add(planned.key);
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
        const transformKey = `${planned.key}:sprite:${instance.sourceEntity ?? index}`;
        activeTransformKeys.add(transformKey);
        entry.mesh.setMatrixAt(
          index,
          composeSmoothedInstanceMatrix(
            this.smoothedInstanceTransforms,
            transformKey,
            instance,
            elapsedSeconds,
            pulseStrengthForEntity(this.entityPulseUntilMs, instance.sourceEntity, now),
            now,
            planned.batch.billboard ? this.camera.quaternion : undefined
          )
        );
      }
      entry.mesh.instanceMatrix.needsUpdate = true;
      this.spriteEntries.set(planned.key, entry);
    }

    cleanupUnusedEntries(this.scene, this.spriteEntries, activeKeys);
    pruneInactiveTransforms(this.smoothedInstanceTransforms, activeTransformKeys, now);
  }

  private async syncAmbientMeshBatches(batches: PlannedMeshBatch[]): Promise<void> {
    const activeKeys = new Set<string>();
    const resolvedBatches = await Promise.all(
      batches
        .filter((planned) => planned.visibleCount > 0)
        .map(async (planned) => ({
          planned,
          geometry: await this.assetRegistry.resolveGeometry(planned.batch, planned.lodLevel)
        }))
    );

    for (const { planned, geometry } of resolvedBatches) {
      activeKeys.add(planned.key);
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
    if (commands.length === 0) {
      return;
    }

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
    const frameStart = monotonicPerfNowMs();
    const sceneTimeSeconds =
      this.sceneTimeMs() / 1000 + DAYLIGHT_START_OFFSET_SECONDS;
    if (this.baseEnvironment) {
      const timeLapse = sampleTimeLapseEnvironment(
        this.baseEnvironment,
        sceneTimeSeconds
      );
      this.timeOfDayHours = timeLapse.timeOfDayHours;
      this.applyDynamicEnvironment(
        timeLapse.environment,
        sceneTimeSeconds
      );
    }
    if (this.waterTexture) {
      const waterStyle = sampleWaterSurfaceStyle(
        this.baseEnvironment ??
          ({
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
          } satisfies ThreeJsEnvironment),
        sceneTimeSeconds
      );
      this.waterTexture.offset.set(
        waterStyle.textureOffset[0],
        waterStyle.textureOffset[1]
      );
      this.waterTexture.repeat.set(
        waterStyle.textureRepeat[0],
        waterStyle.textureRepeat[1]
      );
    }
    const previousAutoClear = this.renderer.autoClear;
    this.renderer.autoClear = true;
    await renderWithFallback(this.renderer, this.scene, this.camera);
    this.renderer.autoClear = false;
    this.renderer.clearDepth();
    await renderWithFallback(this.renderer, this.overlayScene, this.overlayCamera);
    this.renderer.autoClear = previousAutoClear;
    const frameEnd = monotonicPerfNowMs();
    const frameMs = frameEnd - frameStart;
    this.updateAdaptiveResolution(frameMs);
    recordPodThreeRuntimePerfFrame(this.runtimePerf, frameMs, frameEnd);
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
      pendingSpriteAssets: 0,
      geometryLoadsCompleted: 0,
      spriteLoadsCompleted: 0,
      averageGeometryLoadMs: 0,
      averageSpriteLoadMs: 0,
      slowestGeometryLoadMs: 0,
      slowestSpriteLoadMs: 0
    };
    const landscapeMode =
      this.terrainMesh && this.skyDome
        ? LANDSCAPE_PROFILE_ID
        : LANDSCAPE_PROFILE_ID;

    return {
      backend: this.backend,
      renderThread: "main",
      requestedRenderThread: "auto",
      renderThreadFallbackReason: null,
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
      geometryLoadsCompleted: residency.geometryLoadsCompleted,
      spriteLoadsCompleted: residency.spriteLoadsCompleted,
      averageGeometryLoadMs: Number(residency.averageGeometryLoadMs.toFixed(2)),
      averageSpriteLoadMs: Number(residency.averageSpriteLoadMs.toFixed(2)),
      slowestGeometryLoadMs: Number(residency.slowestGeometryLoadMs.toFixed(2)),
      slowestSpriteLoadMs: Number(residency.slowestSpriteLoadMs.toFixed(2)),
      mainThreadPerf: snapshotPodThreeMainThreadPerfStats(
        createPodThreeMainThreadPerfTracker()
      ),
      runtimePerf: snapshotPodThreeRuntimePerfStats(this.runtimePerf),
      ambientInstances: this.ambientInstances,
      visibleWorldChunks: this.visibleWorldChunks,
      preloadedWorldChunks: this.preloadedWorldChunks
    };
  }

  resetPerfMetrics(nowMs = monotonicPerfNowMs()): void {
    resetPodThreeRuntimePerfTracker(this.runtimePerf, nowMs);
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

interface AmbientChunkPlacementSample {
  x: number;
  z: number;
  radialDistance: number;
  lakeMask: number;
  slope: number;
  height: number;
  chunkX: number;
  chunkZ: number;
  slotIndex: number;
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
  densities: Record<PodThreeQualityPreset, number>;
  lakeMaskMax: number;
  minSlope: number;
  maxSlope: number;
  minHeight: number;
  maxHeight: number;
  minRadiusFromOrigin: number;
  regionBias?: (chunkX: number, chunkZ: number) => boolean;
  placementBias?: (sample: AmbientChunkPlacementSample) => number;
}

const AMBIENT_CHUNK_ARCHETYPES: AmbientChunkArchetype[] = [
  {
    id: "canopy-tree",
    mesh: "canopy-tree",
    material: "foliage-canopy",
    tint: [0.22, 0.52, 0.2, 1],
    emissive: [0.015, 0.035, 0.015],
    roughness: 0.96,
    metallic: 0,
    castShadows: true,
    receiveShadows: true,
    renderOrder: 4,
    baseScale: [1.9, 4.4, 1.9],
    densities: {
      ultra: 15,
      high: 11,
      balanced: 7,
      performance: 3
    },
    lakeMaskMax: 0.14,
    minSlope: 0,
    maxSlope: 0.58,
    minHeight: -20,
    maxHeight: 36,
    minRadiusFromOrigin: 10,
    placementBias: ({ x, z, height, slope, lakeMask }) =>
      clamp(
        0.08 +
          sampleValleyFloorMask(x, z) * 0.78 -
          sampleRiverChannelMask(x, z) * 0.34 -
          sampleBackcountryMask(x, z) * 0.14 -
          slope * 0.22 -
          lakeMask * 0.18 +
          clamp((12 - height) / 24, 0, 1) * 0.14,
        0.05,
        0.88
      )
  },
  {
    id: "pine-sapling",
    mesh: "canopy-tree",
    material: "foliage-canopy",
    tint: [0.34, 0.64, 0.28, 1],
    emissive: [0.012, 0.026, 0.01],
    roughness: 0.98,
    metallic: 0,
    castShadows: true,
    receiveShadows: true,
    renderOrder: 3,
    baseScale: [0.62, 1.18, 0.62],
    densities: {
      ultra: 26,
      high: 18,
      balanced: 12,
      performance: 4
    },
    lakeMaskMax: 0.12,
    minSlope: 0,
    maxSlope: 0.48,
    minHeight: -20,
    maxHeight: 40,
    minRadiusFromOrigin: 14,
    placementBias: ({ x, z, height, slope, lakeMask }) =>
      clamp(
        0.16 +
          sampleValleyFloorMask(x, z) * 0.92 -
          sampleRiverChannelMask(x, z) * 0.28 -
          sampleBackcountryMask(x, z) * 0.08 -
          slope * 0.2 -
          lakeMask * 0.14 +
          clamp((16 - height) / 26, 0, 1) * 0.2,
        0.08,
        0.96
      )
  },
  {
    id: "weathered-boulder",
    mesh: "weathered-boulder",
    material: "weathered-stone",
    tint: [0.66, 0.58, 0.46, 1],
    emissive: [0.01, 0.015, 0.02],
    roughness: 1,
    metallic: 0.02,
    castShadows: true,
    receiveShadows: true,
    renderOrder: 5,
    baseScale: [1.2, 1, 1.15],
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
    minRadiusFromOrigin: 12,
    placementBias: ({ x, z, slope }) =>
      clamp(
        0.18 +
          slope * 0.28 +
          sampleValleyFloorMask(x, z) * 0.22 +
          sampleRiverChannelMask(x, z) * 0.18,
        0.12,
        0.78
      )
  },
  {
    id: "basalt-column",
    mesh: "basalt-column",
    material: "basalt",
    tint: [0.72, 0.72, 0.76, 1],
    emissive: [0.015, 0.02, 0.03],
    roughness: 0.9,
    metallic: 0.04,
    castShadows: true,
    receiveShadows: true,
    renderOrder: 5,
    baseScale: [0.82, 1.25, 0.82],
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
    regionBias: (chunkX, chunkZ) => chunkX >= 0 || chunkZ <= -1,
    placementBias: ({ x, z, height, slope }) =>
      clamp(
        0.12 +
          sampleBackcountryMask(x, z) * 0.56 +
          slope * 0.22 +
          clamp((height - 8) / 22, 0, 1) * 0.2,
        0.08,
        0.84
      )
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
      if (archetype.placementBias) {
        const probability = clamp(
          archetype.placementBias({
            x,
            z,
            radialDistance,
            lakeMask,
            slope,
            height,
            chunkX,
            chunkZ,
            slotIndex
          }),
          0,
          1
        );
        if (ambientNoise(chunkX, chunkZ, slotIndex, `${archetype.id}:p`) > probability) {
          continue;
        }
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
      const anchorHeight = meshGroundAnchorHeight(archetype.mesh, scale[1]);

      instances.push({
        archetype,
        instance: {
          position: [x, height + anchorHeight, z],
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

export async function resolveVisibleMeshBatchResources(
  batches: PlannedMeshBatch[],
  assetRegistry: PodThreeAssetRegistry,
  quality: PodThreeQualityProfile
): Promise<ResolvedMeshBatchResources[]> {
  const visibleBatches = batches.filter((planned) => planned.visibleCount > 0);
  return await Promise.all(
    visibleBatches.map(async (planned) => ({
      planned,
      geometry: await assetRegistry.resolveGeometry(planned.batch, planned.lodLevel),
      material:
        (await assetRegistry.resolveMeshMaterial?.(
          planned.batch,
          planned.lodLevel,
          quality
        )) ?? createMeshMaterial(planned.batch, planned.lodLevel, quality)
    }))
  );
}

export async function resolveVisibleSpriteBatchResources(
  batches: PlannedSpriteBatch[],
  assetRegistry: PodThreeAssetRegistry,
  anisotropy: number
): Promise<ResolvedSpriteBatchResources[]> {
  const visibleBatches = batches.filter((planned) => planned.visibleCount > 0);
  return await Promise.all(
    visibleBatches.map(async (planned) => ({
      planned,
      resolved: await assetRegistry.resolveSpriteTexture(
        {
          texture: planned.batch.texture,
          frame: planned.batch.frame
        },
        anisotropy
      )
    }))
  );
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

function composeSmoothedInstanceMatrix(
  transforms: Map<string, SmoothedInstanceTransform>,
  key: string,
  instance: Parameters<typeof sampleAnimatedInstanceTransform>[0],
  elapsedSeconds: number,
  pulse: number,
  now: number,
  rotationOverride?: THREE.Quaternion
): THREE.Matrix4 {
  const sampled = sampleAnimatedInstanceTransform(instance, elapsedSeconds, pulse);
  const targetPosition = new THREE.Vector3(...sampled.position);
  const targetRotation = rotationOverride ?? new THREE.Quaternion(...sampled.rotation);
  const targetScale = new THREE.Vector3(...sampled.scale);
  const previous = transforms.get(key);

  if (
    !previous ||
    now - previous.updatedAt > 260 ||
    previous.position.distanceToSquared(targetPosition) > 64
  ) {
    transforms.set(key, {
      position: targetPosition.clone(),
      rotation: targetRotation.clone(),
      scale: targetScale.clone(),
      updatedAt: now
    });
  } else {
    const deltaSeconds = clamp((now - previous.updatedAt) / 1000, 1 / 240, 0.05);
    const alpha = 1 - Math.exp(-deltaSeconds * 22);
    previous.position.lerp(targetPosition, alpha);
    previous.rotation.slerp(targetRotation, alpha);
    previous.scale.lerp(targetScale, alpha);
    previous.updatedAt = now;
  }

  const resolved = transforms.get(key);
  const matrix = new THREE.Matrix4();
  matrix.compose(
    resolved?.position ?? targetPosition,
    resolved?.rotation ?? targetRotation,
    resolved?.scale ?? targetScale
  );
  return matrix;
}

function pruneInactiveTransforms(
  transforms: Map<string, SmoothedInstanceTransform>,
  activeKeys: Set<string>,
  now: number
): void {
  for (const [key, transform] of transforms) {
    if (activeKeys.has(key) || now - transform.updatedAt <= 360) {
      continue;
    }
    transforms.delete(key);
  }
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
