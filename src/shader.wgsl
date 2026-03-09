struct Uniforms {
    time: f32,
    aspect_ratio: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(in_vertex_index & 1u) << 2u) - 1.0;
    let y = f32(i32(in_vertex_index & 2u) << 1u) - 1.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

fn rot_x(p: vec3<f32>, a: f32) -> vec3<f32> {
    let s = sin(a);
    let c = cos(a);
    return vec3<f32>(p.x, c * p.y - s * p.z, s * p.y + c * p.z);
}

fn rot_y(p: vec3<f32>, a: f32) -> vec3<f32> {
    let s = sin(a);
    let c = cos(a);
    return vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
}

fn sd_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sdf(p: vec3<f32>) -> f32 {
    var q = p;
    q = rot_x(q, uniforms.time);
    q = rot_y(q, uniforms.time * 0.7);
    return sd_box(q, vec3<f32>(0.5));
}

fn get_normal(p: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(0.001, 0.0);
    return normalize(vec3<f32>(
        sdf(p + e.xyy) - sdf(p - e.xyy),
        sdf(p + e.yxy) - sdf(p - e.yxy),
        sdf(p + e.yyx) - sdf(p - e.yyx)
    ));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ro = vec3<f32>(0.0, 0.0, -3.0);
    let uv = vec2<f32>(in.uv.x * uniforms.aspect_ratio, in.uv.y);
    let rd = normalize(vec3<f32>(uv, 1.5));

    var t = 0.0;
    for (var i = 0; i < 64; i++) {
        let p = ro + rd * t;
        let d = sdf(p);
        if (d < 0.001) {
            let n = get_normal(p);
            let light_dir = normalize(vec3<f32>(1.0, 1.0, -1.0));
            let diff = max(dot(n, light_dir), 0.0);
            let color = vec3<f32>(0.8, 0.5, 0.2) * diff + 0.1;
            return vec4<f32>(color, 1.0);
        }
        t += d;
        if (t > 10.0) { break; }
    }

    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
