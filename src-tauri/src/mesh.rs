//! 3D conversion.
//!
//! Two tiers, chosen in setup:
//!
//! * **Built in** (no download): stl, obj, glb and gltf in; stl, obj and glb
//!   out. These are geometry-only formats, so a small reader/writer pair
//!   covers them honestly.
//! * **Blender** (~400 MB): everything above plus fbx, dae, ply and .blend.
//!   Nothing else opens a .blend file, and rewriting FBX is not a weekend
//!   project.
//!
//! Materials, animations and scene hierarchies are not carried through the
//! built-in path — it converts meshes, and the UI says so.

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::engines::{self, EngineId};

pub const BUILTIN_INPUTS: &[&str] = &["stl", "obj", "glb", "gltf"];
pub const BUILTIN_OUTPUTS: &[&str] = &["glb", "obj", "stl"];
pub const BLENDER_INPUTS: &[&str] = &["fbx", "dae", "ply", "blend", "x3d"];
pub const BLENDER_OUTPUTS: &[&str] = &["fbx", "ply", "dae", "gltf"];

pub fn targets(app: &AppHandle) -> Vec<String> {
    let mut out: Vec<String> = BUILTIN_OUTPUTS.iter().map(|s| s.to_string()).collect();
    if engines::executable(app, EngineId::Blender).is_some() {
        out.extend(BLENDER_OUTPUTS.iter().map(|s| s.to_string()));
    }
    out
}

/// Why a model cannot be queued, if it cannot.
pub fn rejection(app: &AppHandle, extension: &str) -> Option<String> {
    let extension = extension.to_ascii_lowercase();
    if BUILTIN_INPUTS.contains(&extension.as_str()) {
        return None;
    }
    if BLENDER_INPUTS.contains(&extension.as_str()) {
        return engines::executable(app, EngineId::Blender)
            .is_none()
            .then(|| format!(".{extension} files need the Blender engine — enable it in setup"));
    }
    Some(format!(".{extension} is not a supported 3D format"))
}

/// Whether this pair can be done in-process, or needs Blender.
pub fn needs_blender(input: &Path, output: &Path) -> bool {
    let ext = |path: &Path| {
        path.extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    };
    !BUILTIN_INPUTS.contains(&ext(input).as_str())
        || !BUILTIN_OUTPUTS.contains(&ext(output).as_str())
}

pub struct BlenderPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cleanup: Vec<PathBuf>,
}

/// Blender is driven by a throwaway script rather than `--python-expr`, which
/// would need a shell-proof one-liner on two platforms.
pub fn blender_plan(
    app: &AppHandle,
    input: &Path,
    output: &Path,
    job_id: &str,
) -> Result<BlenderPlan, String> {
    let blender = engines::executable(app, EngineId::Blender)
        .ok_or("The Blender engine is not installed — enable it in setup")?;

    let scratch = std::env::temp_dir().join(format!("coldmill-{job_id}"));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let script = scratch.join("convert.py");
    std::fs::write(&script, BLENDER_SCRIPT).map_err(|e| e.to_string())?;

    Ok(BlenderPlan {
        program: blender,
        args: vec![
            "--background".into(),
            "--factory-startup".into(),
            "--python-exit-code".into(),
            "1".into(),
            "--python".into(),
            script.to_string_lossy().into_owned(),
            "--".into(),
            input.to_string_lossy().into_owned(),
            output.to_string_lossy().into_owned(),
        ],
        cleanup: vec![scratch],
    })
}

/// Import/export operator names moved around in Blender 4.2, so each call
/// tries the current name first and falls back to the legacy one.
const BLENDER_SCRIPT: &str = r#"
import bpy, os, sys

argv = sys.argv[sys.argv.index("--") + 1:]
src, dst = argv[0], argv[1]
src_ext = os.path.splitext(src)[1].lower()
dst_ext = os.path.splitext(dst)[1].lower()


def call(paths, **kwargs):
    last = None
    for path in paths:
        op = bpy.ops
        try:
            for part in path.split("."):
                op = getattr(op, part)
            op(**kwargs)
            return
        except Exception as err:
            last = err
    raise RuntimeError("no working operator for %s: %s" % (paths, last))


if src_ext == ".blend":
    bpy.ops.wm.open_mainfile(filepath=src)
else:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    importers = {
        ".stl": ["wm.stl_import", "import_mesh.stl"],
        ".obj": ["wm.obj_import", "import_scene.obj"],
        ".ply": ["wm.ply_import", "import_mesh.ply"],
        ".glb": ["import_scene.gltf"],
        ".gltf": ["import_scene.gltf"],
        ".fbx": ["import_scene.fbx"],
        ".dae": ["wm.collada_import"],
        ".x3d": ["import_scene.x3d"],
    }
    if src_ext not in importers:
        raise RuntimeError("unsupported input: %s" % src_ext)
    call(importers[src_ext], filepath=src)

if dst_ext in (".glb", ".gltf"):
    bpy.ops.export_scene.gltf(
        filepath=dst,
        export_format="GLB" if dst_ext == ".glb" else "GLTF_SEPARATE",
    )
elif dst_ext == ".fbx":
    bpy.ops.export_scene.fbx(filepath=dst)
elif dst_ext == ".stl":
    call(["wm.stl_export", "export_mesh.stl"], filepath=dst)
elif dst_ext == ".obj":
    call(["wm.obj_export", "export_scene.obj"], filepath=dst)
elif dst_ext == ".ply":
    call(["wm.ply_export", "export_mesh.ply"], filepath=dst)
elif dst_ext == ".dae":
    bpy.ops.wm.collada_export(filepath=dst)
else:
    raise RuntimeError("unsupported output: %s" % dst_ext)
"#;

// ---------------------------------------------------------------------------
// Built-in converter
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Flat face normals, computed when the source had none.
    fn ensure_normals(&mut self) {
        if self.normals.len() == self.positions.len() {
            return;
        }
        let mut normals = vec![[0.0f32; 3]; self.positions.len()];
        for triangle in self.indices.chunks_exact(3) {
            let [a, b, c] = [
                self.positions[triangle[0] as usize],
                self.positions[triangle[1] as usize],
                self.positions[triangle[2] as usize],
            ];
            let n = normal_of(a, b, c);
            for index in triangle {
                let slot = &mut normals[*index as usize];
                slot[0] += n[0];
                slot[1] += n[1];
                slot[2] += n[2];
            }
        }
        for n in &mut normals {
            let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if length > f32::EPSILON {
                n[0] /= length;
                n[1] /= length;
                n[2] /= length;
            }
        }
        self.normals = normals;
    }
}

fn normal_of(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

/// Runs a whole conversion in-process. Blocking — callers put it on a blocking
/// thread.
pub fn convert(input: &Path, output: &Path) -> Result<(), String> {
    let mut mesh = read(input)?;
    if mesh.positions.is_empty() {
        return Err("the file contains no geometry".into());
    }
    mesh.ensure_normals();
    write(&mesh, output)
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn read(path: &Path) -> Result<Mesh, String> {
    match extension_of(path).as_str() {
        "stl" => read_stl(path),
        "obj" => read_obj(path),
        "glb" | "gltf" => read_gltf(path),
        other => Err(format!("cannot read .{other} without Blender")),
    }
}

fn write(mesh: &Mesh, path: &Path) -> Result<(), String> {
    match extension_of(path).as_str() {
        "stl" => write_stl(mesh, path),
        "obj" => write_obj(mesh, path),
        "glb" => write_glb(mesh, path),
        other => Err(format!("cannot write .{other} without Blender")),
    }
}

// --- STL -------------------------------------------------------------------

fn read_stl(path: &Path) -> Result<Mesh, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() >= 84 {
        let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if bytes.len() == 84 + count * 50 {
            return Ok(read_binary_stl(&bytes, count));
        }
    }
    read_ascii_stl(&String::from_utf8_lossy(&bytes))
}

fn read_binary_stl(bytes: &[u8], count: usize) -> Mesh {
    let mut mesh = Mesh::default();
    let f32_at = |offset: usize| {
        f32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    for triangle in 0..count {
        // 12 bytes of normal, then three vertices, then 2 bytes of attributes.
        let base = 84 + triangle * 50 + 12;
        for vertex in 0..3 {
            let at = base + vertex * 12;
            mesh.positions
                .push([f32_at(at), f32_at(at + 4), f32_at(at + 8)]);
            mesh.indices.push((triangle * 3 + vertex) as u32);
        }
    }
    mesh
}

fn read_ascii_stl(text: &str) -> Result<Mesh, String> {
    let mut mesh = Mesh::default();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("vertex ") else {
            continue;
        };
        let coords: Vec<f32> = rest
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        if coords.len() != 3 {
            return Err("malformed vertex line in ASCII STL".into());
        }
        mesh.indices.push(mesh.positions.len() as u32);
        mesh.positions.push([coords[0], coords[1], coords[2]]);
    }
    Ok(mesh)
}

fn write_stl(mesh: &Mesh, path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = BufWriter::new(file);

    out.write_all(&[0u8; 80]).map_err(|e| e.to_string())?;
    out.write_all(&(mesh.triangle_count() as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;

    for triangle in mesh.indices.chunks_exact(3) {
        let vertices = [
            mesh.positions[triangle[0] as usize],
            mesh.positions[triangle[1] as usize],
            mesh.positions[triangle[2] as usize],
        ];
        let n = normal_of(vertices[0], vertices[1], vertices[2]);
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2])
            .sqrt()
            .max(f32::EPSILON);
        for component in [n[0] / length, n[1] / length, n[2] / length] {
            out.write_all(&component.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        for vertex in vertices {
            for component in vertex {
                out.write_all(&component.to_le_bytes())
                    .map_err(|e| e.to_string())?;
            }
        }
        out.write_all(&[0u8, 0u8]).map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())
}

// --- OBJ -------------------------------------------------------------------

fn read_obj(path: &Path) -> Result<Mesh, String> {
    let (models, _) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            single_index: true,
            triangulate: true,
            ..Default::default()
        },
    )
    .map_err(|e| format!("could not read OBJ: {e}"))?;

    let mut mesh = Mesh::default();
    for model in models {
        let offset = mesh.positions.len() as u32;
        mesh.positions.extend(
            model
                .mesh
                .positions
                .chunks_exact(3)
                .map(|c| [c[0], c[1], c[2]]),
        );
        mesh.normals.extend(
            model
                .mesh
                .normals
                .chunks_exact(3)
                .map(|c| [c[0], c[1], c[2]]),
        );
        mesh.indices
            .extend(model.mesh.indices.iter().map(|i| i + offset));
    }
    // A partially-normalled merge is worse than none: drop and recompute.
    if mesh.normals.len() != mesh.positions.len() {
        mesh.normals.clear();
    }
    Ok(mesh)
}

fn write_obj(mesh: &Mesh, path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = BufWriter::new(file);
    writeln!(out, "# generated by Coldmill").map_err(|e| e.to_string())?;

    for position in &mesh.positions {
        writeln!(out, "v {} {} {}", position[0], position[1], position[2])
            .map_err(|e| e.to_string())?;
    }
    for normal in &mesh.normals {
        writeln!(out, "vn {} {} {}", normal[0], normal[1], normal[2]).map_err(|e| e.to_string())?;
    }
    for triangle in mesh.indices.chunks_exact(3) {
        // OBJ indices are 1-based.
        let (a, b, c) = (triangle[0] + 1, triangle[1] + 1, triangle[2] + 1);
        writeln!(out, "f {a}//{a} {b}//{b} {c}//{c}").map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())
}

// --- glTF ------------------------------------------------------------------

fn read_gltf(path: &Path) -> Result<Mesh, String> {
    let (document, buffers, _) =
        gltf::import(path).map_err(|e| format!("could not read glTF: {e}"))?;

    let mut mesh = Mesh::default();
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or("the file has no scene")?;

    for node in scene.nodes() {
        collect_node(&node, &buffers, node.transform().matrix(), &mut mesh);
    }
    Ok(mesh)
}

/// Node transforms are baked into the vertices: the output formats here have
/// no scene graph to carry them.
fn collect_node(
    node: &gltf::Node,
    buffers: &[gltf::buffer::Data],
    world: [[f32; 4]; 4],
    out: &mut Mesh,
) {
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
            let Some(positions) = reader.read_positions() else {
                continue;
            };
            let positions: Vec<[f32; 3]> = positions.map(|p| transform(&world, p)).collect();
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|n| n.map(|n| rotate(&world, n)).collect())
                .unwrap_or_default();

            let offset = out.positions.len() as u32;
            let indices: Vec<u32> = match reader.read_indices() {
                Some(indices) => indices.into_u32().map(|i| i + offset).collect(),
                None => (0..positions.len() as u32).map(|i| i + offset).collect(),
            };

            if normals.len() == positions.len() {
                out.normals.extend(normals);
            } else if !out.normals.is_empty() {
                out.normals.clear();
            }
            out.positions.extend(positions);
            out.indices.extend(indices);
        }
    }

    for child in node.children() {
        collect_node(
            &child,
            buffers,
            multiply(&world, &child.transform().matrix()),
            out,
        );
    }
}

/// glTF matrices are column-major.
fn multiply(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for (column, target) in out.iter_mut().enumerate() {
        for (row, cell) in target.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][row] * b[column][k]).sum();
        }
    }
    out
}

fn transform(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
    ]
}

fn rotate(m: &[[f32; 4]; 4], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2],
    ]
}

/// Minimal GLB: one mesh, positions + normals + indices, no materials.
fn write_glb(mesh: &Mesh, path: &Path) -> Result<(), String> {
    let mut binary: Vec<u8> = Vec::new();
    for position in &mesh.positions {
        for component in position {
            binary.extend_from_slice(&component.to_le_bytes());
        }
    }
    let normals_offset = binary.len();
    for normal in &mesh.normals {
        for component in normal {
            binary.extend_from_slice(&component.to_le_bytes());
        }
    }
    let indices_offset = binary.len();
    for index in &mesh.indices {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    pad_to_four(&mut binary, 0);

    let (min, max) = bounds(&mesh.positions);
    let json = serde_json::json!({
        "asset": { "version": "2.0", "generator": "Coldmill" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0 }],
        "meshes": [{
            "primitives": [{
                "attributes": { "POSITION": 0, "NORMAL": 1 },
                "indices": 2,
                "mode": 4
            }]
        }],
        "accessors": [
            {
                "bufferView": 0, "componentType": 5126, "count": mesh.positions.len(),
                "type": "VEC3", "min": min, "max": max
            },
            {
                "bufferView": 1, "componentType": 5126, "count": mesh.normals.len(),
                "type": "VEC3"
            },
            {
                "bufferView": 2, "componentType": 5125, "count": mesh.indices.len(),
                "type": "SCALAR"
            }
        ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": normals_offset, "target": 34962 },
            {
                "buffer": 0, "byteOffset": normals_offset,
                "byteLength": indices_offset - normals_offset, "target": 34962
            },
            {
                "buffer": 0, "byteOffset": indices_offset,
                "byteLength": binary.len() - indices_offset, "target": 34963
            }
        ],
        "buffers": [{ "byteLength": binary.len() }]
    });

    let mut json = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    pad_to_four(&mut json, b' ');

    let total = 12 + 8 + json.len() + 8 + binary.len();
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = BufWriter::new(file);

    out.write_all(b"glTF").map_err(|e| e.to_string())?;
    out.write_all(&2u32.to_le_bytes())
        .map_err(|e| e.to_string())?;
    out.write_all(&(total as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;

    out.write_all(&(json.len() as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;
    out.write_all(b"JSON").map_err(|e| e.to_string())?;
    out.write_all(&json).map_err(|e| e.to_string())?;

    out.write_all(&(binary.len() as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;
    out.write_all(b"BIN\0").map_err(|e| e.to_string())?;
    out.write_all(&binary).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())
}

fn pad_to_four(buffer: &mut Vec<u8>, filler: u8) {
    while buffer.len() % 4 != 0 {
        buffer.push(filler);
    }
}

fn bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for position in positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    (min, max)
}

/// Reads just enough of a file to report its triangle count, for the UI.
pub fn quick_triangle_count(path: &Path) -> Option<usize> {
    if extension_of(path) != "stl" {
        return None;
    }
    let mut header = [0u8; 84];
    let mut file = std::fs::File::open(path).ok()?;
    file.read_exact(&mut header).ok()?;
    let size = file.metadata().ok()?.len();
    let count = u32::from_le_bytes([header[80], header[81], header[82], header[83]]) as u64;
    (size == 84 + count * 50).then_some(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn stl_survives_a_round_trip() {
        let dir = std::env::temp_dir().join("coldmill-mesh-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("triangle.stl");

        let mut mesh = triangle();
        mesh.ensure_normals();
        write_stl(&mesh, &path).unwrap();

        let back = read_stl(&path).unwrap();
        assert_eq!(back.positions.len(), 3);
        assert_eq!(back.indices, vec![0, 1, 2]);
        assert_eq!(back.positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(quick_triangle_count(&path), Some(1));
    }

    #[test]
    fn glb_is_well_formed() {
        let dir = std::env::temp_dir().join("coldmill-mesh-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("triangle.glb");

        let mut mesh = triangle();
        mesh.ensure_normals();
        write_glb(&mesh, &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize,
            bytes.len()
        );
        // And it must read back through a real glTF parser.
        let back = read_gltf(&path).unwrap();
        assert_eq!(back.positions.len(), 3);
        assert_eq!(back.indices, vec![0, 1, 2]);
    }

    #[test]
    fn obj_survives_a_round_trip() {
        let dir = std::env::temp_dir().join("coldmill-mesh-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("triangle.obj");

        let mut mesh = triangle();
        mesh.ensure_normals();
        write_obj(&mesh, &path).unwrap();

        let back = read_obj(&path).unwrap();
        assert_eq!(back.positions.len(), 3);
        assert_eq!(back.indices, vec![0, 1, 2]);
        assert_eq!(back.positions[2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn normals_are_generated_when_missing() {
        let mut mesh = triangle();
        mesh.ensure_normals();
        assert_eq!(mesh.normals.len(), 3);
        // A triangle in the XY plane faces +Z.
        assert!((mesh.normals[0][2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn identity_transform_is_a_no_op() {
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        assert_eq!(transform(&identity, [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
        assert_eq!(multiply(&identity, &identity), identity);
    }
}
