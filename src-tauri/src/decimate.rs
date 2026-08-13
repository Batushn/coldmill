//! Reducing a mesh, and moving where its origin sits.
//!
//! The quality control in the corner means something for pictures and sound
//! already; for a 3D model it means how many triangles survive. High leaves
//! the mesh exactly as it was — a reduction nobody asked for is worse than no
//! reduction at all — while the other two tiers trade detail for size.
//!
//! The method is vertex clustering: the model is divided into a grid, every
//! vertex in a cell collapses to one, and triangles whose corners end up in
//! the same cell disappear. It is not the sharpest reduction available —
//! quadric error metrics keep silhouettes better — but it is a few dozen
//! lines, it cannot fail on a mesh that is not watertight, and it degrades
//! predictably rather than dramatically. For "make this smaller before I put
//! it on a website", that is the right trade.

use std::collections::HashMap;

use crate::mesh::Mesh;
use crate::model::Quality;

/// Roughly what fraction of the triangles each tier aims to keep.
pub fn ratio(quality: Quality) -> f64 {
    match quality {
        Quality::Small => 0.25,
        Quality::Balanced => 0.6,
        // Not 1.0-with-a-rebuild: an untouched mesh is returned untouched, so
        // High cannot cost anything.
        Quality::High => 1.0,
    }
}

/// Where the origin of the exported model should sit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pivot {
    /// Leave the coordinates exactly as they came in.
    #[default]
    Keep,
    /// The middle of the bounding box.
    Center,
    /// Centred left-to-right and front-to-back, but sitting on the floor —
    /// what you want for anything that has to stand on a surface.
    CenterBottom,
}

/// What the 3D side of an edit can ask for.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MeshEdit {
    pub pivot: Pivot,
}

impl MeshEdit {
    pub fn is_noop(&self) -> bool {
        self.pivot == Pivot::Keep
    }
}

/// The triangle count a conversion is expected to produce.
///
/// Clustering cannot promise an exact number — how many triangles collapse
/// depends on how the geometry happens to fall across the grid — so this is
/// what the UI shows with a tilde in front of it.
pub fn expected_triangles(source: usize, quality: Quality) -> usize {
    ((source as f64) * ratio(quality)).round() as usize
}

/// Reduces a mesh towards `ratio(quality)`, in place.
pub fn apply(mesh: &mut Mesh, quality: Quality) {
    let keep = ratio(quality);
    if keep >= 1.0 || mesh.indices.len() < 3 {
        return;
    }

    // The grid is cubed, so halving the triangle count means dividing each
    // axis by the cube root of two, not by two. Starting from the vertex
    // count rather than a fixed number keeps a coarse mesh from being
    // shattered and a dense one from being left alone.
    let target = (mesh.positions.len() as f64 * keep).max(8.0);
    let divisions = target.cbrt().round().max(2.0) as usize;

    let (min, max) = bounds(&mesh.positions);
    let extent = [
        (max[0] - min[0]).max(f32::EPSILON),
        (max[1] - min[1]).max(f32::EPSILON),
        (max[2] - min[2]).max(f32::EPSILON),
    ];

    // Each cell keeps the average of the vertices that landed in it, rather
    // than one of them: averaging keeps the surface where it was instead of
    // pulling it towards an arbitrary corner.
    let mut cells: HashMap<(usize, usize, usize), (usize, [f64; 3])> = HashMap::new();
    let mut cell_of = Vec::with_capacity(mesh.positions.len());

    for point in &mesh.positions {
        let key = (
            cell(point[0], min[0], extent[0], divisions),
            cell(point[1], min[1], extent[1], divisions),
            cell(point[2], min[2], extent[2], divisions),
        );
        let entry = cells.entry(key).or_insert((0, [0.0; 3]));
        entry.0 += 1;
        for (sum, value) in entry.1.iter_mut().zip(point) {
            *sum += *value as f64;
        }
        cell_of.push(key);
    }

    // A stable order, so the same mesh always produces the same file. A
    // HashMap's iteration order is not, and a converter whose output changes
    // between runs is a converter nobody can check.
    let mut keys: Vec<(usize, usize, usize)> = cells.keys().copied().collect();
    keys.sort_unstable();

    let mut index_of: HashMap<(usize, usize, usize), u32> = HashMap::new();
    let mut positions = Vec::with_capacity(keys.len());
    for key in keys {
        let (count, sum) = cells[&key];
        index_of.insert(key, positions.len() as u32);
        positions.push([
            (sum[0] / count as f64) as f32,
            (sum[1] / count as f64) as f32,
            (sum[2] / count as f64) as f32,
        ]);
    }

    let mut indices = Vec::with_capacity(mesh.indices.len());
    for triangle in mesh.indices.chunks_exact(3) {
        let corners: Vec<u32> = triangle
            .iter()
            .filter_map(|old| cell_of.get(*old as usize))
            .filter_map(|key| index_of.get(key).copied())
            .collect();
        // Two corners in the same cell means the triangle has collapsed to a
        // line. Dropping it is the whole reduction.
        if corners.len() == 3
            && corners[0] != corners[1]
            && corners[1] != corners[2]
            && corners[0] != corners[2]
        {
            indices.extend_from_slice(&corners);
        }
    }

    // A grid coarse enough to erase the model is worse than no reduction, and
    // there is no honest way to hand back nothing.
    if indices.is_empty() {
        return;
    }

    mesh.positions = positions;
    mesh.indices = indices;
    // The old normals belonged to the old vertices.
    mesh.normals.clear();
}

/// Moves the model so its origin sits where `pivot` asks.
pub fn recenter(mesh: &mut Mesh, pivot: Pivot) {
    if pivot == Pivot::Keep || mesh.positions.is_empty() {
        return;
    }

    let (min, max) = bounds(&mesh.positions);
    let offset = [
        (min[0] + max[0]) / 2.0,
        match pivot {
            Pivot::CenterBottom => min[1],
            _ => (min[1] + max[1]) / 2.0,
        },
        (min[2] + max[2]) / 2.0,
    ];

    for point in &mut mesh.positions {
        for axis in 0..3 {
            point[axis] -= offset[axis];
        }
    }
    // Normals are directions, not places, so a translation leaves them alone.
}

fn cell(value: f32, min: f32, extent: f32, divisions: usize) -> usize {
    let at = ((value - min) / extent * divisions as f32) as usize;
    at.min(divisions - 1)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A dense grid of triangles: enough vertices that clustering has real
    /// work to do.
    fn sheet(side: usize) -> Mesh {
        let mut positions = Vec::new();
        for y in 0..=side {
            for x in 0..=side {
                positions.push([
                    x as f32 / side as f32,
                    // A gentle dome, so the mesh is not perfectly flat.
                    ((x + y) as f32 / side as f32).sin() * 0.2,
                    y as f32 / side as f32,
                ]);
            }
        }
        let mut indices = Vec::new();
        let stride = side + 1;
        for y in 0..side {
            for x in 0..side {
                let a = (y * stride + x) as u32;
                let b = a + stride as u32;
                indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        Mesh {
            positions,
            normals: Vec::new(),
            indices,
        }
    }

    #[test]
    fn high_quality_leaves_the_mesh_exactly_as_it_was() {
        let original = sheet(20);
        let mut mesh = sheet(20);
        apply(&mut mesh, Quality::High);
        assert_eq!(mesh.positions, original.positions);
        assert_eq!(mesh.indices, original.indices);
    }

    #[test]
    fn lower_tiers_mean_fewer_triangles() {
        let full = sheet(30).indices.len();
        let mut balanced = sheet(30);
        apply(&mut balanced, Quality::Balanced);
        let mut small = sheet(30);
        apply(&mut small, Quality::Small);

        assert!(balanced.indices.len() < full, "balanced should reduce");
        assert!(
            small.indices.len() < balanced.indices.len(),
            "small should reduce further: {} vs {}",
            small.indices.len(),
            balanced.indices.len()
        );
    }

    #[test]
    fn a_reduced_mesh_stays_where_it_was() {
        // Clustering must not drift the model: averaging each cell keeps the
        // surface in place, and a mesh that moved would not line up with
        // anything it was modelled against.
        let mut mesh = sheet(30);
        let (before_min, before_max) = bounds(&mesh.positions);
        apply(&mut mesh, Quality::Small);
        let (after_min, after_max) = bounds(&mesh.positions);
        for axis in 0..3 {
            assert!(
                (after_min[axis] - before_min[axis]).abs() < 0.1
                    && (after_max[axis] - before_max[axis]).abs() < 0.1,
                "axis {axis} moved: {before_min:?}..{before_max:?} -> {after_min:?}..{after_max:?}"
            );
        }
    }

    #[test]
    fn reduction_is_the_same_every_time() {
        // A HashMap iterates in a different order per run; the output must
        // not, or nobody could check a conversion against a previous one.
        let mut first = sheet(20);
        let mut second = sheet(20);
        apply(&mut first, Quality::Small);
        apply(&mut second, Quality::Small);
        assert_eq!(first.positions, second.positions);
        assert_eq!(first.indices, second.indices);
    }

    #[test]
    fn a_mesh_too_small_to_reduce_survives() {
        let mut triangle = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2],
        };
        apply(&mut triangle, Quality::Small);
        assert_eq!(triangle.indices.len(), 3, "no triangles is not an answer");
    }

    #[test]
    fn centering_puts_the_middle_at_the_origin() {
        let mut mesh = Mesh {
            positions: vec![[10.0, 4.0, 10.0], [12.0, 8.0, 14.0]],
            normals: Vec::new(),
            indices: Vec::new(),
        };
        recenter(&mut mesh, Pivot::Center);
        let (min, max) = bounds(&mesh.positions);
        for axis in 0..3 {
            assert!(
                (min[axis] + max[axis]).abs() < 1e-5,
                "axis {axis} off centre"
            );
        }
    }

    #[test]
    fn sitting_on_the_floor_leaves_the_base_at_zero() {
        let mut mesh = Mesh {
            positions: vec![[10.0, 4.0, 10.0], [12.0, 8.0, 14.0]],
            normals: Vec::new(),
            indices: Vec::new(),
        };
        recenter(&mut mesh, Pivot::CenterBottom);
        let (min, max) = bounds(&mesh.positions);
        assert!(min[1].abs() < 1e-5, "the base should rest on zero: {min:?}");
        assert!(
            (min[0] + max[0]).abs() < 1e-5,
            "still centred left to right"
        );
        assert!(
            (min[2] + max[2]).abs() < 1e-5,
            "still centred front to back"
        );
    }

    #[test]
    fn keeping_the_pivot_changes_nothing() {
        let original = sheet(4);
        let mut mesh = sheet(4);
        recenter(&mut mesh, Pivot::Keep);
        assert_eq!(mesh.positions, original.positions);
    }
}
