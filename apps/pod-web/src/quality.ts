export type PodThreeQualityPreset =
  | "ultra"
  | "high"
  | "balanced"
  | "performance";

export interface PodThreeQualityProfile {
  preset: PodThreeQualityPreset;
  meshCullDistance: number;
  spriteCullDistance: number;
  highDetailDistance: number;
  mediumDetailDistance: number;
  shadowDistance: number;
  shadowMapSize: number;
  anisotropy: number;
  toneMappingExposure: number;
  maxPixelRatio: number;
  minPixelRatio: number;
  adaptiveResolutionStep: number;
  targetFrameMs: number;
  environmentIntensity: number;
  enableAdaptiveResolution: boolean;
  enableShadows: boolean;
  showGrid: boolean;
}

export interface PodThreeQualityInput {
  backend: "webgpu" | "webgl2";
  preferredPreset?: PodThreeQualityPreset;
  hardwareConcurrency?: number;
  deviceMemory?: number;
  devicePixelRatio?: number;
}

const PRESETS: Record<PodThreeQualityPreset, PodThreeQualityProfile> = {
  ultra: {
    preset: "ultra",
    meshCullDistance: 340,
    spriteCullDistance: 240,
    highDetailDistance: 42,
    mediumDetailDistance: 132,
    shadowDistance: 88,
    shadowMapSize: 2048,
    anisotropy: 16,
    toneMappingExposure: 1.14,
    maxPixelRatio: 2,
    minPixelRatio: 1.15,
    adaptiveResolutionStep: 0.08,
    targetFrameMs: 16.7,
    environmentIntensity: 1.15,
    enableAdaptiveResolution: true,
    enableShadows: true,
    showGrid: true
  },
  high: {
    preset: "high",
    meshCullDistance: 300,
    spriteCullDistance: 220,
    highDetailDistance: 34,
    mediumDetailDistance: 110,
    shadowDistance: 72,
    shadowMapSize: 1536,
    anisotropy: 12,
    toneMappingExposure: 1.1,
    maxPixelRatio: 1.75,
    minPixelRatio: 1,
    adaptiveResolutionStep: 0.08,
    targetFrameMs: 16.7,
    environmentIntensity: 1.05,
    enableAdaptiveResolution: true,
    enableShadows: true,
    showGrid: true
  },
  balanced: {
    preset: "balanced",
    meshCullDistance: 240,
    spriteCullDistance: 180,
    highDetailDistance: 28,
    mediumDetailDistance: 92,
    shadowDistance: 54,
    shadowMapSize: 1024,
    anisotropy: 8,
    toneMappingExposure: 1.02,
    maxPixelRatio: 1.35,
    minPixelRatio: 0.85,
    adaptiveResolutionStep: 0.07,
    targetFrameMs: 16.7,
    environmentIntensity: 0.92,
    enableAdaptiveResolution: true,
    enableShadows: true,
    showGrid: false
  },
  performance: {
    preset: "performance",
    meshCullDistance: 180,
    spriteCullDistance: 132,
    highDetailDistance: 18,
    mediumDetailDistance: 60,
    shadowDistance: 32,
    shadowMapSize: 768,
    anisotropy: 4,
    toneMappingExposure: 0.96,
    maxPixelRatio: 1,
    minPixelRatio: 0.72,
    adaptiveResolutionStep: 0.06,
    targetFrameMs: 16.7,
    environmentIntensity: 0.8,
    enableAdaptiveResolution: true,
    enableShadows: false,
    showGrid: false
  }
};

export function resolveQualityProfile(
  input: PodThreeQualityInput
): PodThreeQualityProfile {
  const preset =
    input.preferredPreset ?? pickPreset(input.backend, input.hardwareConcurrency, input.deviceMemory);
  const profile = { ...PRESETS[preset] };
  const devicePixelRatio = input.devicePixelRatio ?? 1;
  profile.maxPixelRatio = Math.min(profile.maxPixelRatio, Math.max(devicePixelRatio, 1));

  return profile;
}

function pickPreset(
  backend: "webgpu" | "webgl2",
  hardwareConcurrency = 4,
  deviceMemory = 4
): PodThreeQualityPreset {
  if (backend === "webgpu") {
    if (hardwareConcurrency >= 8 && deviceMemory >= 8) {
      return "ultra";
    }

    if (hardwareConcurrency >= 6 && deviceMemory >= 4) {
      return "high";
    }

    return "balanced";
  }

  if (hardwareConcurrency >= 8 && deviceMemory >= 8) {
    return "high";
  }

  if (hardwareConcurrency >= 4 && deviceMemory >= 4) {
    return "balanced";
  }

  return "performance";
}
