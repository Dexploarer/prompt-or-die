# Spec: Asset Generation & Construction Pipeline

## Job to Be Done
Provide AI-driven and procedural asset generation tools so developers can rapidly create game content — meshes, textures, levels, animations, sound — without requiring a full art team.

## Requirements

### 1. Procedural Generation
- **Terrain** — heightmap generation (noise-based: Perlin, Simplex, Voronoi)
- **Dungeons/Levels** — BSP tree, wave function collapse, cellular automata
- **Meshes** — parametric primitives (box, sphere, cylinder, terrain mesh)
- **Textures** — procedural noise textures, color ramps, normal maps from heightmaps
- **Vegetation** — L-system trees and plants

### 2. AI-Assisted Generation
- **Text-to-Mesh** — integration point for external AI mesh generation APIs
- **Text-to-Texture** — integration point for AI texture generation
- **Text-to-Animation** — integration point for AI motion generation
- **Level Layout** — LLM-driven level design from text descriptions

### 3. Asset Pipeline
- **Import** — glTF, OBJ, PNG, JPEG, WAV, OGG
- **Processing** — mesh optimization (LOD generation, decimation), texture compression, atlas packing
- **Caching** — content-addressed storage for processed assets
- **Hot reload** — detect file changes, reprocess, update runtime

### 4. Construction Tools
- **Tile/Voxel editor** — place tiles/voxels to build levels
- **Prefab assembly** — combine assets into reusable prefabs
- **Material editor** — node graph for combining textures/shaders
- **Animation rigging** — assign animations to entities

### 5. SpacetimeDB Integration
- Asset metadata stored in SpacetimeDB tables
- Asset binary data in content-addressed blob storage
- Multiplayer asset editing (conflict resolution via CRDT or last-write-wins)

## Success Criteria
- [ ] Generate a playable dungeon from a seed
- [ ] Import glTF and render in engine
- [ ] Procedural texture generation works
- [ ] Asset hot-reload detects changes and updates
- [ ] Content-addressed cache avoids redundant processing
- [ ] `cargo test -p pod-assets` passes
