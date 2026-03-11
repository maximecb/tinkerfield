struct Uniforms {
    time: f32,
    aspect_ratio: f32,
};

struct Brush {
    pos: vec3<f32>,
    kind: u32,
    scale: vec3<f32>,
    material: u32,
    rot: vec4<f32>,
    op: u32,
};

struct Player {
    position: vec3<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var<storage, read> brushes: array<Brush>;

@group(1) @binding(1)
var<storage, read> grid: array<u32>; // packed u16 indices, 2 per u32

@group(1) @binding(2)
var<uniform> player: Player;

const GRID_W: u32 = 256u;
const GRID_D: u32 = 256u;
const GRID_H: u32 = 64u;
const GRID_C: u32 = 32u;
const SLOT_EMPTY: u32 = 65535u;

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

fn qrot(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}

fn sd_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn get_brush_sdf(p_world: vec3<f32>, brush_idx: u32) -> f32 {
    let b = brushes[brush_idx];

    // Transform world point to local space (but keep world scale for the SDF)
    let p_rel = p_world - b.pos;
    let q_inv = vec4<f32>(-b.rot.xyz, b.rot.w);
    let p_local = qrot(q_inv, p_rel);

    var d = 1e10;
    if (b.kind == 0u) { // BOX
        d = sd_box(p_local, b.scale * 0.5);
    } else if (b.kind == 2u) { // SPHERE
        d = length(p_local) - b.scale.x * 0.5;
    }

    return d;
}

fn sdf(p: vec3<f32>, ray_dir: vec3<f32>) -> f32 {
    // Determine grid cell
    let gx = u32(floor(p.x));
    let gy = u32(floor(p.y));
    let gz = u32(floor(p.z));

    if (gx >= GRID_W || gy >= GRID_H || gz >= GRID_D) {
        return 1.0;
    }

    let cell_idx = ((gy * GRID_D + gz) * GRID_W + gx) * GRID_C;

    // Calculate distance to next cell boundary as a default step
    let next_x = select(f32(gx + 1u), f32(gx), ray_dir.x < 0.0);
    let next_y = select(f32(gy + 1u), f32(gy), ray_dir.y < 0.0);
    let next_z = select(f32(gz + 1u), f32(gz), ray_dir.z < 0.0);

    let dt_x = (next_x - p.x) / ray_dir.x;
    let dt_y = (next_y - p.y) / ray_dir.y;
    let dt_z = (next_z - p.z) / ray_dir.z;

    var d = max(0.001, min(dt_x, min(dt_y, dt_z)) + 0.001);

    var found = false;
    for (var i = 0u; i < GRID_C; i++) {
        let word_idx = (cell_idx + i) / 2u;
        let word = grid[word_idx];
        let brush_idx = select(word & 0xFFFFu, word >> 16u, (i % 2u) == 1u);

        if (brush_idx == SLOT_EMPTY) {
            break;
        }

        let d_brush = get_brush_sdf(p, brush_idx);
        let op = brushes[brush_idx].op;

        if (!found) {
            d = d_brush;
            found = true;
        } else {
            if (op == 0u) { // ADD
                d = min(d, d_brush);
            } else { // SUB
                d = max(d, -d_brush);
            }
        }
    }

    return d;
}

fn get_normal(p: vec3<f32>, ray_dir: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(1.0, -1.0) * 0.001;
    return normalize(
        e.xyy * sdf(p + e.xyy, ray_dir) +
        e.yyx * sdf(p + e.yyx, ray_dir) +
        e.yxy * sdf(p + e.yxy, ray_dir) +
        e.xxx * sdf(p + e.xxx, ray_dir)
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ro = player.position;
    let uv = vec2<f32>(in.uv.x * uniforms.aspect_ratio, in.uv.y);
    let ray_dir = normalize(vec3<f32>(uv, 1.5));

    var t = 0.0;
    for (var i = 0; i < 256; i++) {
        let p = ro + ray_dir * t;
        let d = sdf(p, ray_dir);
        if (d < 0.001) {
            let n = get_normal(p, ray_dir);
            let light_dir = normalize(vec3<f32>(1.0, 1.0, -1.0));
            let diff = max(dot(n, light_dir), 0.2);
            let color = vec3<f32>(0.4, 0.5, 0.7) * diff;
            return vec4<f32>(color, 1.0);
        }
        t += d;
        if (t > 150.0) { break; }
    }

    // Sky gradient
    let sky = mix(vec3<f32>(0.1, 0.2, 0.4), vec3<f32>(0.5, 0.7, 1.0), in.uv.y * 0.5 + 0.5);
    return vec4<f32>(sky, 1.0);
}
