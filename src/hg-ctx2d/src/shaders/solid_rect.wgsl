@group(0) @binding(0)
var<uniform> canvas_uniform: CanvasUniform;

@group(0) @binding(1)
var<uniform> transform_uniform: TransformUniform;

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
    size_i32: vec2i,
    size_f32: vec2f,
}

struct TransformUniform {
    affine_mat: mat2x2f,
    affine_trans: vec2f,
}

struct Instance {
    @location(0) pos: vec3f,
    @location(1) size: vec2f,
    @location(2) color: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4f,
    @location(1) sdf_pos: vec2f,
    @location(2) sdf_size: vec2f,
}

fn clip_pos_to_raster(pos: vec2f) -> vec2f {
    return (pos + vec2f(1.)) * vec2f(0.5, -0.5) * canvas_uniform.size_f32;
}

fn clip_vec_to_raster(vec: vec2f) -> vec2f {
    return vec * vec2f(0.5, -0.5) * canvas_uniform.size_f32;
}

// Adapted from: https://iquilezles.org/articles/distfunctions2d/
fn sdf_box(pos: vec2f, size: vec2f) -> f32 {
    let n = abs(pos) - size;
    return length(vec2f(max(n.x, 0.), max(n.y, 0.))) + min(max(n.x, n.y), 0.);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    // Determine the position of this vertex in pre-transform coordinates.
    let local_pos = instance.pos.xy + vertices[vertex_index] * instance.size;

    // Transform the local position into a clip-space position.
    let clip_pos_2d = transform_uniform.affine_mat * local_pos + transform_uniform.affine_trans;
    let clip_size_2d = transform_uniform.affine_mat * instance.size;
    let clip_pos = vec4f(clip_pos_2d, instance.pos.z, 1.);

    // Transform the clip position into its raster position.
    let raster_pos = clip_pos_to_raster(clip_pos_2d);
    let raster_size = clip_vec_to_raster(clip_size_2d);

    // Compute SDF coordinates such that, when the SDF is evaluated at a given straight border
    // pixel, it equals one minus the portion of that pixel that has been filled.
    //
    // By convention, our SDF shapes are always centered at `(0,0)`. We must give them a pixel size
    // of `floor(raster_size)` to ensure that pixels on the border of the rectangle are given the
    // appropriate portion.
    //
    // Computing SDF coordinates, then, is as simple as remapping `raster_center` to be the origin
    // of the raster position space.
    let sdf_center = floor(raster_pos + raster_size / 2.);
    let sdf_size = floor(raster_size);
    let sdf_pos = raster_pos - sdf_center;

    return VertexOutput(
        clip_pos,
        instance.color,
        sdf_pos,
        sdf_size,
    );
}

@fragment
fn fs_main(out: VertexOutput) -> @location(0) vec4f {
    let sdf = sdf_box(out.sdf_pos, out.sdf_size);
    var alpha = 1. - clamp(sdf, 0., 1.);

    return vec4f(alpha, 0., 1., 1.);
}
