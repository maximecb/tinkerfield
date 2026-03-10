/// Rotate a 3D vector by a quaternion (x, y, z, w)
pub fn quat_rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3]
{
    let [qx, qy, qz, qw] = q;
    let [vx, vy, vz] = v;

    // Standard quaternion-vector rotation: q * (0, v) * conj(q)
    let tx = 2.0 * (qy * vz - qz * vy);
    let ty = 2.0 * (qz * vx - qx * vz);
    let tz = 2.0 * (qx * vy - qy * vx);

    [
        vx + qw * tx + (qy * tz - qz * ty),
        vy + qw * ty + (qz * tx - qx * tz),
        vz + qw * tz + (qx * ty - qy * tx),
    ]
}

pub fn vec3_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3]
{
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn vec3_mul(a: [f32; 3], b: [f32; 3]) -> [f32; 3]
{
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}
