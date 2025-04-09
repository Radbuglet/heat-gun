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
    vec2f(-1., -1.),  // (1)
    vec2f( 1., -1.),  // (2)
    vec2f( 1.,  1.),  // (3)

    // Triangle 2
    vec2f(-1., -1.),  // (1)
    vec2f( 1.,  1.),  // (3)
    vec2f(-1.,  1.),  // (4)
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
    @location(1) @interpolate(perspective) sdf_pos: vec2f,
    @location(2) @interpolate(flat) sdf_size: vec2f,
}

fn local_pos_to_clip(pos: vec2f) -> vec2f {
    return transform_uniform.affine_mat * pos + transform_uniform.affine_trans;
}

fn local_vec_to_clip(vec: vec2f) -> vec2f {
    return transform_uniform.affine_mat * vec;
}

fn clip_pos_to_raster(pos: vec2f) -> vec2f {
    return (pos + vec2f(1.)) * vec2f(0.5, -0.5) * canvas_uniform.size_f32;
}

fn clip_vec_to_raster(vec: vec2f) -> vec2f {
    return vec * vec2f(0.5, -0.5) * canvas_uniform.size_f32;
}

fn raster_pos_to_clip(pos: vec2f) -> vec2f {
    return pos / vec2f(0.5, -0.5) / canvas_uniform.size_f32 - vec2f(1.);
}

fn raster_vec_to_clip(vec: vec2f) -> vec2f {
    return vec / vec2f(0.5, -0.5) / canvas_uniform.size_f32;
}

fn local_pos_to_raster(pos: vec2f) -> vec2f {
    return clip_pos_to_raster(local_pos_to_clip(pos));
}

fn local_vec_to_raster(vec: vec2f) -> vec2f {
    return clip_vec_to_raster(local_vec_to_clip(vec));
}

struct Basis2 {
    x_hat: vec2f,
    y_hat: vec2f,
}

struct BasisAndVec2 {
    basis: Basis2,
    vec: vec2f,
}

fn normalize_basis_and_vec(val: BasisAndVec2) -> BasisAndVec2 {
    let x_hat_len = length(val.basis.x_hat);
    let y_hat_len = length(val.basis.y_hat);
    let new_basis = Basis2(val.basis.x_hat / x_hat_len, val.basis.y_hat / y_hat_len);
    let new_vec = val.vec * vec2f(x_hat_len, y_hat_len);

    return BasisAndVec2(new_basis, new_vec);
}

fn collapse_basis_and_vec(val: BasisAndVec2) -> vec2f {
    return val.basis.x_hat * val.vec.x + val.basis.y_hat * val.vec.y;
}

// Adapted from: https://iquilezles.org/articles/distfunctions2d/
fn sdf_box(pos: vec2f, size: vec2f) -> f32 {
    let n = abs(pos) - size;
    return length(vec2f(max(n.x, 0.), max(n.y, 0.))) + min(max(n.x, n.y), 0.);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    let uv = vertices[vertex_index];

    // Determine the shape of the quad in local-space.
    let local_center = instance.pos.xy + instance.size / 2.;
    let local_size_vec = abs(instance.size) / 2.;
    let local_size = BasisAndVec2(Basis2(vec2f(1., 0.), vec2f(0., 1.)), local_size_vec);

    // Determine the center of the quad in clip and raster-space.
    let clip_center = local_pos_to_clip(local_center);
    let rast_center = clip_pos_to_raster(clip_center);

    // Determine the size of the quad in raster-space.
    var rast_size = local_size;
    rast_size.basis.x_hat = local_vec_to_raster(rast_size.basis.x_hat);
    rast_size.basis.y_hat = local_vec_to_raster(rast_size.basis.y_hat);
    rast_size = normalize_basis_and_vec(rast_size);

    // Determine a conservative size for the quad in raster-space to help us ensure that all
    // partially-filled pixels are shaded during rasterization.
    var con_rast_size = rast_size;
    con_rast_size.vec += vec2f(2.);

    // Determine the size of the quad in clip-space.
    var con_clip_size = con_rast_size;
    con_clip_size.basis.x_hat = raster_vec_to_clip(con_clip_size.basis.x_hat);
    con_clip_size.basis.y_hat = raster_vec_to_clip(con_clip_size.basis.y_hat);
    con_clip_size = normalize_basis_and_vec(con_clip_size);

    // Determine the conservative clip-space and raster-space positions of the vertex.
    let clip_vertex = clip_center + collapse_basis_and_vec(con_clip_size) * uv;
    let rast_vertex = rast_center + collapse_basis_and_vec(con_rast_size) * uv;

    // Our actual SDF shape inherits our raster size. It's interested in the width and height of the
    // shape, which we can extract from the vector in the basis.
    let sdf_size = rast_size.vec;

    // For the SDF position, we're interested in the SDF-space position of the vertex's center.

    // Finding out this position in raster-space is easy.
    let rast_vertex_center = floor(rast_vertex) + vec2f(0.5);

    // This is a position but our next step is going to need it as a vector w.r.t. the origin of SDF
    // space, which is centered at `rast_center`. Apply the transform.
    let rast_vertex_center_vec = rast_vertex_center - rast_center;

    // Converting raster-space positions to SDF positions is a bit more tricky because of the
    // possibility of rotations. Because `rast_size`'s basis is orthonormal, we can quickly
    // determine SDF-relative coordinates by taking the dot-product of our vector w.r.t the basis
    // vectors.
    let sdf_pos = vec2f(
        dot(rast_vertex_center_vec, rast_size.basis.x_hat),
        dot(rast_vertex_center_vec, rast_size.basis.y_hat),
    );

    // Finally, convert `clip_vertex`—which is a position in 2D clip space—to an NDC position.
    let clip_vertex_ndc = vec4f(clip_vertex, instance.pos.z, 1.);

    return VertexOutput(
        clip_vertex_ndc,
        instance.color,
        sdf_pos,
        sdf_size,
    );
}

@fragment
fn fs_main(out: VertexOutput) -> @location(0) vec4f {
    let sdf = sdf_box(out.sdf_pos, out.sdf_size);
    let alpha = 1. - clamp(sdf, 0., 1.);

    return vec4f(out.color.rgb, out.color.a * alpha);
}
