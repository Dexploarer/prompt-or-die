# Asset Pipeline

> Audience: contributors working on staged imports, runtime bundle manifests,
> browser asset delivery, or shipped asset verification.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Reference Bootstrap](./reference-bootstrap.md) ·
> [`apps/pod-web/README.md`](../apps/pod-web/README.md)

This document defines the current authored-asset to shipped-runtime path used
by Prompt or Die.

## Source of truth

The current asset contract lives across these surfaces:

- `crates/pod-assets/examples/stage_import.rs`
- `crates/pod-assets/src/lib.rs`
- `apps/pod-web/scripts/sync-assets.mjs`
- `apps/pod-web/src/assets.ts`

`pod-assets` owns staged imports, runtime bundle assembly, and runtime-public
materialization. `apps/pod-web` consumes that contract for the shipped browser
lane instead of re-owning the filesystem copy rules.

## Quick commands

Stage one asset:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -p pod-assets --example stage_import -- --output-root artifacts/staged-assets path/to/asset.glb
```

Stage assets and materialize the runtime bundle:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -p pod-assets --example stage_import -- --json --materialize-runtime --output-root artifacts/staged-assets --base-dir apps/pod-web --bundle-spec apps/pod-web/artifacts/staged-assets/pod-runtime-bundle-spec.json apps/pod-web/artifacts/source-assets/meshes/adventurer-avatar.glb
```

Regenerate and verify the browser asset lane:

```bash
cd /Users/home/Desktop/prompt-or-die/apps/pod-web
bun run sync:assets
bun run verify:assets
```

## Pipeline shape

| Stage | Owner | Output |
| --- | --- | --- |
| Authored source assets | creators / scripts | `artifacts/source-assets/*` |
| Staged imports | `pod-assets stage_import` | `artifacts/staged-assets/*` plus content-addressed import records |
| Runtime bundle spec | `apps/pod-web/scripts/sync-assets.mjs` | `artifacts/staged-assets/pod-runtime-bundle-spec.json` |
| Runtime bundle manifest | `pod-assets` | `artifacts/staged-assets/pod-staged-asset-manifest.json` |
| Shipped browser assets | `pod-assets` materialization | `apps/pod-web/public/assets/*` plus `pod-asset-manifest.json` |

## Runtime contract

The browser manifest is semantic, not filename-driven:

- meshes and sprites are addressed by stable ids
- mesh entries may expose `lods`, `meshoptLods`, `runtime.variants`, and
  `runtime.compressedVariants`
- sprite entries may expose both `path` and `ktx2Path`
- `runtime.preferredEncoding` records which variant actually wins the current
  budget report

The shipped sample lane currently uses:

- `.glb` as the preferred browser mesh fast path
- optional `.gltf` sidecars as human-inspectable source artifacts
- optional `.ktx2` sprite sidecars for compressed browser texture delivery
- optional `.meshopt.glb` variants for compressed browser mesh delivery

## Budget and selection rules

The sample sync writes
`apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json`.

That report is the selection truth for the shipped sample lane:

- the runtime prefers the smaller valid variant
- ring sprites currently prefer committed `.ktx2` fixtures when they beat the
  raster source
- sample meshes prefer `meshopt` variants when they beat the source `.glb`
- generated mesh LOD outputs must shrink monotonically

## Failure guards

The shared pipeline fails fast when:

- two runtime outputs reuse the same path
- a compressed sprite sidecar is not a staged `.ktx2` import
- a compressed mesh variant does not resolve to a staged `.gltf` or `.glb`
- `stage_import --json` does not return a valid runtime bundle manifest payload
- generated runtime assets drift from the committed browser asset tree

## Browser-side validation surfaces

`apps/pod-web` treats the asset lane as a benchmarked contract, not as an
informal helper script:

- `bun run verify:assets` reruns asset sync and fails on drift
- `bun run test:smoke` exercises the runtime against the shipped asset lane
- `bun run measure:render-routes:check` keeps render-route and asset-load
  regressions visible in artifact form

## When to update this doc

Update this file whenever any of the following change:

- `stage_import` CLI flags or output shape
- runtime bundle manifest fields
- browser selection policy for `ktx2`, `meshopt`, or LOD variants
- the command used to regenerate or verify the shipped browser asset lane
