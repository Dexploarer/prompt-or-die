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
- The compact runtime stats line now includes average geometry and sprite load times as `load <geo>/<sprite>ms`; deeper debug surfaces retain the underlying counters plus slowest-load timings for budget tracking

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
- runtime mesh entries can point at either `.glb` or `.gltf`, but `.glb` is the preferred fast path for shipped browser assets
- mesh entries may also declare `lods`, `meshoptLods`, `runtime.variants`, `runtime.preferredEncoding`, and `runtime.compressedVariants`, allowing the loader to pick explicit base/LOD/compressed mesh paths deterministically instead of assuming one implicit runtime file
- sprite entries may declare both `path` and `ktx2Path`; the runtime will prefer KTX2 when the transcoder path is available and fall back to the plain texture otherwise
- the staged runtime bundle contract now accepts optional `.ktx2` source sidecars for sprites plus optional meshopt-compressed glTF/GLB variants for meshes, so app-level manifests can safely project those optimized runtime outputs when creators supply them

The bundled sample pipeline now emits both human-inspectable `.gltf` sidecars and staged `.glb` mesh sources; the manifest points at `.glb` so the shipped path exercises the lower-overhead binary loader by default.

The sample sync now also writes:

- `artifacts/source-assets/`: the generated authoring-side `.glb` / `.gltf` mesh files plus raster `.png` sample textures and any copied `.ktx2` / `.meshopt.glb` fixture sidecars
- `artifacts/staged-assets/`: content-addressed staged imports produced through `cargo run -p pod-assets --example stage_import -- --json --materialize-runtime ...`
- `artifacts/staged-assets/pod-runtime-bundle-spec.json`: the pod-web sample asset set described as a runtime bundle input spec
- `artifacts/staged-assets/pod-staged-asset-manifest.json`: the explicit handoff map emitted by `pod-assets`, linking generated source assets to staged import records and shipped runtime paths

`bun run sync:assets` now leaves sample geometry generation in `scripts/sync-assets.mjs`, but the canonical staged-to-runtime write into `public/assets` is performed by `pod-assets` via the bundle spec instead of direct JS copies.
If future sample or creator assets provide a matching `artifacts/source-assets/textures/<asset-id>.ktx2` sidecar, `scripts/sync-assets.mjs` now adds it to the shared runtime bundle spec automatically. The emitted staged bundle manifest then feeds that sidecar back into the shipped `pod-asset-manifest.json` as `ktx2Path`, so compressed sprite delivery no longer depends on any hand-maintained app-local mapping.
The same sync now emits generated mesh LOD variants (`lod1`, `lod2`) into the shared bundle spec and writes a concrete runtime budget report to `artifacts/staged-assets/pod-runtime-budget-report.json`, so shipped sample meshes now carry explicit size/triangle/load metadata instead of a single opaque runtime path.
It also copies checked-in `.ktx2` ring fixtures from `fixtures/textures/` for the three shipped ring sprites and checked-in `.meshopt.glb` fixtures from `fixtures/meshes/` for the shipped sample meshes, staging both through the same `pod-assets` bundle contract so `ktx2Path` and `meshoptLods` now point at valid runtime files without requiring local texture or mesh compression during `bun run sync:assets`.

Current failure guards:

- `pod-assets` rejects runtime bundle specs that reuse the same runtime output path
- compressed sprite sidecars must resolve to staged `.ktx2` imports, not arbitrary source formats
- compressed mesh variants must resolve to staged `.gltf` / `.glb` imports, not arbitrary binary blobs
- `bun run sync:assets` fails immediately if `stage_import --json` does not return a valid bundle manifest payload
- runtime budget enforcement fails if generated mesh LOD outputs do not shrink monotonically or exceed the declared per-category shipping budgets

Current selection policy:

- sprite descriptors expose both `path` and `ktx2Path` when a real `.ktx2` fixture exists
- `runtime.preferredEncoding` is set from the generated budget report, so the browser now prefers the smaller committed `.ktx2` ring fixtures over the generated PNG sources on the shipped ring sprites
- mesh descriptors expose base `lods` plus `meshoptLods` when a real compressed mesh fixture exists
- `runtime.preferredEncoding` is also set for meshes, so the browser now prefers the smaller committed `.meshopt.glb` fixtures over the source `.glb` files on shipped sample meshes where the compressed asset wins the budget report

Current runtime perf surface:

- `window.podRender.getStats().runtimePerf` reports `warmupMs`, frame budget, rendered-frame count, stable/slow frame counts, stable-frame percentage, and slowest frame time
- `window.podRender.getStats().mainThreadPerf` reports `warmupMs`, submission count, average submission time, and slowest submission time so worker routes can be compared as actual main-thread relief instead of render-thread timing alone
- `window.podRender.getStats().mainThreadPerf.byKind` now breaks that submission traffic into `frame`, `control`, and `resize` buckets for worker-route attribution
- worker render routes now post only the newest pending frame while a prior worker render is still in flight, batch same-turn telemetry/world-event updates into one combined control message, and no longer send a duplicate `resize` message immediately after `init` when the surface metrics have not changed
- the local-sandbox worker smoke route now treats `mainThreadPerf.byKind.control === 0` and `resize === 0` as regression ceilings, so worker-route chatter increases fail in browser CI instead of staying observational
- the local-sandbox smoke route now also enforces explicit frame-stability floors through `runtimePerf`, requiring the main-thread route to hold at least `90%` stable frames and the worker route to hold at least `50%` stable frames with more stable than slow frames
- `window.podRender.getStats()` also reports `requestedRenderThread` and `renderThreadFallbackReason`, so worker fallback behavior is inspectable when OffscreenCanvas, worker construction, or canvas transfer support is missing
- `tests/worker-input.e2e.ts` now asserts both `runtimePerf` and `mainThreadPerf` on main-thread and worker routes after deterministic local-sandbox movement, so render-thread and submission-path regressions surface as data instead of only pass/fail input checks
- `bun run measure:render-routes` now emits `artifacts/render-route-measurements.json` with main-vs-worker `local-sandbox` measurements and gate results, and the root moat benchmark command includes the same payload under `browserRouteMeasurements`

Current transport debug surface:

- authoritative `shard_transport_summary` documents now include full snapshot count/bytes, recovery snapshot bytes, delta message count/bytes, delta entity churn, peak queue depth, and per-client queue-pressure incident counts
- `formatConnectionSummary()` intentionally stays compact for the gameplay HUD
- `formatTransportDebugSummary()` powers the debug-side transport rollup in the telemetry/incident panel, where the richer networking counters can be inspected without bloating the main HUD
- `src/direct-connect.test.ts` now includes the degraded-path regression gate that forces reconnect, not recovery, when the action backlog saturates under stale authority

To regenerate the sample assets and bundled Basis transcoders:

```bash
cd apps/pod-web
bun run sync:assets
```
