declare module "three/webgpu" {
  import { Camera, Scene, WebGLRenderer } from "three";

  export class WebGPURenderer extends WebGLRenderer {
    init(): Promise<void>;
    renderAsync(scene: Scene, camera: Camera): Promise<void>;
    backend?: { isWebGPUBackend?: boolean };
  }
}
