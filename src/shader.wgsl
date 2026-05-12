struct Uniforms {
    time: f32,
    aspect_ratio: f32,
    pixel_size_at_1m: f32,
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

@group(2) @binding(0)
var material_textures: texture_2d_array<f32>;

@group(2) @binding(1)
var material_sampler: sampler;

@group(2) @binding(2)
var<storage, read> specular_factors: array<f32>;

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

fn sd_box(p: vec3<f32>, size: vec3<f32>) -> f32 {
    let q = abs(p) - size;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sd_cylinder(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let d = abs(vec2<f32>(length(p.xz), p.y)) - vec2<f32>(r, h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sd_cone(p: vec3<f32>, h: f32, r1: f32, r2: f32) -> f32 {
    let q = vec2<f32>(length(p.xz), p.y);
    let k1 = vec2<f32>(r2, h);
    let k2 = vec2<f32>(r2 - r1, 2.0 * h);
    let ca = vec2<f32>(q.x - min(q.x, select(r2, r1, q.y < 0.0)), abs(q.y) - h);
    let cb = q - k1 + k2 * clamp(dot(k1 - q, k2) / dot(k2, k2), 0.0, 1.0);
    let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
    return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}

fn sd_ellipsoid(p: vec3<f32>, r: vec3<f32>) -> f32 {
    let k0 = length(p / r);
    let k1 = length(p / (r * r));
    return k0 * (k0 - 1.0) / k1;
}

fn sdf_brush(p_world: vec3<f32>, brush_idx: u32) -> f32 {
    let b = brushes[brush_idx];

    // Transform world point to local space
    let p_rel = p_world - b.pos;
    let q_inv = vec4<f32>(-b.rot.xyz, b.rot.w);
    let p_local = qrot(q_inv, p_rel);

    // Scaling factor (half-extents)
    let s = b.scale * 0.5;

    var d = 1e10;
    if (b.kind == 0u) { // BOX
        d = sd_box(p_local, s);
    } else if (b.kind == 1u) { // CYLINDER
        // Scaling trick: scale point, call unit primitive, multiply by min scale
        d = sd_cylinder(p_local / s, 1.0, 1.0) * min(s.x, min(s.y, s.z));
    } else if (b.kind == 2u) { // SPHERE (Ellipsoid)
        d = sd_ellipsoid(p_local, s);
    } else if (b.kind == 3u) { // CONE
        d = sd_cone(p_local / s, 1.0, 1.0, 0.0) * min(s.x, min(s.y, s.z));
    }

    return d;
}

/// Unpack a u16 brush index from the grid_indices storage buffer.
/// Each u32 word contains two u16 indices.
fn get_grid_brush_index(idx: u32) -> u32 {
    let word = grid_indices[idx / 2u];
    return select(word & 0xFFFFu, word >> 16u, (idx % 2u) == 1u);
}

struct Hit {
    d: f32,
    mat_id: u32,
};

/// This function assumes we have valid grid coordinates
fn sdf_at_cell(p: vec3<f32>, cell_idx: u32) -> Hit {
    let cell_info = grid[cell_idx];
    let offset = cell_info >> 8u;
    let count = cell_info & 0xFFu;

    var res = Hit(1e10, 0u);
    var found = false;
    for (var i = 0u; i < count; i++) {
        let brush_idx = get_grid_brush_index(offset + i);
        let d_brush = sdf_brush(p, brush_idx);
        let b = brushes[brush_idx];
        let op = b.op;

        if (!found) {
            res.d = d_brush;
            res.mat_id = b.material;
            found = true;
        } else {
            if (op == 0u) { // ADD
                if (d_brush < res.d) {
                    res.d = d_brush;
                    res.mat_id = b.material;
                }
            } else { // SUB
                if (-d_brush > res.d) {
                    res.d = -d_brush;
                }
            }
        }
    }

    return res;
}

fn get_normal(p: vec3<f32>, cell_idx: u32) -> vec3<f32> {
    let e = vec2<f32>(1.0, -1.0) * 0.00001;
    return normalize(
        e.xyy * sdf_at_cell(p + e.xyy, cell_idx).d +
        e.yyx * sdf_at_cell(p + e.yyx, cell_idx).d +
        e.yxy * sdf_at_cell(p + e.yxy, cell_idx).d +
        e.xxx * sdf_at_cell(p + e.xxx, cell_idx).d
    );
}

fn triplanar_sample(p: vec3<f32>, n: vec3<f32>, mat_id: u32, t: f32) -> vec3<f32> {
    let blending = abs(n);
    let b = blending / (blending.x + blending.y + blending.z);

    // Scale world-space coordinates to UV space:
    // 1024 texels per texture / 512 texels per meter = 2 meters per texture repeat.
    let uv_p = p * 0.5;

    // Calculate LOD based on distance t.
    // At distance t, one screen pixel covers ~ (t * uniforms.pixel_size_at_1m) world units.
    // Our texture is 1024 texels for 2 meters (512 texels/m).
    // Texels covered per pixel = (t * uniforms.pixel_size_at_1m) * 512.0
    // LOD = log2(texels per pixel)
    let texels_per_pixel = t * uniforms.pixel_size_at_1m * 512.0;
    let lod = max(0.0, log2(texels_per_pixel));

    let xaxis = textureSampleLevel(material_textures, material_sampler, uv_p.zy, i32(mat_id), lod).rgb;
    let yaxis = textureSampleLevel(material_textures, material_sampler, uv_p.xz, i32(mat_id), lod).rgb;
    let zaxis = textureSampleLevel(material_textures, material_sampler, uv_p.xy, i32(mat_id), lod).rgb;

    return xaxis * b.x + yaxis * b.y + zaxis * b.z;
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

struct RayResult {
    t: f32,
    cell_idx: u32,
    mat_id: u32,
    hit: bool,
};

fn ray_march(ro: vec3<f32>, rd: vec3<f32>, max_t: f32) -> RayResult {
    let grid_size = vec3<f32>(f32(world.grid_size_x), f32(world.grid_size_y), f32(world.grid_size_z));
    let t_hit = intersect_aabb(ro, rd, world.grid_min, world.grid_min + grid_size);

    var res: RayResult;
    res.hit = false;
    res.t = max_t;

    // If the ray doesn't intersect the grid
    var t = max(t_hit.x, 0.0);
    if (t > t_hit.y || t > max_t) { return res; }

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
                let hit = sdf_at_cell(p, cell_idx);
                let d = hit.d;

                // Calculate epsilon as a fraction of the pixel size at distance t
                let epsilon = t * uniforms.pixel_size_at_1m * 0.125;

                if (d < epsilon) {
                    res.t = t;
                    res.cell_idx = cell_idx;
                    res.mat_id = hit.mat_id;
                    res.hit = true;
                    return res;
                }
                t += max(d, epsilon);
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

        if (t > max_t || t > t_hit.y) { break; }
    }

    return res;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ro = player.position;
    let rd = normalize(in.ray_dir);

    let res = ray_march(ro, rd, 150.0);

    if (!res.hit) {
        // Sky gradient
        let sky = mix(vec3<f32>(0.5, 0.8, 1.0), vec3<f32>(0.1, 0.4, 0.9), in.uv.y * 0.5 + 0.5);
        return vec4<f32>(sky, 1.0);
    }

    let t = res.t;
    let p = ro + rd * t;
    let cell_idx = res.cell_idx;
    let mat_id = res.mat_id;

    let n = get_normal(p, cell_idx);
    let albedo = triplanar_sample(p, n, mat_id, t);
    let spec_factor = specular_factors[mat_id];

    let light_dir = normalize(vec3<f32>(0.85, 1.0, -0.7));
    let shadow_res = ray_march(p + n * 0.01, light_dir, 50.0);
    var shadow = 1.0;
    if (shadow_res.hit) {
        shadow = 0.0;
    }

    let diff = max(dot(n, light_dir), 0.0) * shadow;
    let half_dir = normalize(light_dir - rd);
    let spec_highlight = pow(max(dot(n, half_dir), 0.0), 32.0) * shadow;
    let ambient = 0.15;

    let color = albedo * (diff + ambient) + vec3<f32>(spec_factor) * spec_highlight;
    return vec4<f32>(color, 1.0);
}
