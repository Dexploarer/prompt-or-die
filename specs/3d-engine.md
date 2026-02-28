# Spec: 3D Engine Foundation

## Job to Be Done
Extend the existing 2D wgpu renderer to support full 3D rendering — meshes, materials, cameras, lighting — while keeping the 2D path as a first-class mode. Games built on POD can be 2D, 3D, or hybrid.

## Requirements

### 1. 3D Components
- `Transform3D` — position (Vec3), rotation (Quat), scale (Vec3)
- `Mesh` — vertex buffer reference, index buffer, primitive topology
- `Material` — shader reference, textures, uniforms
- `Camera3D` — projection (perspective/orthographic), view matrix, FOV, near/far
- `Light` — point, directional, spot; color, intensity, range
- `MeshRenderer` — ties Mesh + Material to entity

### 2. Render Pipeline (wgpu)
- Forward rendering pipeline with depth buffer
- Shader system (WGSL) — vertex/fragment with uniform binding
- Render pass management — clear, depth test, blending
- Batched draw calls — group by material/mesh
- Frustum culling against camera
- Support both 2D sprite rendering and 3D mesh rendering in same frame

### 3. Asset Formats
- glTF 2.0 import (meshes, materials, animations, scene hierarchy)
- OBJ import (basic mesh)
- Image textures (PNG, JPEG, HDR)
- Custom binary mesh format for fast loading

### 4. Scene Graph Integration
- Parent-child transform hierarchy (world matrix = parent × local)
- pod-scene SceneGraph nodes support both 2D and 3D transforms
- Prefabs can contain 3D hierarchies

### 5. Camera System
- Multiple cameras (game view, editor view, agent POV)
- Camera controller components (orbit, fly, follow)
- Render-to-texture for agent vision simulation

## Success Criteria
- [ ] Render a textured 3D mesh with lighting
- [ ] Camera orbits around scene
- [ ] glTF model loads and displays correctly
- [ ] 2D and 3D content renders in same frame
- [ ] Transform hierarchy works (parent rotation affects children)
- [ ] 60 FPS with 1000 entities
- [ ] `cargo test -p pod-render` passes
