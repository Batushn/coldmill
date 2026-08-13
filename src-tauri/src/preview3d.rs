//! A small software renderer, so a 3D file gets a picture in the queue like
//! everything else.
//!
//! There is no GPU involved and no dependency added. The queue already parses
//! these files to count their triangles, so drawing them costs one more pass
//! over geometry that is in memory anyway — cheaper than the thumbnail of a
//! video, which has to decode.
//!
//! The output is a PPM. ffmpeg is already a sidecar and reads PPM natively, so
//! turning it into the JPEG the UI wants costs no new code and no new crate.

use crate::mesh::Mesh;

/// Where the camera stands, in radians. A three-quarter view from slightly
/// above: the angle that shows three faces of a box, and so tells you the most
/// about a shape you have never seen.
const YAW: f32 = -0.6;
const PITCH: f32 = 0.45;

/// Matches `--surface` in styles.css, so the picture sits in the row without a
/// visible edge around it.
const BACKGROUND: [u8; 3] = [30, 32, 36];
/// A neutral clay. Anything more colourful competes with the file's own
/// thumbnail neighbours for attention.
const MATERIAL: [f32; 3] = [0.62, 0.66, 0.72];
/// Enough that a face pointing away from the light is still a shape rather
/// than a silhouette.
const AMBIENT: f32 = 0.25;

/// Renders to a binary PPM (P6), or `None` when there is nothing to draw.
pub fn render(mesh: &Mesh, width: usize, height: usize) -> Option<Vec<u8>> {
    if mesh.indices.len() < 3 || mesh.positions.is_empty() {
        return None;
    }

    let (min, max) = bounds(&mesh.positions);
    let centre = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];

    // Turn the model, then fit whatever that leaves on screen. Fitting before
    // rotating would clip a long diagonal the moment it swung into view.
    let view: Vec<[f32; 3]> = mesh
        .positions
        .iter()
        .map(|point| {
            rotate([
                point[0] - centre[0],
                point[1] - centre[1],
                point[2] - centre[2],
            ])
        })
        .collect();

    let (vmin, vmax) = bounds(&view);
    let span = (vmax[0] - vmin[0]).max(vmax[1] - vmin[1]);
    if !span.is_finite() || span <= 0.0 {
        return None;
    }
    // A tenth of the frame as breathing room on the tightest axis.
    let scale = (width.min(height) as f32 * 0.9) / span;

    let mut colour = vec![BACKGROUND; width * height];
    let mut depth = vec![f32::NEG_INFINITY; width * height];

    for triangle in mesh.indices.chunks_exact(3) {
        let Some(corners) = triangle
            .iter()
            .map(|index| view.get(*index as usize).copied())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };

        let screen: Vec<[f32; 3]> = corners
            .iter()
            .map(|point| {
                [
                    width as f32 / 2.0 + point[0] * scale,
                    // Screen y grows downward; model y grows up.
                    height as f32 / 2.0 - point[1] * scale,
                    point[2],
                ]
            })
            .collect();

        // The light sits at the camera, so brightness reads as "how much is
        // this face turned towards me" — the cue that makes a flat image look
        // solid.
        let normal = face_normal(corners[0], corners[1], corners[2]);
        let lit = AMBIENT + (1.0 - AMBIENT) * normal[2].abs();
        let shade = [
            (MATERIAL[0] * lit * 255.0) as u8,
            (MATERIAL[1] * lit * 255.0) as u8,
            (MATERIAL[2] * lit * 255.0) as u8,
        ];

        fill(&screen, shade, width, height, &mut colour, &mut depth);
    }

    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in colour {
        ppm.extend_from_slice(&pixel);
    }
    Some(ppm)
}

/// Yaw then pitch. Written out rather than built as matrices: it is two
/// rotations that never change, and the long form is the readable one.
fn rotate(p: [f32; 3]) -> [f32; 3] {
    let (sy, cy) = YAW.sin_cos();
    let (sp, cp) = PITCH.sin_cos();
    let x = p[0] * cy + p[2] * sy;
    let z = -p[0] * sy + p[2] * cy;
    let y = p[1] * cp - z * sp;
    let z = p[1] * sp + z * cp;
    [x, y, z]
}

fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in points {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    (min, max)
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length <= f32::EPSILON {
        return [0.0, 0.0, 1.0];
    }
    [n[0] / length, n[1] / length, n[2] / length]
}

/// Scanline fill with a depth test, over the triangle's bounding box only.
fn fill(
    screen: &[[f32; 3]],
    shade: [u8; 3],
    width: usize,
    height: usize,
    colour: &mut [[u8; 3]],
    depth: &mut [f32],
) {
    let (ax, ay) = (screen[0][0], screen[0][1]);
    let (bx, by) = (screen[1][0], screen[1][1]);
    let (cx, cy) = (screen[2][0], screen[2][1]);

    let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    // Zero area means the triangle is edge-on and covers nothing.
    if area.abs() <= f32::EPSILON {
        return;
    }

    let left = ax.min(bx).min(cx).floor().max(0.0) as usize;
    let right = (ax.max(bx).max(cx).ceil() as isize).clamp(0, width as isize) as usize;
    let top = ay.min(by).min(cy).floor().max(0.0) as usize;
    let bottom = (ay.max(by).max(cy).ceil() as isize).clamp(0, height as isize) as usize;

    for y in top..bottom {
        for x in left..right {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Barycentric coordinates: inside when all three have the same
            // sign as the triangle's own winding.
            let w0 = ((bx - px) * (cy - py) - (by - py) * (cx - px)) / area;
            let w1 = ((cx - px) * (ay - py) - (cy - py) * (ax - px)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }

            let z = w0 * screen[0][2] + w1 * screen[1][2] + w2 * screen[2][2];
            let at = y * width + x;
            if z > depth[at] {
                depth[at] = z;
                colour[at] = shade;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube, as twelve triangles.
    fn cube() -> Mesh {
        let positions = vec![
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ];
        let indices = vec![
            0, 1, 2, 0, 2, 3, // back
            4, 6, 5, 4, 7, 6, // front
            0, 4, 5, 0, 5, 1, // bottom
            3, 2, 6, 3, 6, 7, // top
            0, 3, 7, 0, 7, 4, // left
            1, 5, 6, 1, 6, 2, // right
        ];
        Mesh {
            positions,
            normals: Vec::new(),
            indices,
        }
    }

    fn pixels(ppm: &[u8], width: usize, height: usize) -> Vec<[u8; 3]> {
        let header = format!("P6\n{width} {height}\n255\n");
        ppm[header.len()..]
            .chunks_exact(3)
            .map(|p| [p[0], p[1], p[2]])
            .collect()
    }

    #[test]
    fn an_empty_mesh_draws_nothing() {
        assert!(render(&Mesh::default(), 64, 64).is_none());
    }

    #[test]
    fn a_cube_covers_the_middle_and_leaves_the_corners() {
        let ppm = render(&cube(), 64, 64).expect("a picture");
        let pixels = pixels(&ppm, 64, 64);
        assert_eq!(pixels.len(), 64 * 64);

        assert_ne!(
            pixels[32 * 64 + 32],
            BACKGROUND,
            "the middle of the frame should be the model"
        );
        assert_eq!(
            pixels[0], BACKGROUND,
            "a cube seen at an angle cannot reach the corner of the frame"
        );
    }

    #[test]
    fn the_faces_are_not_all_the_same_shade() {
        // The whole point of the three-quarter view: without shading the
        // silhouette would be all the picture says.
        let ppm = render(&cube(), 96, 96).expect("a picture");
        let shades: std::collections::HashSet<[u8; 3]> = pixels(&ppm, 96, 96)
            .into_iter()
            .filter(|pixel| *pixel != BACKGROUND)
            .collect();
        assert!(
            shades.len() >= 3,
            "three faces are in view, so three shades: {shades:?}"
        );
    }

    #[test]
    fn a_flat_mesh_edge_on_is_not_a_crash() {
        let flat = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2],
        };
        // Nothing to assert about the picture; the point is that it returns.
        let _ = render(&flat, 32, 32);
    }

    /// Not an assertion — it writes a picture out so a human can look at it.
    /// `cargo test --lib -- --ignored looks_right` then open the file.
    #[test]
    #[ignore]
    fn looks_right() {
        // A UV sphere with a flat base, so shading, silhouette and the
        // depth test all have something to get wrong.
        let (rings, segments) = (24usize, 32usize);
        let mut positions = Vec::new();
        for ring in 0..=rings {
            let phi = std::f32::consts::PI * ring as f32 / rings as f32;
            for segment in 0..=segments {
                let theta = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;
                positions.push([
                    phi.sin() * theta.cos(),
                    phi.cos().max(-0.7),
                    phi.sin() * theta.sin(),
                ]);
            }
        }
        let mut indices = Vec::new();
        let stride = segments + 1;
        for ring in 0..rings {
            for segment in 0..segments {
                let a = (ring * stride + segment) as u32;
                let b = a + stride as u32;
                indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }

        let mesh = Mesh {
            positions,
            normals: Vec::new(),
            indices,
        };
        let ppm = render(&mesh, 320, 320).expect("a picture");
        let out = std::env::temp_dir().join("coldmill-preview3d.ppm");
        std::fs::write(&out, ppm).unwrap();
        eprintln!("wrote {}", out.display());
    }
}
