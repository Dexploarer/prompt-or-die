// Render shader for pod-render native backend

struct VertexInput {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

@group(0) @binding(0)
var texture_sampler: texture_2d<f32>;
@group(0) @binding(1)
var sampler_: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    // Sample texture if needed, otherwise use color directly
    let tex_color = textureSample(texture_sampler, sampler_, input.uv);
    return input.color * tex_color;
}
