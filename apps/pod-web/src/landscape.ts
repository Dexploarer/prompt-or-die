export type LandscapeVec2Tuple = [number, number];
export type LandscapeVec3Tuple = [number, number, number];
export type LandscapeVec4Tuple = [number, number, number, number];

export interface LandscapeEnvironment {
  biomeId: string;
  skyColor: LandscapeVec4Tuple;
  fogColor: LandscapeVec4Tuple;
  fogNear: number;
  fogFar: number;
  ambientColor: LandscapeVec3Tuple;
  ambientIntensity: number;
  sunColor: LandscapeVec3Tuple;
  sunIntensity: number;
  sunDirection: LandscapeVec3Tuple;
  fillColor: LandscapeVec3Tuple;
  fillIntensity: number;
  fillDirection: LandscapeVec3Tuple;
  rimColor: LandscapeVec3Tuple;
  rimIntensity: number;
  groundColor: LandscapeVec4Tuple;
  starfieldIntensity: number;
}

export interface TimeLapseEnvironmentState {
  cycleT: number;
  timeOfDayHours: number;
  environment: LandscapeEnvironment;
}

export interface LandscapeSurfaceSample {
  terrainHeight: number;
  waterHeight: number | null;
  surfaceHeight: number;
  waterDepth: number;
  lakeMask: number;
  hasWaterSurface: boolean;
  isSwimmable: boolean;
}

export interface TerrainMaterialSample {
  tint: LandscapeVec3Tuple;
  brightness: number;
  shoreMask: number;
  cliffMask: number;
  highlandMask: number;
  rockMask: number;
  foamMask: number;
}

export interface WaterSurfaceStyle {
  shallowColor: LandscapeVec3Tuple;
  deepColor: LandscapeVec3Tuple;
  highlightColor: LandscapeVec3Tuple;
  emissiveColor: LandscapeVec3Tuple;
  emissiveIntensity: number;
  opacity: number;
  shorelineColor: LandscapeVec3Tuple;
  shorelineOpacity: number;
  shorelineEmissive: LandscapeVec3Tuple;
  textureOffset: LandscapeVec2Tuple;
  textureRepeat: LandscapeVec2Tuple;
  waveStrength: number;
}

export const LANDSCAPE_PROFILE_ID = "cliff-lagoon-heightfield";
export const WATER_PROFILE_ID = "animated-lagoon";
export const LANDSCAPE_WORLD_SIZE = 260;
export const WATER_LEVEL = -2.35;
export const WATER_CENTER: LandscapeVec2Tuple = [18, -14];
export const WATER_RADII: LandscapeVec2Tuple = [22, 16];
export const DAY_CYCLE_DURATION_SECONDS = 180;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function mixScalar(left: number, right: number, weight: number): number {
  return left + (right - left) * weight;
}

export function mixVec3(
  left: LandscapeVec3Tuple,
  right: LandscapeVec3Tuple,
  weight: number
): LandscapeVec3Tuple {
  return [
    mixScalar(left[0], right[0], weight),
    mixScalar(left[1], right[1], weight),
    mixScalar(left[2], right[2], weight)
  ];
}

export function smoothstep(edge0: number, edge1: number, value: number): number {
  if (Math.abs(edge1 - edge0) <= Number.EPSILON) {
    return value < edge0 ? 0 : 1;
  }

  const t = clamp((value - edge0) / (edge1 - edge0), 0, 1);
  return t * t * (3 - 2 * t);
}

function hashNoise(x: number, y: number): number {
  const sine = Math.sin(x * 127.1 + y * 311.7) * 43758.5453123;
  return sine - Math.floor(sine);
}

function valueNoise(x: number, y: number): number {
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const x1 = x0 + 1;
  const y1 = y0 + 1;
  const sx = x - x0;
  const sy = y - y0;

  const n00 = hashNoise(x0, y0);
  const n10 = hashNoise(x1, y0);
  const n01 = hashNoise(x0, y1);
  const n11 = hashNoise(x1, y1);
  const ix0 = mixScalar(n00, n10, smoothstep(0, 1, sx));
  const ix1 = mixScalar(n01, n11, smoothstep(0, 1, sx));
  return mixScalar(ix0, ix1, smoothstep(0, 1, sy));
}

export function fractalNoise(x: number, y: number): number {
  let amplitude = 0.5;
  let frequency = 0.08;
  let value = 0;

  for (let octave = 0; octave < 5; octave += 1) {
    value += valueNoise(x * frequency, y * frequency) * amplitude;
    amplitude *= 0.5;
    frequency *= 2;
  }

  return value;
}

export function sampleLakeMask(x: number, z: number): number {
  const dx = (x - WATER_CENTER[0]) / WATER_RADII[0];
  const dz = (z - WATER_CENTER[1]) / WATER_RADII[1];
  const radial = clamp(1 - Math.hypot(dx, dz), 0, 1);
  return smoothstep(0, 1, radial);
}

export function sampleTerrainHeight(x: number, z: number): number {
  const distance = Math.hypot(x, z);
  const macroMountains = (fractalNoise(x * 0.01, z * 0.01) - 0.5) * 20;
  const rollingHills = (fractalNoise(x * 0.028 + 11, z * 0.028 - 17) - 0.5) * 7;
  const centralPlateau = 6.8 * Math.exp(-(distance * distance) / (2 * 34 * 34));
  const hubTerraceBlend = 1 - smoothstep(0, 22, distance);
  const hubTerraceHeight = mixScalar(macroMountains + rollingHills + centralPlateau - 5, 3.8, hubTerraceBlend);
  const outerWall = smoothstep(52, 136, distance) * 24;
  const cliffBands =
    Math.pow(Math.abs(valueNoise(x * 0.018 - 9, z * 0.018 + 23) - 0.5) * 2, 1.7) *
    smoothstep(64, 112, distance) *
    11;
  const northRidge =
    18 *
    Math.exp(-((x - 18) * (x - 18)) / (2 * 52 * 52) - ((z + 82) * (z + 82)) / (2 * 18 * 18));
  const westernBluffs =
    14 *
    Math.exp(-((x + 74) * (x + 74)) / (2 * 16 * 16) - ((z - 8) * (z - 8)) / (2 * 56 * 56));
  const riverValley =
    -3.4 *
    Math.exp(-((x + 10) * (x + 10)) / (2 * 58 * 58) - ((z - 4) * (z - 4)) / (2 * 14 * 14));
  const lakeMask = sampleLakeMask(x, z);
  const lakeBasin = -8.2 * lakeMask;
  const shorelineShelf =
    -2 * smoothstep(0.08, 0.7, lakeMask) * (1 - smoothstep(0.7, 1, lakeMask));

  return (
    macroMountains +
    rollingHills +
    centralPlateau +
    outerWall +
    cliffBands +
    northRidge +
    westernBluffs +
    riverValley +
    lakeBasin +
    shorelineShelf -
    5 +
    (hubTerraceHeight - (macroMountains + rollingHills + centralPlateau - 5))
  );
}

export function sampleTerrainSlope(x: number, z: number): number {
  const step = 1.2;
  const height = sampleTerrainHeight(x, z);
  const dx = sampleTerrainHeight(x + step, z) - height;
  const dz = sampleTerrainHeight(x, z + step) - height;
  return Math.hypot(dx, dz) / step;
}

export function sampleTerrainMaterial(
  environment: LandscapeEnvironment,
  x: number,
  z: number
): TerrainMaterialSample {
  const terrainHeight = sampleTerrainHeight(x, z);
  const slope = sampleTerrainSlope(x, z);
  const lake = sampleLakeMask(x, z);
  const shoreMask =
    lake * (1 - smoothstep(2.4, 8.4, Math.abs(terrainHeight - WATER_LEVEL)));
  const cliffMask = clamp(
    (slope - 0.55) / 1.9 + Math.max(terrainHeight - 10, 0) / 18,
    0,
    1
  );
  const meadowNoise = fractalNoise(x * 0.12 + 8, z * 0.12 - 12);
  const ridgeNoise = fractalNoise(x * 0.032 - 14, z * 0.032 + 21);
  const distanceFalloff =
    1 - clamp(Math.hypot(x, z) / (LANDSCAPE_WORLD_SIZE * 0.5), 0, 1);
  const highlandMask = clamp((terrainHeight - 16) / 16, 0, 1);
  const rockMask = clamp(
    cliffMask * 0.7 + highlandMask * 0.8 + ridgeNoise * 0.24,
    0,
    1
  );
  const foamMask = clamp(shoreMask * 1.35, 0, 1);
  const grass = mixVec3(
    environment.groundColor.slice(0, 3) as LandscapeVec3Tuple,
    [0.2, 0.35, 0.22],
    0.45
  );
  const moss = mixVec3(grass, [0.36, 0.49, 0.24], 0.52);
  const cliff = mixVec3(grass, [0.38, 0.35, 0.31], 0.78);
  const basalt: LandscapeVec3Tuple = [0.22, 0.24, 0.28];
  const highland: LandscapeVec3Tuple = [0.54, 0.52, 0.46];
  const sand: LandscapeVec3Tuple = [0.72, 0.66, 0.48];

  let tint = mixVec3(grass, moss, meadowNoise * 0.75 + distanceFalloff * 0.15);
  tint = mixVec3(tint, sand, shoreMask * 0.7);
  tint = mixVec3(tint, cliff, cliffMask * 0.72);
  tint = mixVec3(tint, highland, highlandMask * 0.52);
  tint = mixVec3(tint, basalt, rockMask * 0.58);
  tint = mixVec3(tint, [0.92, 0.9, 0.82], foamMask * 0.14);

  return {
    tint,
    brightness: clamp(
      0.72 +
        terrainHeight * 0.011 -
        cliffMask * 0.06 +
        meadowNoise * 0.12 +
        foamMask * 0.08,
      0.34,
      1.18
    ),
    shoreMask,
    cliffMask,
    highlandMask,
    rockMask,
    foamMask
  };
}

export function sampleTerrainPoint(
  x: number,
  z: number,
  clearance = 0
): LandscapeVec3Tuple {
  return [x, sampleTerrainHeight(x, z) + clearance, z];
}

export function sampleLandscapeSurface(
  x: number,
  z: number
): LandscapeSurfaceSample {
  const terrainHeight = sampleTerrainHeight(x, z);
  const lakeMask = sampleLakeMask(x, z);
  const waterDepth = Math.max(0, WATER_LEVEL - terrainHeight);
  const hasWaterSurface = lakeMask >= 0.055 && waterDepth >= 0.18;
  const waterHeight = hasWaterSurface ? WATER_LEVEL : null;

  return {
    terrainHeight,
    waterHeight,
    surfaceHeight: waterHeight ?? terrainHeight,
    waterDepth: waterHeight == null ? 0 : waterDepth,
    lakeMask,
    hasWaterSurface,
    isSwimmable: hasWaterSurface && waterDepth >= 0.95
  };
}

export function sampleSurfaceHeight(x: number, z: number): number {
  return sampleLandscapeSurface(x, z).surfaceHeight;
}

export function sampleWaterSurfaceStyle(
  environment: LandscapeEnvironment,
  elapsedSeconds: number
): WaterSurfaceStyle {
  const skyBrightness =
    environment.skyColor[0] * 0.32 +
    environment.skyColor[1] * 0.48 +
    environment.skyColor[2] * 0.2;
  const daylight = clamp((skyBrightness - 0.18) / 0.62, 0, 1);
  const twilight = clamp(1 - Math.abs(daylight - 0.45) / 0.45, 0, 1);
  const waveStrength = 0.48 + daylight * 0.18 + twilight * 0.08;
  const shallowColor = mixVec3(
    [0.16, 0.41, 0.55],
    [0.32, 0.74, 0.82],
    daylight * 0.82 + twilight * 0.12
  );
  const deepColor = mixVec3(
    [0.05, 0.1, 0.18],
    [0.12, 0.28, 0.42],
    daylight * 0.76 + twilight * 0.12
  );
  const highlightColor = mixVec3(
    [0.48, 0.66, 0.78],
    [0.9, 0.97, 1],
    daylight * 0.7 + twilight * 0.22
  );
  const emissiveColor = mixVec3(
    [0.03, 0.06, 0.1],
    [
      0.04 + environment.skyColor[0] * 0.08,
      0.06 + environment.skyColor[1] * 0.1,
      0.09 + environment.skyColor[2] * 0.12
    ],
    daylight * 0.88 + twilight * 0.08
  );
  const shorelineColor = mixVec3(
    [0.58, 0.53, 0.41],
    [
      0.7 + environment.sunColor[0] * 0.08,
      0.64 + environment.sunColor[1] * 0.08,
      0.54 + environment.sunColor[2] * 0.04
    ],
    daylight * 0.85 + twilight * 0.08
  );
  const shorelineEmissive = mixVec3(
    [0.06, 0.06, 0.05],
    [
      0.08 + environment.skyColor[0] * 0.05,
      0.08 + environment.skyColor[1] * 0.05,
      0.07 + environment.skyColor[2] * 0.04
    ],
    daylight * 0.74 + twilight * 0.12
  );
  const textureOffsetX = (((elapsedSeconds * 0.031) % 1) + 1) % 1;
  const textureOffsetY = (((elapsedSeconds * 0.019) % 1) + 1) % 1;

  return {
    shallowColor,
    deepColor,
    highlightColor,
    emissiveColor,
    emissiveIntensity: 0.34 + daylight * 0.14 + twilight * 0.05,
    opacity: 0.76 + daylight * 0.14,
    shorelineColor,
    shorelineOpacity: 0.28 + daylight * 0.12 + twilight * 0.04,
    shorelineEmissive,
    textureOffset: [textureOffsetX, textureOffsetY],
    textureRepeat: [1.55 + daylight * 0.18, 1.42 + waveStrength * 0.16],
    waveStrength
  };
}

export function describeEnvironmentPreset(
  environment: LandscapeEnvironment
): "daylight" | "twilight" | "night" {
  const brightness =
    environment.skyColor[0] * 0.35 +
    environment.skyColor[1] * 0.45 +
    environment.skyColor[2] * 0.2;
  if (brightness >= 0.55 && environment.starfieldIntensity <= 0.18) {
    return "daylight";
  }
  if (brightness >= 0.26) {
    return "twilight";
  }
  return "night";
}

export function sampleTimeLapseEnvironment(
  baseEnvironment: LandscapeEnvironment,
  elapsedSeconds: number
): TimeLapseEnvironmentState {
  const cycleT =
    ((elapsedSeconds / DAY_CYCLE_DURATION_SECONDS) % 1 + 1) % 1;
  const timeOfDayHours = cycleT * 24;
  const sunHeight = Math.sin(cycleT * Math.PI * 2 - Math.PI / 2);
  const daylight = smoothstep(-0.1, 0.22, sunHeight);
  const twilight = 1 - Math.abs(clamp(sunHeight / 0.42, -1, 1));

  const nightSky: LandscapeVec3Tuple = [0.06, 0.09, 0.16];
  const dawnSky: LandscapeVec3Tuple = [0.8, 0.54, 0.4];
  const nightFog: LandscapeVec3Tuple = [0.1, 0.13, 0.18];
  const dawnFog: LandscapeVec3Tuple = [0.77, 0.57, 0.46];
  const nightGround: LandscapeVec3Tuple = [0.08, 0.12, 0.11];
  const nightSun: LandscapeVec3Tuple = [0.54, 0.62, 0.92];

  const daySky = baseEnvironment.skyColor.slice(0, 3) as LandscapeVec3Tuple;
  const dayFog = baseEnvironment.fogColor.slice(0, 3) as LandscapeVec3Tuple;
  const dayGround = baseEnvironment.groundColor.slice(0, 3) as LandscapeVec3Tuple;

  const skyFromNight = mixVec3(nightSky, dawnSky, twilight);
  const fogFromNight = mixVec3(nightFog, dawnFog, twilight);
  const dynamicSky = mixVec3(skyFromNight, daySky, daylight);
  const dynamicFog = mixVec3(fogFromNight, dayFog, daylight);
  const dynamicGround = mixVec3(nightGround, dayGround, daylight * 0.9 + twilight * 0.1);
  const dynamicSun = mixVec3(nightSun, baseEnvironment.sunColor, daylight * 0.92 + twilight * 0.08);

  const azimuth = cycleT * Math.PI * 2 - Math.PI * 0.15;
  const sunDirection: LandscapeVec3Tuple = [
    Math.cos(azimuth) * 58,
    mixScalar(-8, 56, clamp((sunHeight + 1) * 0.5, 0, 1)),
    Math.sin(azimuth) * 42
  ];
  const fillDirection: LandscapeVec3Tuple = [-sunDirection[0] * 0.45, 18, -sunDirection[2] * 0.45];

  return {
    cycleT,
    timeOfDayHours,
    environment: {
      ...baseEnvironment,
      skyColor: [...dynamicSky, 1],
      fogColor: [...dynamicFog, 1],
      ambientColor: mixVec3([0.12, 0.14, 0.19], baseEnvironment.ambientColor, daylight),
      ambientIntensity: mixScalar(0.2, baseEnvironment.ambientIntensity, daylight),
      sunColor: dynamicSun,
      sunIntensity: mixScalar(0.18, baseEnvironment.sunIntensity, daylight * 0.95 + twilight * 0.18),
      sunDirection,
      fillColor: mixVec3([0.16, 0.22, 0.4], baseEnvironment.fillColor, daylight * 0.85 + twilight * 0.2),
      fillIntensity: mixScalar(0.18, baseEnvironment.fillIntensity, daylight * 0.9 + twilight * 0.28),
      fillDirection,
      rimColor: mixVec3([0.22, 0.48, 0.68], baseEnvironment.rimColor, daylight * 0.85 + twilight * 0.12),
      rimIntensity: mixScalar(1.6, baseEnvironment.rimIntensity, daylight * 0.92 + twilight * 0.18),
      groundColor: [...dynamicGround, 1],
      starfieldIntensity: mixScalar(0.72, baseEnvironment.starfieldIntensity, daylight)
    }
  };
}
