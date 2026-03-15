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
    focal_length: f32,
    forward: vec3<f32>,
    _pad1: f32,
    right: vec3<f32>,
    _pad2: f32,
    up: vec3<f32>,
    _pad3: f32,
};

struct WorldUniforms {
    grid_min: vec3<f32>,
    grid_size_x: u32,
    grid_size_y: u32,
    grid_size_z: u32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var<storage, read> brushes: array<Brush>;

@group(1) @binding(1)
var<storage, read> grid: array<u32>; // (offset << 8) | count

@group(1) @binding(2)
var<storage, read> grid_indices: array<u32>; // packed u16 indices, 2 per u32

@group(1) @binding(3)
var<uniform> player: Player;

@group(1) @binding(4)
var<uniform> world: WorldUniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) ray_dir: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(in_vertex_index & 1u) << 2u) - 1.0;
    let y = f32(i32(in_vertex_index & 2u) << 1u) - 1.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);

    // Calculate ray direction at this vertex
    out.ray_dir =
        out.uv.x * uniforms.aspect_ratio * player.right +
        out.uv.y * player.up +
        player.focal_length * player.forward;

    return out;
}

fn qrot(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let qxyz = q.xyz;
    let t = 2.0 * cross(qxyz, v);
    return v + q.w * t + cross(qxyz, t);
}

fn sd_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sd_cylinder(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let d = abs(vec2<f32>(length(p.xy), p.z)) - vec2<f32>(r, h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sdf_brush(p_world: vec3<f32>, brush_idx: u32) -> f32 {
    let b = brushes[brush_idx];

    // Transform world point to local space (but keep world scale for the SDF)
    let p_rel = p_world - b.pos;
    let q_inv = vec4<f32>(-b.rot.xyz, b.rot.w);
    let p_local = qrot(q_inv, p_rel);

    var d = 1e10;
    if (b.kind == 0u) { // BOX
        d = sd_box(p_local, b.scale * 0.5);
    } else if (b.kind == 1u) { // CYLINDER
        d = sd_cylinder(p_local, b.scale.z * 0.5, b.scale.x * 0.5);
    } else if (b.kind == 2u) { // SPHERE
        d = length(p_local) - b.scale.x * 0.5;
    }

    return d;
}

/// Unpack a u16 brush index from the grid_indices storage buffer.
/// Each u32 word contains two u16 indices.
fn get_grid_brush_index(idx: u32) -> u32 {
    let word = grid_indices[idx / 2u];
    return select(word & 0xFFFFu, word >> 16u, (idx % 2u) == 1u);
}

/// This function assumes we have valid grid coordinates
fn sdf_at_cell(p: vec3<f32>, cell_idx: u32) -> f32 {
    let cell_info = grid[cell_idx];
    let offset = cell_info >> 8u;
    let count = cell_info & 0xFFu;

    var d = 1e10;
    var found = false;
    for (var i = 0u; i < count; i++) {
        let brush_idx = get_grid_brush_index(offset + i);
        let d_brush = sdf_brush(p, brush_idx);
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

fn get_normal(p: vec3<f32>, cell_idx: u32) -> vec3<f32> {
    let e = vec2<f32>(1.0, -1.0) * 0.00005;
    return normalize(
        e.xyy * sdf_at_cell(p + e.xyy, cell_idx) +
        e.yyx * sdf_at_cell(p + e.yyx, cell_idx) +
        e.yxy * sdf_at_cell(p + e.yxy, cell_idx) +
        e.xxx * sdf_at_cell(p + e.xxx, cell_idx)
    );
}

fn intersect_aabb(ro: vec3<f32>, rd: vec3<f32>, b_min: vec3<f32>, b_max: vec3<f32>) -> vec2<f32> {
    let inv_rd = 1.0 / rd;
    let t1 = (b_min - ro) * inv_rd;
    let t2 = (b_max - ro) * inv_rd;
    let t_min = min(t1, t2);
    let t_max = max(t1, t2);
    let t_near = max(t_min.x, max(t_min.y, t_min.z));
    let t_far = min(t_max.x, min(t_max.y, t_max.z));
    return vec2<f32>(t_near, t_far);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ro = player.position;
    let rd = normalize(in.ray_dir);

    let grid_size = vec3<f32>(f32(world.grid_size_x), f32(world.grid_size_y), f32(world.grid_size_z));
    let t_hit = intersect_aabb(ro, rd, world.grid_min, world.grid_min + grid_size);

    var t = max(t_hit.x, 0.0);
    if (t > t_hit.y || t > 150.0) {
        // Sky gradient
        let sky = mix(vec3<f32>(0.5, 0.8, 1.0), vec3<f32>(0.1, 0.4, 0.9), in.uv.y * 0.5 + 0.5);
        return vec4<f32>(sky, 1.0);
    }

    let inv_rd = 1.0 / rd;
    let delta_t = abs(inv_rd);
    let step = vec3<i32>(sign(rd));

    let p_start = ro + rd * (t + 0.001);
    var ipos = vec3<i32>(floor(p_start - world.grid_min));
    var t_max = (vec3<f32>(ipos) - (ro - world.grid_min) + 0.5 + vec3<f32>(step) * 0.5) * inv_rd;

    for (var i = 0; i < 256; i++) {
        if (ipos.x < 0 || ipos.y < 0 || ipos.z < 0 ||
            ipos.x >= i32(world.grid_size_x) ||
            ipos.y >= i32(world.grid_size_y) ||
            ipos.z >= i32(world.grid_size_z)) {
            break;
        }

        let cell_idx = (u32(ipos.y) * world.grid_size_z + u32(ipos.z)) * world.grid_size_x + u32(ipos.x);
        let t_boundary = min(t_max.x, min(t_max.y, t_max.z));

        // Check if cell is potentially non-empty
        if ((grid[cell_idx] & 0xFFu) > 0u) {
            // Sphere trace within this cell
            while (t < t_boundary - 0.001) {
                let p = ro + rd * t;
                let d = sdf_at_cell(p, cell_idx);
                if (d < 0.001) {
                    let n = get_normal(p, cell_idx);
                    let light_dir = normalize(vec3<f32>(0.8, 1.0, -0.7));
                    let diff = max(dot(n, light_dir), 0.0);
                    let half_dir = normalize(light_dir - rd);
                    let spec = pow(max(dot(n, half_dir), 0.0), 32.0);
                    let ambient = 0.15;
                    let color = vec3<f32>(0.4, 0.5, 0.7) * (diff + ambient) + vec3<f32>(0.4) * spec;
                    return vec4<f32>(color, 1.0);
                }
                t += max(d, 0.001);
            }
        }

        t = t_boundary;

        // Step DDA
        if (t_max.x < t_max.y) {
            if (t_max.x < t_max.z) {
                t_max.x += delta_t.x;
                ipos.x += step.x;
            } else {
                t_max.z += delta_t.z;
                ipos.z += step.z;
            }
        } else {
            if (t_max.y < t_max.z) {
                t_max.y += delta_t.y;
                ipos.y += step.y;
            } else {
                t_max.z += delta_t.z;
                ipos.z += step.z;
            }
        }

        if (t > 150.0 || t > t_hit.y) { break; }
    }

    // Sky gradient
    let sky = mix(vec3<f32>(0.5, 0.8, 1.0), vec3<f32>(0.1, 0.4, 0.9), in.uv.y * 0.5 + 0.5);
    return vec4<f32>(sky, 1.0);
}
