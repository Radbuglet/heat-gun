@group(0) @binding(0)
var<uniform> canvas_uniform: CanvasUniform;

const vertices = array(
    // (0, 0)       (1, 0)
    //       1----2
    //       |    |
    //       4----3
    // (0, 1)       (1, 1)

    // Triangle 1
    vec2f(0., 0.),  // (1)
    vec2f(1., 0.),  // (2)
    vec2f(1., 1.),  // (3)

    // Triangle 2
    vec2f(0., 0.),  // (1)
    vec2f(1., 1.),  // (3)
    vec2f(0., 1.),  // (4)
);

struct CanvasUniform {
    affine_mat: mat2x2f,
    affine_trans: vec2f,
}

struct Instance {
    @location(0) pos: vec3f,
    @location(1) size: vec2f,
    @location(3) color: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4f,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    let local_pos = instance.pos.xy + vertices[vertex_index] * instance.size;

    let clip_pos_2d = canvas_uniform.affine_mat * local_pos + canvas_uniform.affine_trans;
    let clip_pos = vec4f(clip_pos_2d, instance.pos.z, 1.);

    return VertexOutput(
        clip_pos,
        instance.color,
    );
}

@fragment
fn fs_main(out: VertexOutput) -> @location(0) vec4f {
    return out.color;
}
