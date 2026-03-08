import { copyFile, mkdir, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import * as THREE from "three";
import { GLTFExporter } from "three/examples/jsm/exporters/GLTFExporter.js";

if (typeof globalThis.FileReader === "undefined") {
  globalThis.FileReader = class FileReader {
    result = null;
    onloadend = null;
    onerror = null;

    async readAsArrayBuffer(blob) {
      try {
        this.result = await blob.arrayBuffer();
        this.onloadend?.();
      } catch (error) {
        this.onerror?.(error);
      }
    }

    async readAsDataURL(blob) {
      try {
        const buffer = Buffer.from(await blob.arrayBuffer());
        const mimeType = blob.type || "application/octet-stream";
        this.result = `data:${mimeType};base64,${buffer.toString("base64")}`;
        this.onloadend?.();
      } catch (error) {
        this.onerror?.(error);
      }
    }
  };
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, "..");
const publicAssetsRoot = join(appRoot, "public", "assets");
const meshesRoot = join(publicAssetsRoot, "meshes");
const texturesRoot = join(publicAssetsRoot, "textures");
const basisRoot = join(publicAssetsRoot, "basis");
const threeRoot = join(appRoot, "node_modules", "three", "examples", "jsm");
const exporterNormalWarning =
  "THREE.GLTFExporter: Creating normalized normal attribute from the non-normalized one.";

await mkdir(meshesRoot, { recursive: true });
await mkdir(texturesRoot, { recursive: true });
await mkdir(basisRoot, { recursive: true });

const originalConsoleWarn = console.warn;
console.warn = (...args) => {
  if (args[0] === exporterNormalWarning) {
    return;
  }
  originalConsoleWarn(...args);
};

const meshDefinitions = {
  "adventurer-avatar": () => new THREE.CapsuleGeometry(0.45, 1.3, 4, 8),
  "adventurer-hero": () => new THREE.CapsuleGeometry(0.48, 1.45, 4, 8),
  "basalt-column": () => new THREE.CylinderGeometry(0.65, 0.9, 3.2, 8),
  "canopy-tree": () => new THREE.ConeGeometry(1.1, 2.4, 6),
  "glass-spire": () => new THREE.ConeGeometry(0.85, 3.0, 5),
  "rift-beast": () => new THREE.OctahedronGeometry(1.1, 0),
  "spirit-companion": () => new THREE.IcosahedronGeometry(0.95, 0),
  "supply-crate": () => new THREE.BoxGeometry(1.4, 1.1, 1.2),
  "weathered-boulder": () => new THREE.DodecahedronGeometry(1.0, 0)
};

for (const [assetId, createGeometry] of Object.entries(meshDefinitions)) {
  const geometry = createGeometry();
  geometry.computeVertexNormals();
  geometry.normalizeNormals();
  const scene = new THREE.Scene();
  const mesh = new THREE.Mesh(
    geometry,
    new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.8, metalness: 0.08 })
  );
  scene.add(mesh);

  const gltf = await exportScene(scene);
  await writeFile(join(meshesRoot, `${assetId}.gltf`), `${JSON.stringify(gltf, null, 2)}\n`);
  geometry.dispose();
}

await writeTexture("danger-ring.svg", ringSvg("#f35b4a", "#ff9e8f"));
await writeTexture("mist-ring.svg", ringSvg("#9fe9ff", "#d7fbff"));
await writeTexture("selection-ring.svg", ringSvg("#5eeec8", "#d7fff2"));

const manifest = {
  version: 1,
  meshes: {
    "adventurer-avatar": {
      path: "/assets/meshes/adventurer-avatar.gltf",
      aliases: ["adventurer", "npc", "player", "traveler"],
      category: "character",
      tags: ["humanoid", "avatar", "npc"]
    },
    "adventurer-hero": {
      path: "/assets/meshes/adventurer-hero.gltf",
      aliases: ["hero", "controlled-player", "main-player"],
      category: "character",
      tags: ["humanoid", "avatar", "hero"]
    },
    "basalt-column": {
      path: "/assets/meshes/basalt-column.gltf",
      aliases: ["column", "pillar", "wall", "obsidian-wall"],
      category: "structure",
      tags: ["basalt", "stone", "structure"]
    },
    "canopy-tree": {
      path: "/assets/meshes/canopy-tree.gltf",
      aliases: ["tree", "pine-tree", "forest-resource"],
      category: "flora",
      tags: ["woodcutting", "forest", "resource"]
    },
    "glass-spire": {
      path: "/assets/meshes/glass-spire.gltf",
      aliases: ["spire", "crystal", "obelisk"],
      category: "structure",
      tags: ["glass", "tower", "magic"]
    },
    "rift-beast": {
      path: "/assets/meshes/rift-beast.gltf",
      aliases: ["monster", "creature", "beast", "wolf"],
      category: "creature",
      tags: ["wild", "combat", "enemy"]
    },
    "spirit-companion": {
      path: "/assets/meshes/spirit-companion.gltf",
      aliases: ["companion", "pet", "summon", "spirit"],
      category: "companion",
      tags: ["ally", "summon", "creature"]
    },
    "supply-crate": {
      path: "/assets/meshes/supply-crate.gltf",
      aliases: ["crate", "cache", "loot", "chest"],
      category: "loot",
      tags: ["container", "supply", "reward"]
    },
    "weathered-boulder": {
      path: "/assets/meshes/weathered-boulder.gltf",
      aliases: ["rock", "boulder", "ore-vein", "resource-stone"],
      category: "resource",
      tags: ["stone", "ore", "resource"]
    }
  },
  sprites: {
    "danger-ring": {
      path: "/assets/textures/danger-ring.svg",
      aliases: ["critical-ring", "hostile-ring"],
      category: "ui",
      tags: ["danger", "warning", "ground-ring"],
      colorSpace: "srgb"
    },
    "mist-ring": {
      path: "/assets/textures/mist-ring.svg",
      aliases: ["fog-ring", "shimmer-ring"],
      category: "effect",
      tags: ["mist", "magic", "atmosphere"],
      colorSpace: "srgb"
    },
    "selection-ring": {
      path: "/assets/textures/selection-ring.svg",
      aliases: ["target-ring", "focus-ring"],
      category: "ui",
      tags: ["selection", "focus", "ground-ring"],
      colorSpace: "srgb"
    }
  }
};

await writeFile(
  join(publicAssetsRoot, "pod-asset-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`
);

await copyFile(
  join(threeRoot, "libs", "basis", "basis_transcoder.js"),
  join(basisRoot, "basis_transcoder.js")
);
await copyFile(
  join(threeRoot, "libs", "basis", "basis_transcoder.wasm"),
  join(basisRoot, "basis_transcoder.wasm")
);

console.warn = originalConsoleWarn;
console.log("Synchronized pod-web sample assets");

function exportScene(scene) {
  return new Promise((resolveExport, rejectExport) => {
    const exporter = new GLTFExporter();
    exporter.parse(
      scene,
      (result) => {
        if (result instanceof ArrayBuffer) {
          rejectExport(new Error("Expected JSON glTF export for pod-web sample assets"));
          return;
        }
        resolveExport(result);
      },
      rejectExport,
      {
        binary: false,
        onlyVisible: true
      }
    );
  });
}

async function writeTexture(fileName, contents) {
  await writeFile(join(texturesRoot, fileName), contents);
}

function ringSvg(innerColor, outerColor) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <defs>
    <radialGradient id="pod-ring" cx="50%" cy="50%" r="50%">
      <stop offset="55%" stop-color="${innerColor}" stop-opacity="0"/>
      <stop offset="72%" stop-color="${innerColor}" stop-opacity="0.92"/>
      <stop offset="88%" stop-color="${outerColor}" stop-opacity="0.45"/>
      <stop offset="100%" stop-color="${outerColor}" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="256" height="256" fill="transparent"/>
  <circle cx="128" cy="128" r="112" fill="url(#pod-ring)"/>
</svg>
`;
}
