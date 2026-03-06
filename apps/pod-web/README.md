# pod-web

`pod-web` is the browser-side Three.js client for Prompt or Die.

It consumes the `pod-render` browser frame contract and provides:

- `three/webgpu` rendering with automatic WebGL2 fallback
- instanced mesh batches for 3D authored content
- billboard sprite batches with transparent depth-order preservation
- a 2D overlay scene for the legacy `RenderFrame` contract
- a demo bridge via `window.podRender.*` so the app is useful before the Rust wasm entrypoint is wired in

## Run

```bash
cd apps/pod-web
bun install
bun run dev
```

## Validate

```bash
cd apps/pod-web
bun run typecheck
bun test
bun run build
```

## Bridge surface

- `window.podRender.render(json)`
  - accepts the legacy `RenderFrame` JSON from `pod-render`
- `window.podRender.renderThreeJsWebGpuFrame(json)`
  - accepts the batched `ThreeJsWebGpuFrame` JSON from `pod-render`
- `window.podRender.resetDemo()`
  - returns the app to its built-in demo scene
