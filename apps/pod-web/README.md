# pod-web

`pod-web` is the browser-side Three.js client for Prompt or Die.

It consumes the `pod-render` browser frame contract and provides:

- `three/webgpu` rendering with automatic WebGL2 fallback
- manifest-driven glTF mesh loading with `GLTFLoader`, `MeshoptDecoder`, and `KTX2Loader` support
- instanced mesh batches for 3D authored content with distance LOD splitting
- billboard sprite batches with transparent depth-order preservation
- CPU-side frustum and distance culling before instance upload
- adaptive resolution scaling and quality presets for different hardware classes
- parallel mesh/texture prewarming with live asset residency stats in the HUD
- optional `OffscreenCanvas` render-worker path for creator benchmarking and future main-thread relief
- ACES tone mapping, tuned shadows, cached materials, and a richer atmospheric scene baseline
- a 2D overlay scene for the legacy `RenderFrame` contract
- a demo bridge via `window.podRender.*` so the app is useful before the Rust wasm entrypoint is wired in
- direct browser websocket connection to the authoritative `pod-server` runtime using `Welcome` / `StateDelta` / `DebugDocument` JSON messages
- a default local shard sandbox, `Verdant Hollow`, with a safe spawn, NPC hub, combat lane, resources, loot caches, and landmark scenery for browser-first MMO smoke testing

## Run

```bash
cd apps/pod-web
bun install
bun run sync:assets
bun run dev
```

### Connect to a live shard

Start the authoritative server in network mode, then open:

```text
http://127.0.0.1:5173/?server=127.0.0.1:7778&player=WebPlayer&debug=1
```

`server` may be `host:port`, `ws://host:port`, or `wss://host:port`.

### Local AAA test world

With no `?server=` parameter, `pod-web` now boots straight into the local `Verdant Hollow` test biome instead of the old static fallback. This is the default browser smoke target for graphics and interaction work.

Recommended local routes:

```text
http://127.0.0.1:4174/
http://127.0.0.1:4174/?renderThread=worker
```

The local shard includes:

- a safe spawn with nearby NPCs and loot
- multiple wild creatures for combat and capture
- woodcutting and mining nodes
- landmark scenery using the shipped canopy-tree, glass-spire, basalt-column, boulder, crate, companion, creature, and humanoid assets

### Render thread selection

- default: main-thread rendering
- `?renderThread=worker`: transfers the canvas to a dedicated render worker via `OffscreenCanvas`
- `?renderThread=main`: forces the existing main-thread path
- `?backend=webgl2`: forces the WebGL2 renderer path for deterministic browser smoke testing and fallback debugging
- `?backend=webgpu`: forces the WebGPU path when available
- worker mode now mirrors the main thread's logical viewport size and device pixel ratio, so it should not look softer than the main-thread path on high-DPI displays

### Browser controls

- `WASD` or arrow keys: move
- `Tab`: cycle nearby targets
- `Space`: attack current target
- `E`: interact with current target
- `G`: gather from current target
- `R`: loot current target
- `C`: capture current target
- `1`: summon companion slot `0`
- `F`: issue a follow command to companion slot `0`
- `P`: toggle auto-retaliate
- `Enter`: focus chat input and send a shard message
- HUD feedback and event feed rows are driven by authoritative `EventBatch` messages from the shard, so combat/chat/loot/capture outcomes come from server events rather than client-side guesses
- The action status row is driven by authoritative `acknowledged_action_tick` and `Rejected` responses, so creators can see pending, acknowledged, and rejected browser input on the live shard path
- The selected-target summary and suggested-actions row give browser players and creators an immediate interaction hint from the live shard state instead of relying on memorized keybinds alone

## Validate

```bash
cd apps/pod-web
bun run typecheck
bun test
bun run build
bun run test:smoke
```

`bun run test:smoke` boots the local shard at `http://127.0.0.1:4178` and uses Playwright to prove that both:

- `/?backend=webgl2`
- `/?renderThread=worker&backend=webgl2`

can claim gameplay focus and move the controlled entity under automated input.

## Bridge surface

- `window.podRender.render(json)`
  - accepts the legacy `RenderFrame` JSON from `pod-render`
- `window.podRender.renderThreeJsWebGpuFrame(json)`
  - accepts the batched `ThreeJsWebGpuFrame` JSON from `pod-render`
- `window.podRender.resetDemo()`
  - returns the app to its built-in demo scene
- `?server=127.0.0.1:7778&debug=1`
  - connects the browser directly to the authoritative websocket runtime and enables live TOON debug documents

## Asset manifest

`pod-web` now ships a reproducible browser asset pipeline rooted at:

- `/Users/home/Desktop/prompt-or-die/apps/pod-web/public/assets/pod-asset-manifest.json`

The manifest is creator-facing:

- meshes and sprites are addressed by semantic ids instead of hard-coded filenames
- each entry can declare `aliases`, `category`, and `tags` so `monster`, `wolf`, `ore-vein`, `tree`, and similar creator terms resolve intuitively
- sprite entries may declare both `path` and `ktx2Path`; the runtime will prefer KTX2 when the transcoder path is available and fall back to the plain texture otherwise

To regenerate the sample assets and bundled Basis transcoders:

```bash
cd apps/pod-web
bun run sync:assets
```
