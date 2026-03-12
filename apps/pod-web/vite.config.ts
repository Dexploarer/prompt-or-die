import { defineConfig } from "vite";

export default defineConfig({
  optimizeDeps: {
    include: [
      "three/examples/jsm/loaders/GLTFLoader.js",
      "three/examples/jsm/loaders/KTX2Loader.js",
      "three/examples/jsm/libs/meshopt_decoder.module.js",
      "three/examples/jsm/utils/BufferGeometryUtils.js"
    ]
  },
  build: {
    // The renderer intentionally ships large vendor chunks for Three core/WebGPU.
    // Raise the warning threshold above that known envelope after manual splitting.
    chunkSizeWarningLimit: 850,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("three/examples/jsm/loaders/GLTFLoader")) {
            return "three-gltf-loader";
          }
          if (id.includes("three/examples/jsm/loaders/KTX2Loader")) {
            return "three-ktx2-loader";
          }
          if (id.includes("three/examples/jsm/libs/meshopt_decoder")) {
            return "three-meshopt";
          }
          if (id.includes("three/examples/jsm/utils/BufferGeometryUtils")) {
            return "three-buffer-geometry-utils";
          }
          if (id.includes("@toon-format/toon")) {
            return "toon-format";
          }
          if (id.includes("three_webgpu") || id.includes("three/build/three.webgpu")) {
            return "three-webgpu";
          }
          if (id.includes("/node_modules/three/")) {
            return "three-core";
          }
          return undefined;
        }
      }
    }
  },
  server: {
    host: "0.0.0.0",
    port: 4173
  }
});
