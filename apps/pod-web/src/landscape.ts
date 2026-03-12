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

interface TerrainBiomePalette {
  grass: LandscapeVec3Tuple;
  moss: LandscapeVec3Tuple;
  cliff: LandscapeVec3Tuple;
  basalt: LandscapeVec3Tuple;
  highland: LandscapeVec3Tuple;
  sand: LandscapeVec3Tuple;
  foam: LandscapeVec3Tuple;
  accent: LandscapeVec3Tuple;
  accentStrength: number;
  brightnessBias: number;
}

export const LANDSCAPE_PROFILE_ID = "cliff-lagoon-heightfield";
export const WATER_PROFILE_ID = "animated-lagoon";
export const LANDSCAPE_WORLD_SIZE = 260;
export const WATER_LEVEL = -2.35;
export const WATER_CENTER: LandscapeVec2Tuple = [18, -14];
export const WATER_RADII: LandscapeVec2Tuple = [22, 16];
export const DAY_CYCLE_DURATION_SECONDS = 180;
const INFINITE_WORLD_ITERATION_OFFSETS: LandscapeVec2Tuple[] = [
  [-68214.5, 90412.75],
  [23811.25, -51772.5],
  [-14822.25, -83612.5],
  [76122.25, 13844.25],
  [49412.5, -19241.75],
  [-92614.25, 30412.5]
];

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

function distanceToSegment(
  pointX: number,
  pointY: number,
  startX: number,
  startY: number,
  endX: number,
  endY: number
): number {
  const deltaX = endX - startX;
  const deltaY = endY - startY;
  const segmentLengthSquared = deltaX * deltaX + deltaY * deltaY;
  if (segmentLengthSquared <= Number.EPSILON) {
    return Math.hypot(pointX - startX, pointY - startY);
  }

  const projection =
    ((pointX - startX) * deltaX + (pointY - startY) * deltaY) /
    segmentLengthSquared;
  const clampedProjection = clamp(projection, 0, 1);
  const closestX = startX + deltaX * clampedProjection;
  const closestY = startY + deltaY * clampedProjection;
  return Math.hypot(pointX - closestX, pointY - closestY);
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

function ridgedFractalNoise(x: number, y: number): number {
  const centered = fractalNoise(x, y) * 2 - 1;
  return 1 - Math.abs(centered);
}

function sampleInfiniteWorldElevation(x: number, z: number): number {
  let elevation = 0;
  let frequency = 0.003;
  let amplitude = 1;
  let normalisation = 0;

  for (let index = 0; index < INFINITE_WORLD_ITERATION_OFFSETS.length; index += 1) {
    const [offsetX, offsetZ] = INFINITE_WORLD_ITERATION_OFFSETS[index] ?? [0, 0];
    const noise = valueNoise(x * frequency + offsetX, z * frequency + offsetZ) * 2 - 1;
    elevation += noise * amplitude;
    normalisation += amplitude;
    amplitude *= 0.45;
    frequency *= 2.05;
  }

  const normalized = normalisation <= Number.EPSILON ? 0 : elevation / normalisation;
  return Math.pow(Math.abs(normalized), 2) * Math.sign(normalized) * 40 + 1;
}

export function sampleLakeMask(x: number, z: number): number {
  const dx = (x - WATER_CENTER[0]) / WATER_RADII[0];
  const dz = (z - WATER_CENTER[1]) / WATER_RADII[1];
  const radial = clamp(1 - Math.hypot(dx, dz), 0, 1);
  return smoothstep(0, 1, radial);
}

export function sampleValleyFloorMask(x: number, z: number): number {
  const corridor = clamp(1 - distanceToSegment(x, z, 10, 16, 4, -118) / 34, 0, 1);
  const backfield = smoothstep(6, 112, -z);
  const landingFan = clamp(1 - Math.hypot(x - 3, z + 2) / 34, 0, 1);
  return clamp(corridor * (0.34 + backfield * 0.66) + landingFan * 0.18, 0, 1);
}

export function sampleRiverChannelMask(x: number, z: number): number {
  const channel = clamp(1 - distanceToSegment(x, z, 16, 18, 12, -116) / 9.5, 0, 1);
  const backfield = smoothstep(2, 108, -z);
  return clamp(channel * (0.28 + backfield * 0.72), 0, 1);
}

export function sampleBackcountryMask(x: number, z: number): number {
  const backdrop = smoothstep(12, 116, -z);
  const flanks = smoothstep(18, 72, Math.abs(x));
  return clamp(backdrop * 0.72 + flanks * 0.28, 0, 1);
}

export function sampleTerrainHeight(x: number, z: number): number {
  const distance = Math.hypot(x, z);
  const infiniteRanges = sampleInfiniteWorldElevation(x, z);
  const macroMountains = infiniteRanges * 0.72 + (fractalNoise(x * 0.008 - 11, z * 0.008 + 7) - 0.5) * 10;
  const rollingHills = (fractalNoise(x * 0.022 + 13, z * 0.022 - 19) - 0.5) * 6;
  const ridgedRanges =
    ridgedFractalNoise(x * 0.014 - 8, z * 0.014 + 19) * smoothstep(6, 110, -z) * 34;
  const ridgeDetail =
    ridgedFractalNoise(x * 0.028 + 21, z * 0.028 - 17) * smoothstep(18, 118, -z) * 11;
  const centralShelf =
    2.6 * Math.exp(-((x - 2) * (x - 2)) / (2 * 28 * 28) - ((z + 2) * (z + 2)) / (2 * 24 * 24));
  const heroPeak =
    52 *
    Math.exp(-((x - 8) * (x - 8)) / (2 * 18 * 18) - ((z + 112) * (z + 112)) / (2 * 18 * 18));
  const westRange =
    34 *
    Math.exp(-((x + 50) * (x + 50)) / (2 * 20 * 20) - ((z + 84) * (z + 84)) / (2 * 44 * 44));
  const eastRange =
    42 *
    Math.exp(-((x - 46) * (x - 46)) / (2 * 24 * 24) - ((z + 94) * (z + 94)) / (2 * 54 * 54));
  const alpineLift = sampleBackcountryMask(x, z) * 12;
  const overlookShelf =
    8 *
    Math.exp(-((x + 24) * (x + 24)) / (2 * 28 * 28) - ((z - 18) * (z - 18)) / (2 * 20 * 20));
  const westOverlookCliff =
    26 *
    Math.exp(-((x + 46) * (x + 46)) / (2 * 18 * 18) - ((z - 14) * (z - 14)) / (2 * 34 * 34));
  const overlookRidge =
    14 *
    Math.exp(-((x + 12) * (x + 12)) / (2 * 30 * 30) - ((z - 28) * (z - 28)) / (2 * 14 * 14));
  const overlookNotch =
    -8 *
    Math.exp(-((x + 24) * (x + 24)) / (2 * 16 * 16) - ((z - 8) * (z - 8)) / (2 * 10 * 10));
  const hubBaseHeight = macroMountains + rollingHills + centralShelf + overlookShelf - 5.5;
  const hubTerraceBlend = 1 - smoothstep(0, 24, distance);
  const hubTerraceHeight = mixScalar(hubBaseHeight, 1.6, hubTerraceBlend);
  const outerWall = smoothstep(96, 136, distance) * (6 + ridgedRanges * 0.36);
  const cliffBands =
    Math.pow(ridgedFractalNoise(x * 0.018 - 9, z * 0.018 + 23), 1.45) *
    smoothstep(32, 118, distance) *
    12;
  const northRidge =
    22 *
    Math.exp(-((x - 18) * (x - 18)) / (2 * 52 * 52) - ((z + 82) * (z + 82)) / (2 * 18 * 18));
  const westernBluffs =
    11 *
    Math.exp(-((x + 70) * (x + 70)) / (2 * 16 * 16) - ((z + 2) * (z + 2)) / (2 * 52 * 52));
  const valleyFloor = sampleValleyFloorMask(x, z);
  const riverChannel = sampleRiverChannelMask(x, z);
  const riverValley = -7.5 * valleyFloor - 3.5 * riverChannel;
  const lakeMask = sampleLakeMask(x, z);
  const lakeReliefAttenuation = 1 - lakeMask * 0.82;
  const lakeBasin =
    -12.8 * lakeMask - 6.4 * smoothstep(0.42, 1, lakeMask);
  const shorelineShelf =
    -2 * smoothstep(0.08, 0.7, lakeMask) * (1 - smoothstep(0.7, 1, lakeMask));

  return (
    macroMountains * lakeReliefAttenuation +
    rollingHills * lakeReliefAttenuation +
    centralShelf +
    infiniteRanges * 0.28 * lakeReliefAttenuation +
    ridgedRanges +
    ridgeDetail +
    alpineLift +
    outerWall +
    cliffBands +
    northRidge +
    westernBluffs +
    westRange +
    eastRange +
    heroPeak +
    westOverlookCliff +
    overlookRidge +
    overlookNotch +
    riverValley +
    lakeBasin +
    shorelineShelf -
    4 +
    (hubTerraceHeight - hubBaseHeight)
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
  const valleyFloorMask = sampleValleyFloorMask(x, z);
  const riverChannelMask = sampleRiverChannelMask(x, z);
  const backcountryMask = sampleBackcountryMask(x, z);
  const distanceFalloff =
    1 - clamp(Math.hypot(x, z) / (LANDSCAPE_WORLD_SIZE * 0.5), 0, 1);
  const highlandMask = clamp((terrainHeight - 16) / 16, 0, 1);
  const rockMask = clamp(
    cliffMask * 0.7 + highlandMask * 0.8 + ridgeNoise * 0.24,
    0,
    1
  );
  const snowMask = clamp(
    (terrainHeight - 26) / 18 + highlandMask * 0.34 + cliffMask * 0.22 + ridgeNoise * 0.18,
    0,
    1
  );
  const foamMask = clamp(shoreMask * 1.35, 0, 1);
  const palette = terrainPaletteForBiome(environment);
  const sedimentNoise = fractalNoise(x * 0.08 - 13, z * 0.08 + 9);
  const accentNoise = fractalNoise(x * 0.046 + 17, z * 0.046 - 11);
  const isResonantShore = environment.biomeId === "resonant-shore";
  const shorelineLandingMask = isResonantShore
    ? clamp(
        1 -
          distanceToSegment(x, z, 24.4, -43.7, 31.6, -44.8) / 2.8,
        0,
        1
      )
    : 0;
  const monolithApproachMask = isResonantShore
    ? clamp(
        1 -
          distanceToSegment(x, z, 31.6, -44.8, 36.8, -45.4) / 3.2,
        0,
        1
      )
    : 0;
  const campClearingMask = isResonantShore
    ? clamp(1 - Math.hypot(x - 29.6, z + 38.4) / 8.8, 0, 1)
    : 0;
  const lagoonSandMask = isResonantShore
    ? clamp(
        sampleRiverChannelMask(x, z) * 0.42 +
          smoothstep(0.04, 0.32, lake) * (1 - smoothstep(0.72, 1, lake)) * 0.12 +
          shorelineLandingMask * 0.18,
        0,
        1
      )
    : 0;
  const accentMask = clamp(
    rockMask * 0.24 +
      shoreMask * 0.38 +
      highlandMask * 0.08 +
      Math.max(accentNoise - 0.54, 0) * 1.9,
    0,
    1
  );

  let tint = mixVec3(
    palette.grass,
    palette.moss,
    meadowNoise * 0.72 + distanceFalloff * 0.18
  );
  tint = mixVec3(tint, palette.grass, valleyFloorMask * 0.18);
  tint = mixVec3(tint, palette.sand, shoreMask * (0.72 + sedimentNoise * 0.08));
  tint = mixVec3(tint, palette.cliff, cliffMask * 0.7);
  tint = mixVec3(tint, palette.highland, highlandMask * 0.56);
  tint = mixVec3(tint, palette.basalt, rockMask * 0.62);
  tint = mixVec3(tint, palette.accent, accentMask * palette.accentStrength);
  tint = mixVec3(tint, palette.foam, foamMask * 0.16);
  if (isResonantShore) {
    tint = mixVec3(tint, [0.34, 0.54, 0.26], valleyFloorMask * 0.42);
    tint = mixVec3(tint, [0.82, 0.76, 0.58], lagoonSandMask * 0.34);
    tint = mixVec3(
      tint,
      [0.72, 0.66, 0.5],
      clamp(shorelineLandingMask * 0.52 + monolithApproachMask * 0.34, 0, 1)
    );
    tint = mixVec3(tint, [0.52, 0.68, 0.46], campClearingMask * 0.18);
    tint = mixVec3(tint, [0.97, 0.95, 0.9], snowMask * 0.62);
  }

  return {
    tint,
    brightness: clamp(
      0.72 +
        terrainHeight * 0.011 -
        cliffMask * 0.08 +
        meadowNoise * 0.11 +
        foamMask * 0.1 +
        valleyFloorMask * 0.06 +
        snowMask * 0.12 -
        riverChannelMask * 0.03 +
        backcountryMask * 0.02 +
        shorelineLandingMask * 0.08 +
        monolithApproachMask * 0.05 +
        campClearingMask * 0.04 +
        palette.brightnessBias,
      0.32,
      1.2
    ),
    shoreMask,
    cliffMask,
    highlandMask,
    rockMask,
    foamMask
  };
}

function terrainPaletteForBiome(environment: LandscapeEnvironment): TerrainBiomePalette {
  const ground = environment.groundColor.slice(0, 3) as LandscapeVec3Tuple;
  if (environment.biomeId === "resonant-shore") {
    return {
      grass: mixVec3(ground, [0.26, 0.38, 0.2], 0.66),
      moss: [0.48, 0.64, 0.28],
      cliff: [0.74, 0.66, 0.5],
      basalt: [0.54, 0.5, 0.46],
      highland: [0.96, 0.93, 0.84],
      sand: [0.84, 0.76, 0.58],
      foam: [0.98, 0.97, 0.94],
      accent: [0.82, 0.7, 0.48],
      accentStrength: 0.16,
      brightnessBias: 0.22
    };
  }
  if (environment.biomeId === "breaker-shelf") {
    return {
      grass: mixVec3(ground, [0.32, 0.28, 0.18], 0.48),
      moss: [0.46, 0.36, 0.22],
      cliff: [0.52, 0.4, 0.3],
      basalt: [0.28, 0.24, 0.22],
      highland: [0.66, 0.58, 0.46],
      sand: [0.84, 0.72, 0.52],
      foam: [0.96, 0.9, 0.82],
      accent: [0.72, 0.54, 0.32],
      accentStrength: 0.18,
      brightnessBias: 0.04
    };
  }
  if (environment.biomeId === "windward-shelf") {
    return {
      grass: mixVec3(ground, [0.2, 0.34, 0.24], 0.56),
      moss: [0.28, 0.46, 0.28],
      cliff: [0.42, 0.42, 0.38],
      basalt: [0.24, 0.26, 0.3],
      highland: [0.58, 0.58, 0.52],
      sand: [0.74, 0.7, 0.58],
      foam: [0.92, 0.93, 0.9],
      accent: [0.34, 0.6, 0.42],
      accentStrength: 0.16,
      brightnessBias: 0.01
    };
  }

  const grass = mixVec3(ground, [0.2, 0.35, 0.22], 0.45);
  return {
    grass,
    moss: mixVec3(grass, [0.36, 0.49, 0.24], 0.52),
    cliff: mixVec3(grass, [0.38, 0.35, 0.31], 0.78),
    basalt: [0.22, 0.24, 0.28],
    highland: [0.54, 0.52, 0.46],
    sand: [0.72, 0.66, 0.48],
    foam: [0.92, 0.9, 0.82],
    accent: [0.34, 0.48, 0.3],
    accentStrength: 0.1,
    brightnessBias: 0
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
    textureRepeat: [1.04 + daylight * 0.1, 1.08 + waveStrength * 0.08],
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
