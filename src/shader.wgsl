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
    yaw: f32,
    pitch: f32,
};

struct OctreeNode {
    child_base_idx: u32,
    brush_count: u32,
    brush_offset: u32,
    _pad: u32,
};

struct OctreeRoot {
    min: vec3<f32>,
    size: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var<storage, read> brushes: array<Brush>;

@group(1) @binding(1)
var<storage, read> nodes: array<OctreeNode>;

@group(1) @binding(2)
var<storage, read> octree_indices: array<u32>; // packed u16 indices

@group(1) @binding(3)
var<uniform> root: OctreeRoot;

@group(1) @binding(4)
var<uniform> player: Player;

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

fn sdf_at_leaf(p: vec3<f32>, node: OctreeNode) -> f32 {
    var d = 1e10;
    var found = false;
    for (var i = 0u; i < node.brush_count; i++) {
        let idx_in_indices = node.brush_offset + i;
        let word = octree_indices[idx_in_indices / 2u];
        let brush_idx = select(word & 0xFFFFu, word >> 16u, (idx_in_indices % 2u) == 1u);

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

fn get_normal(p: vec3<f32>, node: OctreeNode) -> vec3<f32> {
    let e = vec2<f32>(1.0, -1.0) * 0.0005;
    return normalize(
        e.xyy * sdf_at_leaf(p + e.xyy, node) +
        e.yyx * sdf_at_leaf(p + e.yyx, node) +
        e.yxy * sdf_at_leaf(p + e.yxy, node) +
        e.xxx * sdf_at_leaf(p + e.xxx, node)
    );
}

fn intersect_aabb(ro: vec3<f32>, inv_rd: vec3<f32>, b_min: vec3<f32>, b_max: vec3<f32>) -> vec2<f32> {
    let t1 = (b_min - ro) * inv_rd;
    let t2 = (b_max - ro) * inv_rd;
    let t_min = min(t1, t2);
    let t_max = max(t1, t2);
    let n = max(t_min.x, max(t_min.y, t_min.z));
    let f = min(t_max.x, min(t_max.y, t_max.z));
    return vec2<f32>(n, f);
}

struct StackNode {
    idx: u32,
    min: vec3<f32>,
    size: f32,
    t_min: f32,
};

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ro = player.position;
    let rd = normalize(in.ray_dir);
    let inv_rd = 1.0 / rd;

    var stack: array<StackNode, 16>;
    var stack_ptr = 0;

    let root_hit = intersect_aabb(ro, inv_rd, root.min, root.min + root.size);
    if (root_hit.y < 0.0 || root_hit.x > root_hit.y) {
        let sky = mix(vec3<f32>(0.5, 0.8, 1.0), vec3<f32>(0.1, 0.4, 0.9), in.uv.y * 0.5 + 0.5);
        return vec4<f32>(sky, 1.0);
    }

    stack[stack_ptr] = StackNode(0u, root.min, root.size, max(root_hit.x, 0.0));
    stack_ptr++;

    while (stack_ptr > 0) {
        stack_ptr--;
        let sn = stack[stack_ptr];
        let node = nodes[sn.idx];

        if (node.child_base_idx == 0u) {
            // Leaf: sphere trace
            let node_hit = intersect_aabb(ro, inv_rd, sn.min, sn.min + sn.size);
            var t = max(sn.t_min, 0.0);
            let t_end = node_hit.y;

            if (node.brush_count > 0u) {
                while (t < t_end) {
                    let p = ro + rd * t;
                    let d = sdf_at_leaf(p, node);
                    if (d < 0.0005) {
                        let n = get_normal(p, node);
                        let light_dir = normalize(vec3<f32>(1.0, 1.0, -1.0));
                        let light_dir2 = normalize(vec3<f32>(-0.8, 0.4, 0.5));
                        let diff1 = max(dot(n, light_dir), 0.0);
                        let diff2 = max(dot(n, light_dir2), 0.0);
                        let half_dir = normalize(light_dir - rd);
                        let spec = pow(max(dot(n, half_dir), 0.0), 32.0);
                        let ambient = 0.15;
                        let color = vec3<f32>(0.4, 0.5, 0.7) * (diff1 + 0.3 * diff2 + ambient) + vec3<f32>(0.4) * spec;
                        return vec4<f32>(color, 1.0);
                    }
                    t += max(d, 0.0005);
                    if (t > 200.0) { break; }
                }
            }
        } else {
            // Internal node: push children in approximate distance order
            let child_size = sn.size * 0.5;
            
            // Front-to-back heuristic: decide which axes to flip based on ray direction
            let first = select(vec3<u32>(0u), vec3<u32>(1u), rd < vec3<f32>(0.0));
            
            for (var i = 7; i >= 0; i--) {
                // Determine child index by XORing with ray direction sign
                let child_idx_raw = u32(i);
                let child_idx = child_idx_raw ^ (first.x | (first.y << 1u) | (first.z << 2u));
                
                let offset = vec3<f32>(
                    f32(child_idx & 1u) * child_size,
                    f32((child_idx >> 1u) & 1u) * child_size,
                    f32((child_idx >> 2u) & 1u) * child_size
                );
                let c_min = sn.min + offset;
                let c_hit = intersect_aabb(ro, inv_rd, c_min, c_min + child_size);
                
                if (c_hit.y >= 0.0 && c_hit.x <= c_hit.y && c_hit.y >= sn.t_min) {
                    stack[stack_ptr] = StackNode(node.child_base_idx + child_idx, c_min, child_size, max(c_hit.x, 0.0));
                    stack_ptr++;
                }
            }
        }
    }

    let sky = mix(vec3<f32>(0.5, 0.8, 1.0), vec3<f32>(0.1, 0.4, 0.9), in.uv.y * 0.5 + 0.5);
    return vec4<f32>(sky, 1.0);
}
