import { defineConfig } from "vite";

export default defineConfig({
  optimizeDeps: {
    include: [
      "three/examples/jsm/loaders/GLTFLoader.js",
      "three/examples/jsm/loaders/KTX2Loader.js",
      "three/examples/jsm/libs/meshopt_decoder.module.js"
    ]
  },
  build: {
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
          if (id.includes("@toon-format/toon")) {
            return "toon-format";
          }
          if (id.includes("three_webgpu") || id.includes("three/build/three.webgpu")) {
            return "three-webgpu";
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
