use std::path::Path;
use std::time::Instant;
use crate::lexer::{Lexer, ParseError};
use crate::materials::MaterialRegistry;
use crate::math::{Vec3, Quat};
use crate::world::{Brush, World, KIND_BOX, KIND_CYLINDER, KIND_SPHERE, KIND_CONE, OP_ADD, OP_SUB};

pub fn parse_map(path: &Path, materials: &MaterialRegistry) -> Result<World, ParseError>
{
    let path_str = path.to_string_lossy();
    let mut lexer = Lexer::from_file(&path_str)?;
    let mut world = World::new();
    let start = Instant::now();
    let mut num_brushes = 0;

    loop {
        lexer.eat_ws()?;

        if lexer.eof() {
            break;
        }

        lexer.expect_token("(")?;

        if lexer.match_keyword("player")? {
            parse_player(&mut lexer, &mut world)?;
        } else {
            let brush = parse_entry(&mut lexer, materials)?;
            world.add_brush(brush);
            num_brushes += 1;
        }
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
