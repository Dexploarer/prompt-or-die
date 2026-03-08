# pod-web

`pod-web` is the browser-side Three.js client for Prompt or Die.

It consumes the `pod-render` browser frame contract and provides:

- `three/webgpu` rendering with automatic WebGL2 fallback
- manifest-driven glTF mesh loading with `GLTFLoader`, `MeshoptDecoder`, and `KTX2Loader` support
- instanced mesh batches for 3D authored content with distance LOD splitting
- billboard sprite batches with transparent depth-order preservation
- CPU-side frustum and distance culling before instance upload
- adaptive resolution scaling and quality presets for different hardware classes
- ACES tone mapping, tuned shadows, cached materials, and a richer atmospheric scene baseline
- a 2D overlay scene for the legacy `RenderFrame` contract
- a demo bridge via `window.podRender.*` so the app is useful before the Rust wasm entrypoint is wired in
- direct browser websocket connection to the authoritative `pod-server` runtime using `Welcome` / `StateDelta` / `DebugDocument` JSON messages

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
```

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
