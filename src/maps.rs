use std::fs;
use std::io;
use std::path::Path;
use std::time::Instant;
use crate::lexer::{Lexer, ParseError};
use crate::materials::MaterialRegistry;
use crate::math::{Vec3, Quat};
use crate::world::{Brush, Player, World, KIND_BOX, KIND_CYLINDER, KIND_SPHERE, KIND_CONE, OP_ADD, OP_SUB};

pub fn parse_map(path: &Path, materials: &MaterialRegistry) -> Result<World, ParseError>
{
    let path_str = path.to_string_lossy();
    let mut lexer = Lexer::from_file(&path_str)?;
    let mut world = World::new();
    let start = Instant::now();
    let mut num_brushes = 0;

    let mut prev_end: usize = 0;
    let mut header_set = false;

    loop {
        lexer.eat_ws()?;

        if lexer.eof() {
            // Trailing whitespace + comments after the last entry are dropped
            break;
        }

        let leading = lexer.capture_from(prev_end);

        lexer.expect_token("(")?;

        if lexer.match_keyword("player")? {
            parse_player(&mut lexer, &mut world)?;
            if !header_set {
                world.set_header(leading);
                header_set = true;
            }
        } else {
            let brush = parse_entry(&mut lexer, materials)?;
            let id = world.add_brush(brush);
            if !header_set {
                world.set_header(leading);
                header_set = true;
            } else {
                world.comments.insert(id, leading);
            }
            num_brushes += 1;
        }

        prev_end = lexer.cur_idx();
    }

    let elapsed_ms = start.elapsed().as_millis();
    println!("Loaded map with {} brushes in {}ms", num_brushes, elapsed_ms);

    Ok(world)
}

fn parse_player(lexer: &mut Lexer, world: &mut World) -> Result<(), ParseError>
{
    let mut x = world.player.position.x;
    let mut y = world.player.position.y;
    let mut z = world.player.position.z;
    let mut ra = world.player.yaw;

    loop {
        lexer.eat_ws()?;

        if lexer.match_char(')') {
            break;
        }

        let key = lexer.parse_ident()?;
        lexer.expect_token("=")?;

        match key.as_str() {
            "x"  => x  = parse_f32(lexer)?,
            "y"  => y  = parse_f32(lexer)?,
            "z"  => z  = parse_f32(lexer)?,
            "ra" => ra = parse_f32(lexer)?,
            _ => return lexer.parse_error(&format!("unknown player attribute \"{}\"", key)),
        }
    }

    world.player.position = Vec3::new(x, y, z);
    world.player.yaw = ra;
    world.player.update_basis();

    Ok(())
}

/// Parse a brush entry. '(' must already be consumed by the caller.
fn parse_entry(lexer: &mut Lexer, materials: &MaterialRegistry) -> Result<Brush, ParseError>
{
    lexer.eat_ws()?;
    let keyword = lexer.parse_ident()?;

    match keyword.as_str() {
        "sub" => {
            lexer.expect_token("(")?;
            let mut brush = parse_entry(lexer, materials)?;
            brush.op = OP_SUB;
            lexer.expect_token(")")?;
            Ok(brush)
        }
        "box"      => parse_brush(lexer, materials, KIND_BOX),
        "cylinder" => parse_brush(lexer, materials, KIND_CYLINDER),
        "sphere"   => parse_brush(lexer, materials, KIND_SPHERE),
        "cone"     => parse_brush(lexer, materials, KIND_CONE),
        _ => lexer.parse_error(&format!("unknown shape \"{}\"", keyword)),
    }
}

fn parse_brush(lexer: &mut Lexer, materials: &MaterialRegistry, kind: u32) -> Result<Brush, ParseError>
{
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut z = 0.0f32;
    let mut s = 1.0f32;
    let mut sx: Option<f32> = None;
    let mut sy: Option<f32> = None;
    let mut sz: Option<f32> = None;
    let mut ra = 0.0f32;
    let mut rx: Option<f32> = None;
    let mut ry: Option<f32> = None;
    let mut rz: Option<f32> = None;
    let mut mat_name = String::from("concrete_01");

    loop {
        lexer.eat_ws()?;

        if lexer.match_char(')') {
            break;
        }

        let key = lexer.parse_ident()?;
        lexer.expect_token("=")?;

        match key.as_str() {
            "x"   => x  = parse_f32(lexer)?,
            "y"   => y  = parse_f32(lexer)?,
            "z"   => z  = parse_f32(lexer)?,
            "s"   => s  = parse_f32(lexer)?,
            "sx"  => sx = Some(parse_f32(lexer)?),
            "sy"  => sy = Some(parse_f32(lexer)?),
            "sz"  => sz = Some(parse_f32(lexer)?),
            "ra"  => ra = parse_f32(lexer)?,
            "rx"  => rx = Some(parse_f32(lexer)?),
            "ry"  => ry = Some(parse_f32(lexer)?),
            "rz"  => rz = Some(parse_f32(lexer)?),
            "mat" => {
                lexer.eat_ws()?;
                let quote = lexer.peek_ch();
                if quote != '\'' && quote != '"' {
                    return lexer.parse_error("expected string for mat=");
                }
                mat_name = lexer.parse_str(quote)?;
            }
            _ => return lexer.parse_error(&format!("unknown attribute \"{}\"", key)),
        }
    }

    let pos = Vec3::new(x, y, z);
    let scale = Vec3::new(
        sx.unwrap_or(s),
        sy.unwrap_or(s),
        sz.unwrap_or(s),
    );

    let rot = if ra != 0.0 {
        let axis = Vec3::new(
            rx.unwrap_or(0.0),
            ry.unwrap_or(1.0),
            rz.unwrap_or(0.0),
        ).normalize();
        Quat::from_axis_angle(axis, ra.to_radians())
    } else {
        Quat::IDENTITY
    };

    let material = materials.id_from_name(&mat_name);

    Ok(Brush {
        pos,
        kind,
        scale,
        material,
        rot,
        op: OP_ADD,
        _pad: [0; 3],
    })
}

fn parse_f32(lexer: &mut Lexer) -> Result<f32, ParseError>
{
    lexer.eat_ws()?;
    let s = lexer.read_numeric();
    s.parse::<f32>().map_err(|_| ParseError::new(lexer, "expected number"))
}

/// Serialize a world back to a map file, preserving comments captured at load.
/// Entries are written without trailing newlines so that the leading whitespace
/// captured above the *next* entry provides the separator, matching what the
/// parser saw. A single newline is appended at the end of the file.
pub fn save_map(path: &Path, world: &World, materials: &MaterialRegistry) -> io::Result<()>
{
    let mut out = String::new();
    out.push_str(&world.header);
    out.push_str(&serialize_player(&world.player));

    for (id, brush) in world.active_brushes() {
        // Guarantee a newline before each brush, regardless of whether captured
        // leading trivia exists or starts with one. Then strip a leading '\n'
        // from the captured leading so the normal case stays byte-identical.
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if let Some(c) = world.comments.get(&id) {
            let c = c.strip_prefix('\n').unwrap_or(c);
            out.push_str(c);
        }
        out.push_str(&serialize_brush(brush, materials));
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }

    fs::write(path, out)
}

fn serialize_player(p: &Player) -> String
{
    format!(
        "(player x={} y={} z={} ra={})",
        fmt_f32(p.position.x),
        fmt_f32(p.position.y),
        fmt_f32(p.position.z),
        fmt_f32(p.yaw),
    )
}

fn serialize_brush(b: &Brush, materials: &MaterialRegistry) -> String
{
    let kind = match b.kind {
        KIND_BOX      => "box",
        KIND_CYLINDER => "cylinder",
        KIND_SPHERE   => "sphere",
        KIND_CONE     => "cone",
        _             => "box",
    };

    let mut s = String::new();
    if b.op == OP_SUB {
        s.push_str("(sub ");
    }

    s.push('(');
    s.push_str(kind);
    s.push_str(&format!(" x={} y={} z={}", fmt_f32(b.pos.x), fmt_f32(b.pos.y), fmt_f32(b.pos.z)));
    s.push_str(&format!(" sx={} sy={} sz={}", fmt_f32(b.scale.x), fmt_f32(b.scale.y), fmt_f32(b.scale.z)));

    let (axis, angle_rad) = b.rot.to_axis_angle();
    if angle_rad.abs() > 1e-6 {
        s.push_str(&format!(" ra={}", fmt_f32(angle_rad.to_degrees())));
        let default_axis = axis.x.abs() < 1e-5
            && (axis.y - 1.0).abs() < 1e-5
            && axis.z.abs() < 1e-5;
        if !default_axis {
            s.push_str(&format!(" rx={} ry={} rz={}", fmt_f32(axis.x), fmt_f32(axis.y), fmt_f32(axis.z)));
        }
    }

    s.push_str(&format!(" mat='{}'", materials.material_name(b.material)));

    s.push(')');
    if b.op == OP_SUB {
        s.push(')');
    }

    s
}

/// Format an f32 using Rust's shortest round-trippable form, but normalize
/// negative zero to "0".
fn fmt_f32(v: f32) -> String
{
    if v == 0.0 {
        return "0".to_string();
    }
    format!("{}", v)
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn castle_roundtrip_preserves_comments()
    {
        let materials = MaterialRegistry::load();
        let path = Path::new("maps/castle.map");

        let world = parse_map(path, &materials).expect("parse");
        let out_path = std::env::temp_dir().join("castle_roundtrip.map");
        save_map(&out_path, &world, &materials).expect("save");

        // Re-parse and confirm brush count + header survive a second trip
        let world2 = parse_map(&out_path, &materials).expect("re-parse");
        assert_eq!(world.active_brushes().count(), world2.active_brushes().count());
        assert_eq!(world.header, world2.header);

        // Spot-check that file-level comment survived the round trip
        let saved = std::fs::read_to_string(&out_path).expect("read saved");
        assert!(saved.starts_with("# Castle on a hill"),
            "saved file should start with header comment, got: {:?}",
            &saved[..80.min(saved.len())]);
        assert!(saved.contains("# === Ground ==="),
            "section divider comment should survive");
        assert!(saved.contains("# === Crenellations"),
            "crenellation comment should survive");
    }

    #[test]
    fn all_maps_roundtrip()
    {
        let materials = MaterialRegistry::load();
        for name in ["castle", "city", "fountain", "house", "nature"] {
            let src_path = Path::new("maps").join(format!("{}.map", name));
            let world = parse_map(&src_path, &materials)
                .unwrap_or_else(|e| panic!("parse {}: {}", name, e));
            let out_path = std::env::temp_dir().join(format!("{}_roundtrip.map", name));
            save_map(&out_path, &world, &materials)
                .unwrap_or_else(|e| panic!("save {}: {}", name, e));
            let world2 = parse_map(&out_path, &materials)
                .unwrap_or_else(|e| panic!("re-parse {}: {}", name, e));
            assert_eq!(
                world.active_brushes().count(),
                world2.active_brushes().count(),
                "brush count mismatch for {}", name);
        }
    }

    #[test]
    fn nature_roundtrip_preserves_rotations()
    {
        let materials = MaterialRegistry::load();
        let path = Path::new("maps/nature.map");

        let world = parse_map(path, &materials).expect("parse");
        let out_path = std::env::temp_dir().join("nature_roundtrip.map");
        save_map(&out_path, &world, &materials).expect("save");

        // Re-parse and verify rotations survive — compare each brush's rotation
        let world2 = parse_map(&out_path, &materials).expect("re-parse");
        let b1: Vec<_> = world.active_brushes().map(|(_, b)| *b).collect();
        let b2: Vec<_> = world2.active_brushes().map(|(_, b)| *b).collect();
        assert_eq!(b1.len(), b2.len());
        for (a, b) in b1.iter().zip(b2.iter()) {
            // Rotations should round-trip to within a small tolerance
            let dot = a.rot.x * b.rot.x + a.rot.y * b.rot.y + a.rot.z * b.rot.z + a.rot.w * b.rot.w;
            assert!(dot.abs() > 0.999, "rotation mismatch: {:?} vs {:?}", a.rot, b.rot);
        }
    }
}
