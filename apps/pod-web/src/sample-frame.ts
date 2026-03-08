import type {
  ThreeJsEnvironment,
  ThreeJsInstance,
  ThreeJsMeshBatch,
  ThreeJsSpriteBatch,
  ThreeJsWebGpuFrame
} from "./contracts";

export function createDemoFrame(seconds: number): ThreeJsWebGpuFrame {
  const orbit = seconds * 0.18;

  return {
    camera: {
      x: Math.sin(seconds * 0.22) * 8,
      y: Math.cos(seconds * 0.18) * 6,
      zoom: 1.05 + Math.sin(seconds * 0.35) * 0.12,
      rotation: orbit,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight
    },
    backgroundColor: [0.05, 0.09, 0.15, 1],
    environment: createDemoEnvironment(),
    overlayCommands: [],
    meshBatches: createMeshBatches(seconds),
    spriteBatches: createSpriteBatches(seconds),
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

function createDemoEnvironment(): ThreeJsEnvironment {
  return {
    biomeId: "demo-rift",
    skyColor: [0.05, 0.09, 0.15, 1],
    fogColor: [0.06, 0.11, 0.16, 1],
    fogNear: 24,
    fogFar: 190,
    ambientColor: [0.64, 0.8, 1],
    ambientIntensity: 1.18,
    sunColor: [1, 0.95, 0.84],
    sunIntensity: 2.8,
    sunDirection: [28, 38, 16],
    fillColor: [0.4, 0.72, 1],
    fillIntensity: 0.78,
    fillDirection: [-20, 12, -8],
    rimColor: [0.33, 0.82, 1],
    rimIntensity: 11.5,
    groundColor: [0.06, 0.1, 0.15, 1],
    starfieldIntensity: 0.82
  };
}

function createMeshBatches(seconds: number): ThreeJsMeshBatch[] {
  const columns = new Array<ThreeJsInstance>();
  for (let x = -4; x <= 4; x += 1) {
    for (let z = -4; z <= 4; z += 1) {
      columns.push({
        position: [x * 6, 2.6 + Math.sin(seconds + x * 0.7 + z * 0.4) * 0.35, z * 6],
        rotation: [0, 0, 0, 1],
        scale: [1, 1 + ((x + z + 8) % 3) * 0.25, 1]
      });
    }
  }

  const glassNear: ThreeJsInstance[] = [
    {
      position: [12, 5.4, 16],
      rotation: [0, 0, 0, 1],
      scale: [1.8, 2.4, 1.8]
    }
  ];
  const glassFar: ThreeJsInstance[] = [
    {
      position: [-14, 5.8, 34],
      rotation: [0, 0, 0, 1],
      scale: [2.1, 2.8, 2.1]
    }
  ];

  return [
    {
      mesh: "basalt-column",
      material: "obsidian",
      layer: 0,
      phase: "opaque",
      sortDepth: 0,
      renderOrder: 0,
      transparent: false,
      doubleSided: false,
      castShadows: true,
      receiveShadows: true,
      tint: [0.19, 0.28, 0.38, 1],
      roughness: 0.92,
      metallic: 0.08,
      emissive: [0.01, 0.02, 0.03],
      depthWrite: true,
      depthTest: true,
      instances: columns
    },
    {
      mesh: "glass-spire",
      material: "aether-glass",
      layer: 1,
      phase: "transparent",
      sortDepth: glassFar[0].position[2],
      renderOrder: 1,
      transparent: true,
      doubleSided: true,
      castShadows: false,
      receiveShadows: true,
      tint: [0.42, 0.78, 1, 0.42],
      roughness: 0.08,
      metallic: 0.12,
      emissive: [0.07, 0.13, 0.19],
      depthWrite: false,
      depthTest: true,
      instances: glassFar
    },
    {
      mesh: "glass-spire",
      material: "aether-glass",
      layer: 1,
      phase: "transparent",
      sortDepth: glassNear[0].position[2],
      renderOrder: 2,
      transparent: true,
      doubleSided: true,
      castShadows: false,
      receiveShadows: true,
      tint: [0.42, 0.78, 1, 0.42],
      roughness: 0.08,
      metallic: 0.12,
      emissive: [0.07, 0.13, 0.19],
      depthWrite: false,
      depthTest: true,
      instances: glassNear
    }
  ];
}

function createSpriteBatches(seconds: number): ThreeJsSpriteBatch[] {
  const shimmerAlpha = 0.34 + Math.sin(seconds * 1.5) * 0.05;
  return [
    {
      texture: "mist-ring",
      frame: 0,
      layer: 2,
      billboard: true,
      phase: "transparent",
      sortDepth: 24,
      renderOrder: 3,
      transparent: true,
      depthWrite: false,
      depthTest: true,
      instances: [
        {
          position: [-8, 3.4, 24],
          rotation: [0, 0, 0, 1],
          scale: [7, 7, 1],
          color: [0.58, 0.78, 1, shimmerAlpha]
        },
        {
          position: [10, 4, 24],
          rotation: [0, 0, 0, 1],
          scale: [6.5, 6.5, 1],
          color: [0.86, 0.96, 1, shimmerAlpha * 0.82]
        }
      ]
    },
    {
      texture: "mist-ring",
      frame: 0,
      layer: 2,
      billboard: true,
      phase: "transparent",
      sortDepth: 10,
      renderOrder: 4,
      transparent: true,
      depthWrite: false,
      depthTest: true,
      instances: [
        {
          position: [0, 2.8, 10],
          rotation: [0, 0, 0, 1],
          scale: [4.5, 4.5, 1],
          color: [0.41, 0.96, 0.82, 0.28]
        }
      ]
    }
  ];
}
