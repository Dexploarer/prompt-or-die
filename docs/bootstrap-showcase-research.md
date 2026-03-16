# Bootstrap Showcase Research

This brief exists to answer one repo-specific question:

`What do we need to ship so the first Prompt or Die world looks intentional, distinctive, and technically credible instead of like a systems sandbox?`

The target is not a generic "better scene". The target is a production-worthy bootstrap route for `apps/pod-web` that can serve as:

- the canonical first-world experience
- the benchmarked creator bootstrap path
- the visual reference for future starter worlds

> Audience: contributors shaping the first-world browser experience and the
> creator bootstrap route.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Reference Bootstrap](./reference-bootstrap.md) ·
> [Benchmark Suite](./benchmark-suite.md)

## Scope

This research is focused on the actual POD stack:

- `three` `r182`
- `WebGPURenderer` with WebGL2 fallback
- manifest-driven glTF loading
- instanced mesh batching
- current low-poly/stylized asset language
- browser-first MMO sandbox constraints

## Executive summary

- The current bootstrap should evolve into a dedicated showcase route, not keep accumulating polish inside the generic local test biome.
- The right visual lane for POD right now is stylized low-poly PBR with deliberate silhouettes, strong camera composition, controlled toon treatment, and a curated landmark set. It is a better fit for the existing runtime than chasing photorealism.
- The official `three.js` stack already supports nearly everything needed for a stronger bootstrap: glTF + meshopt + KTX2, PMREM-based environment lighting, WebGPU post-processing, toon materials, outline passes, batching, and shader precompilation.
- The academic camera literature consistently points the same way: use constrained or cinematography-aware framing, obstruction handling, and event-driven camera changes. The current sandbox camera is usable, but not yet authored as a first-impression system.
- The NPR literature reinforces a key product decision: the bootstrap should emphasize abstraction, salient forms, mood, and readable motion over realism. That aligns with POD's current art budget and runtime constraints.

## Source-backed findings

### 1. Asset delivery should stay glTF-centered, but the bootstrap pack needs a real authored kit

High confidence.

Official sources:

- [GLTFLoader](https://threejs.org/docs/pages/GLTFLoader.html)
- [GLTFExporter](https://threejs.org/docs/pages/GLTFExporter.html)
- [glTF Registry](https://registry.khronos.org/glTF/)
- [Physically Based Rendering in glTF](https://www.khronos.org/gltf/pbr)

Relevant takeaways:

- `GLTFLoader` supports the extensions POD actually cares about for shipping browser assets: `KHR_texture_basisu`, `EXT_meshopt_compression`, and `EXT_mesh_gpu_instancing`.
- `GLTFExporter` supports `KHR_mesh_quantization` and `EXT_mesh_gpu_instancing`, which means generated or kitbashed bootstrap assets can still stay on the glTF path instead of becoming one-off runtime meshes forever.
- Khronos positions glTF 2.x as the runtime delivery format for portable PBR materials, which matches POD's need to keep browser and native presentation aligned.

Implication for POD:

- Keep the bootstrap route on a glTF manifest path.
- Stop thinking of the asset pack as "sample meshes".
- Promote it into a curated bootstrap kit:
  - hero
  - 2 NPC archetypes
  - 2 hostile archetypes
  - 1 companion
  - 3 landmark structures
  - 3 foliage/rock sets
  - 2 camp props
  - 2 effect sprites

### 2. Texture compression and mesh compression are worth baking in from the start

High confidence.

Official sources:

- [KTX2Loader](https://threejs.org/docs/pages/KTX2Loader.html)
- [Basis Universal GPU Texture Compression](https://threejs.org/examples/jsm/libs/basis/)
- [GLTFLoader](https://threejs.org/docs/pages/GLTFLoader.html)
- [three.js WebGPU glTF compressed example](https://threejs.org/examples/webgpu_loader_gltf_compressed)

Relevant takeaways:

- `KTX2Loader` requires `detectSupport(renderer)` before loading textures and relies on the Basis transcoder.
- `GLTFLoader` can use both `setKTX2Loader(...)` and `setMeshoptDecoder(...)`, which POD already mirrors in its runtime loaders.

Implication for POD:

- Final bootstrap assets should move from "small JSON glTFs with embedded data" to a compressed authored pack:
  - `.glb` or external-buffer `.gltf`
  - `meshopt` for geometry
  - `KTX2` for color/normal/mask textures where textures are used
- This should be part of the bootstrap contract from the start, not a later optimization pass.

### 3. The scene should use PMREM-backed lighting and a small number of strong materials, not many weak ones

High confidence.

Official sources:

- [PMREMGenerator](https://threejs.org/docs/pages/PMREMGenerator.html)
- [ColorEnvironment](https://threejs.org/docs/pages/ColorEnvironment.html)
- [three.js WebGPU PMREM scene example](https://threejs.org/examples/webgpu_pmrem_scene.html)
- [three.js WebGPU equirectangular example](https://threejs.org/examples/webgpu_equirectangular.html)

Relevant takeaways:

- `PMREMGenerator.fromScene(...)` is explicitly supported and can be faster than image-based environment loading when bandwidth is low.
- PMREM is the intended path for stable roughness-aware PBR lighting.

Implication for POD:

- The bootstrap showcase should have an authored lighting rig, not only directional/fill colors.
- Build one procedural environment per showcase biome:
  - dawn glass-shard coast
  - noon verdant camp
  - dusk ruin basin
- Feed that through PMREM so a small material set can still feel rich.
- Keep the number of material families intentionally low:
  - hero cloth/leather
  - painted stone
  - moss/foliage
  - polished crystal/glass
  - brass/utility metal

### 4. Stylization should be explicit, not accidental

High confidence.

Official sources:

- [MeshToonMaterial](https://threejs.org/docs/pages/MeshToonMaterial.html)
- [ToonLightingModel](https://threejs.org/docs/pages/ToonLightingModel.html)
- [ToonOutlinePassNode](https://threejs.org/docs/pages/ToonOutlinePassNode.html)
- [three.js WebGPU toon material example](https://threejs.org/examples/webgpu_materials_toon.html)

Research sources:

- [MNPR: A Framework for Real-Time Expressive Non-Photorealistic Rendering of 3D Computer Graphics](https://diglib.eg.org/items/908efe9f-fb6a-4d3e-98b6-bda5d952c71e)
- [State of the Art Non-Photorealistic Rendering (NPR) Techniques](https://diglib.eg.org/items/7ba3e4eb-b58d-477a-a8dc-70bfbece3448)

Relevant takeaways:

- three.js already provides the toon shading and outline infrastructure needed for a selective NPR pass.
- The NPR papers consistently argue for stylization systems with controllable abstraction and salient feature emphasis, not realism-at-all-costs.

Implication for POD:

- The bootstrap route should adopt a clear stylization policy:
  - toon or semi-toon for environment solids
  - standard/PBR for crystal, water, and hero accent materials
  - selective outline on hero, selected NPCs, interactables, and landmark silhouettes
- Do not apply outlines globally.
- The goal is "readable and authored", not "comic filter everywhere".

### 5. Fog and background need to be art-direction tools, not leftovers

High confidence.

Official sources:

- [Fog](https://threejs.org/docs/pages/Fog.html)
- [three.js fog manual](https://threejs.org/manual/en/fog.html)
- [three.js WebGPU custom fog example](https://threejs.org/examples/webgpu_custom_fog.html)
- [three.js WebGPU custom fog background example](https://threejs.org/examples/webgpu_custom_fog_background.html)

Relevant takeaways:

- three.js documentation is explicit that fog and background should be coordinated if the scene is meant to fade intentionally.
- Custom fog in WebGPU exists and can be part of the authored look.

Implication for POD:

- The bootstrap showcase should own its depth read:
  - background color
  - fog ramp
  - horizon silhouette
  - water/atmosphere transition
- This is especially important because POD's terrain is broad and stylized; poor fog/background tuning makes it look empty immediately.

### 6. Post-processing is available in WebGPU and should be used selectively

High confidence.

Official sources:

- [WebGPURenderer manual](https://threejs.org/manual/en/webgpurenderer)
- [PostProcessing](https://threejs.org/docs/pages/PostProcessing.html)
- [RenderOutputNode](https://threejs.org/docs/pages/RenderOutputNode.html)

Relevant takeaways:

- `WebGPURenderer` has a modern node-based post stack.
- `PostProcessing` and `RenderOutputNode` make it possible to control where tone mapping and color conversion happen.

Implication for POD:

- The showcase route should use a minimal, authored post stack:
  - output transform
  - selective outline
  - very light bloom or emissive lift on glass/attunement materials
  - optional subtle vignette
- Avoid broad blur, heavy bloom, or film grain until the scene art is already strong.

### 7. The first impression should be camera-directed, not just player-following

High confidence on direction, medium confidence on exact implementation shape.

Research sources:

- [Through-the-Lens Camera Control](https://graphics.cs.wisc.edu/Papers/1992/GW92/)
- [Camera Control through Cinematography in 3D Computer Games](https://arrow.tudublin.ie/itbj/vol5/iss1/14)
- [Cinematic Camera Control in 3D Computer Games](https://www.sciweavers.org/publications/cinematic-camera-control-3d-computer-games)

Relevant takeaways:

- Camera systems become better when they are constraint-aware, image-aware, and cinematography-aware rather than only physically trailing the avatar.
- The game-camera literature repeatedly emphasizes framing important subjects, avoiding obstruction, and using event-driven changes rather than one static mode.

Implication for POD:

- The showcase bootstrap should add authored camera states:
  - `arrival`
  - `free movement`
  - `targeted interaction`
  - `combat pressure`
  - `landmark reveal`
- The player can still own the camera, but the default opening state should be staged intentionally.
- The very first frame should already have:
  - readable hero silhouette
  - one NPC
  - one landmark
  - one environmental layer behind them

### 8. Merge/batch aggressively, but only after the authored composition is correct

High confidence.

Official sources:

- [BufferGeometryUtils](https://threejs.org/docs/pages/module-BufferGeometryUtils.html)
- [Optimize Lots of Objects](https://threejs.org/manual/en/optimize-lots-of-objects.html)
- [three.js instancing performance example](https://threejs.org/examples/webgl_instancing_performance.html)
- [three.js WebGPU instance mesh example](https://threejs.org/examples/webgpu_instance_mesh.html)
- [SceneOptimizer](https://threejs.org/docs/pages/SceneOptimizer.html)

Relevant takeaways:

- `mergeGeometries(...)` requires compatible attributes.
- The three.js optimization guidance is clear: reduce scene graph churn, merge where static, instance where repeated.
- `Renderer.compileAsync(...)` exists specifically to reduce first-use shader stutter.

Implication for POD:

- For the bootstrap route:
  - merge static camp clusters into authored landmark meshes where possible
  - keep repeated foliage/stone on instanced paths
- use `compileAsync(...)` during bootstrap warmup to remove shader-pop on first camera movement
- Optimize after the visual composition is locked, not before.

## Example-to-repo translation map

This is the critical bridge between the external research and the code we already have.

### Asset loading and compression

- `GLTFLoader`, `KTX2Loader`, `MeshoptDecoder`, and the compressed glTF example map directly onto [assets.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/assets.ts), which already calls `setMeshoptDecoder(...)` and `setKTX2Loader(...)`.
- The missing piece is not runtime capability. The missing piece is that [sync-assets.mjs](/Users/home/Desktop/prompt-or-die/apps/pod-web/scripts/sync-assets.mjs) still emits JSON glTF sample assets instead of a compressed authored showcase pack.

### PMREM and environment lighting

- The PMREM docs/examples map onto [renderer.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/renderer.ts) and [landscape.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/landscape.ts).
- POD already has a strong fog/background/environment path. The next showcase pass should add an authored environment rig instead of only tuning the existing directional and fog values.

### Toon materials and selective outlines

- The toon material example maps directly onto [assets.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/assets.ts), which already uses `MeshToonMaterial` for stylized batches.
- The outline/post examples imply a showcase-only render path layered into [renderer.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/renderer.ts), not a global runtime filter over every route.

### Fog/background coupling

- The fog docs and custom fog examples map onto [landscape.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/landscape.ts), where POD already derives time-of-day fog colors, and [renderer.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/renderer.ts), where those values are applied to the scene and water presentation.
- This means the showcase route can get a materially better depth read without inventing a new world renderer.

### Camera research and authored first frame

- The camera-control papers do not imply a brand-new camera subsystem. They imply a state machine layered over the current gameplay controller in [main.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/main.ts).
- The key missing capability is not orbit/follow. It is authored framing states for arrival, reveal, focus, and combat pressure.

### Warmup and scene optimization

- `Renderer.compileAsync(...)` and the optimization docs map onto [renderer.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/renderer.ts), which is where a showcase warmup pass should precompile the final landmark/hero material set before the player first rotates the camera.
- Instancing is already part of POD's scene path. The next change is to separate what should remain instanced from what should become merged authored landmark clusters.

## Repo-specific blueprint

### A. Split bootstrap from sandbox

Recommendation:

- Keep the current local sandbox for systems and smoke validation.
- Add a dedicated `bootstrap-showcase` route that is allowed to be more authored and more directed.

Why:

- A benchmark route and a first-impression route can overlap, but they should not be forced to be the same thing if that makes both worse.
- The current route has too many test-world responsibilities.

### B. Build one signature vista

Recommendation:

- The opening camera should land on one curated composition, not a freeform test camp.

The opening frame should include:

- hero in foreground or lower-third
- one strong NPC silhouette
- one dominant landmark
- one secondary prop cluster
- one color-separating environmental layer

### C. Use a finite bootstrap asset kit

Recommendation:

- Cap the bootstrap kit intentionally.

Suggested first pack:

- 2 humanoids
- 2 creatures
- 1 companion
- 3 trees/flora variants
- 3 rock/column variants
- 2 structural landmarks
- 2 camp props
- 3 effect sprites

This is enough to look authored without turning the bootstrap into a content-production sink.

### D. Lock a material language

Recommendation:

- Stop letting every object live in the same pale value band.

Suggested families:

- `hero` = warm cloth + brass accents
- `npc` = cooler field cloth
- `landmark crystal` = cyan/teal emissive glass
- `stone` = blue-gray basalt
- `foliage` = muted mint/olive
- `camp props` = warm wood/bronze

### E. Add a showcase camera state machine

Recommendation:

- Layer camera states over the current third-person controller instead of replacing it.

Suggested states:

- `intro_hold`
- `player_controlled`
- `focus_target`
- `reveal_landmark`
- `combat_tight`

### F. Use selective post only

Recommendation:

- For the showcase route, use:
  - outline on hero / interactables / landmark
  - restrained emissive bloom for crystal
  - controlled output transform

Avoid:

- heavy bloom
- full-screen noise
- global outlines
- aggressive chromatic tricks

## Suggested implementation order

1. Create `bootstrap-showcase` route and keep `local-sandbox` intact.
2. Build the curated opening vista and spawn choreography.
3. Replace the rest of the bootstrap pack with final low-poly authored assets.
4. Add PMREM-backed environment and tightened fog/background coupling.
5. Add showcase camera state machine.
6. Add selective outline + restrained post stack on WebGPU.
7. Compress the asset pack with meshopt + KTX2.
8. Add benchmark + screenshot diff checks for the showcase route.

## What this means for POD positioning

The bootstrap route should not try to prove "we are already a AAA art pipeline".

It should prove something more believable and more useful:

- POD has a distinct authored visual voice.
- POD can present a browser-first agent world that already feels like a game, not a graphics test.
- POD can turn a small curated content pack into a coherent first-world experience.

That is enough to be strategic leverage. It is not enough to stop there.

## Open questions

- Do we want the official first-world route to be fully controllable from frame zero, or should it spend 2-4 seconds in a lightly directed attract-state before yielding control?
- Should the bootstrap asset pack stay procedurally/generated-in-repo, or should it become a true authored art pack under version control?
- Do we want the showcase route to remain benchmarked by default, or should the benchmark continue to target the systems sandbox while the showcase gets its own screenshot-based gate?

## Source list

Official docs and examples:

- [WebGPURenderer manual](https://threejs.org/manual/en/webgpurenderer)
- [Renderer](https://threejs.org/docs/pages/Renderer.html)
- [GLTFLoader](https://threejs.org/docs/pages/GLTFLoader.html)
- [GLTFExporter](https://threejs.org/docs/pages/GLTFExporter.html)
- [KTX2Loader](https://threejs.org/docs/pages/KTX2Loader.html)
- [Basis Universal GPU Texture Compression](https://threejs.org/examples/jsm/libs/basis/)
- [PMREMGenerator](https://threejs.org/docs/pages/PMREMGenerator.html)
- [ColorEnvironment](https://threejs.org/docs/pages/ColorEnvironment.html)
- [Fog](https://threejs.org/docs/pages/Fog.html)
- [three.js fog manual](https://threejs.org/manual/en/fog.html)
- [MeshToonMaterial](https://threejs.org/docs/pages/MeshToonMaterial.html)
- [ToonLightingModel](https://threejs.org/docs/pages/ToonLightingModel.html)
- [ToonOutlinePassNode](https://threejs.org/docs/pages/ToonOutlinePassNode.html)
- [PostProcessing](https://threejs.org/docs/pages/PostProcessing.html)
- [RenderOutputNode](https://threejs.org/docs/pages/RenderOutputNode.html)
- [BufferGeometryUtils](https://threejs.org/docs/pages/module-BufferGeometryUtils.html)
- [Optimize Lots of Objects](https://threejs.org/manual/en/optimize-lots-of-objects.html)
- [SceneOptimizer](https://threejs.org/docs/pages/SceneOptimizer.html)
- [three.js WebGPU toon material example](https://threejs.org/examples/webgpu_materials_toon.html)
- [three.js WebGPU PMREM scene example](https://threejs.org/examples/webgpu_pmrem_scene.html)
- [three.js WebGPU custom fog example](https://threejs.org/examples/webgpu_custom_fog.html)
- [three.js WebGPU custom fog background example](https://threejs.org/examples/webgpu_custom_fog_background.html)
- [three.js WebGPU glTF compressed example](https://threejs.org/examples/webgpu_loader_gltf_compressed)
- [three.js WebGPU KTX2 texture example](https://threejs.org/examples/webgpu_loader_texture_ktx2.html)

Specifications and standards:

- [glTF Registry](https://registry.khronos.org/glTF/)
- [Physically Based Rendering in glTF](https://www.khronos.org/gltf/pbr)

Research papers:

- [Through-the-Lens Camera Control](https://graphics.cs.wisc.edu/Papers/1992/GW92/)
- [Camera Control through Cinematography in 3D Computer Games](https://arrow.tudublin.ie/itbj/vol5/iss1/14)
- [Cinematic Camera Control in 3D Computer Games](https://www.sciweavers.org/publications/cinematic-camera-control-3d-computer-games)
- [MNPR: A Framework for Real-Time Expressive Non-Photorealistic Rendering of 3D Computer Graphics](https://diglib.eg.org/items/908efe9f-fb6a-4d3e-98b6-bda5d952c71e)
- [State of the Art Non-Photorealistic Rendering (NPR) Techniques](https://diglib.eg.org/items/7ba3e4eb-b58d-477a-a8dc-70bfbece3448)
