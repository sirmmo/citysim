//! Vehicles without a model in the data files, built from boxes: people on
//! foot, handcarts and horse carts. Same frame as the legacy car models: a
//! tile from 0 to 1, driving towards +z in the right lane (x ≈ 0.3).

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use opencity_core::traffic::VehicleKind;

const WOOD: [f32; 3] = [0.45, 0.3, 0.17];
const WHEEL: [f32; 3] = [0.25, 0.18, 0.1];
const SACK: [f32; 3] = [0.82, 0.74, 0.55];
const SKIN: [f32; 3] = [0.87, 0.68, 0.52];
const HORSE: [f32; 3] = [0.42, 0.27, 0.15];
const CLOTH: [[f32; 3]; 3] = [[0.55, 0.2, 0.15], [0.25, 0.32, 0.5], [0.35, 0.45, 0.25]];

#[derive(Default)]
struct Builder {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
}

impl Builder {
    /// An axis-aligned box from `a` to `b`, faces shaded by their direction.
    fn cuboid(&mut self, a: Vec3, b: Vec3, c: [f32; 3]) {
        let faces: [([Vec3; 4], f32); 6] = [
            ([vec3(a.x, b.y, a.z), vec3(b.x, b.y, a.z), vec3(b.x, b.y, b.z), vec3(a.x, b.y, b.z)], 1.0),
            ([vec3(a.x, a.y, a.z), vec3(a.x, a.y, b.z), vec3(b.x, a.y, b.z), vec3(b.x, a.y, a.z)], 0.45),
            ([vec3(a.x, a.y, b.z), vec3(a.x, b.y, b.z), vec3(b.x, b.y, b.z), vec3(b.x, a.y, b.z)], 0.85),
            ([vec3(a.x, a.y, a.z), vec3(b.x, a.y, a.z), vec3(b.x, b.y, a.z), vec3(a.x, b.y, a.z)], 0.6),
            ([vec3(b.x, a.y, a.z), vec3(b.x, a.y, b.z), vec3(b.x, b.y, b.z), vec3(b.x, b.y, a.z)], 0.75),
            ([vec3(a.x, a.y, a.z), vec3(a.x, b.y, a.z), vec3(a.x, b.y, b.z), vec3(a.x, a.y, b.z)], 0.7),
        ];
        for (q, shade) in faces {
            let base = self.vertices.len() as u16;
            let color = Color::new(c[0] * shade, c[1] * shade, c[2] * shade, 1.0);
            for p in q {
                self.vertices.push(Vertex { position: p, uv: Vec2::ZERO, color: color.into(), normal: Vec4::ZERO });
            }
            self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// A person standing at (x, z), facing +z.
    fn person(&mut self, x: f32, z: f32, cloth: [f32; 3]) {
        self.cuboid(vec3(x - 0.025, 0.0, z - 0.015), vec3(x + 0.025, 0.07, z + 0.015), [0.3, 0.25, 0.2]);
        self.cuboid(vec3(x - 0.03, 0.07, z - 0.02), vec3(x + 0.03, 0.15, z + 0.02), cloth);
        self.cuboid(vec3(x - 0.018, 0.15, z - 0.018), vec3(x + 0.018, 0.188, z + 0.018), SKIN);
    }

    /// A pair of wheels on an axle across `x0..x1` at `z`.
    fn wheels(&mut self, x0: f32, x1: f32, z: f32, r: f32) {
        for x in [x0 - 0.015, x1] {
            self.cuboid(vec3(x, 0.0, z - r), vec3(x + 0.015, 2.0 * r, z + r), WHEEL);
        }
    }

    /// A cart bed with its load, from `z0` to `z1`.
    fn cart(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, r: f32) {
        let y = 1.4 * r;
        self.wheels(x0, x1, (z0 + z1) / 2.0, r);
        self.cuboid(vec3(x0, y, z0), vec3(x1, y + 0.025, z1), WOOD);
        let m = (z1 - z0) * 0.15;
        self.cuboid(vec3(x0 + 0.02, y + 0.025, z0 + m), vec3(x1 - 0.02, y + 0.075, z1 - m), SACK);
    }

    fn build(self) -> Mesh {
        Mesh { vertices: self.vertices, indices: self.indices, texture: None }
    }
}

/// The mesh of a vehicle drawn procedurally, `None` for those with a model.
pub fn mesh(kind: VehicleKind) -> Option<Mesh> {
    let mut b = Builder::default();
    match kind {
        VehicleKind::Walker => {
            b.person(0.27, 0.45, CLOTH[0]);
            b.person(0.35, 0.53, CLOTH[1]);
            b.person(0.3, 0.62, CLOTH[2]);
        }
        VehicleKind::Handcart => {
            b.cart(0.22, 0.38, 0.45, 0.68, 0.045);
            // Shafts back to the person pushing it.
            for x in [0.23, 0.36] {
                b.cuboid(vec3(x, 0.08, 0.35), vec3(x + 0.01, 0.095, 0.45), WOOD);
            }
            b.person(0.3, 0.33, CLOTH[0]);
        }
        VehicleKind::HorseCart => {
            b.cart(0.2, 0.4, 0.25, 0.52, 0.06);
            for x in [0.24, 0.355] {
                b.cuboid(vec3(x, 0.09, 0.52), vec3(x + 0.01, 0.105, 0.62), WOOD);
            }
            b.person(0.3, 0.3, CLOTH[1]);
            // The horse: legs, body, neck and head.
            for (x, z) in [(0.27, 0.6), (0.315, 0.6), (0.27, 0.71), (0.315, 0.71)] {
                b.cuboid(vec3(x, 0.0, z), vec3(x + 0.015, 0.08, z + 0.015), HORSE);
            }
            b.cuboid(vec3(0.265, 0.08, 0.58), vec3(0.335, 0.15, 0.74), HORSE);
            b.cuboid(vec3(0.28, 0.13, 0.72), vec3(0.32, 0.2, 0.77), HORSE);
            b.cuboid(vec3(0.285, 0.17, 0.76), vec3(0.315, 0.2, 0.82), [0.3, 0.2, 0.12]);
        }
        VehicleKind::Bus => {
            // A long single-decker: yellow body, a band of windows, dark
            // roof edge and wheels, the windscreen at the front (+z).
            let (x0, x1, z0, z1) = (0.21, 0.39, 0.12, 0.88);
            b.wheels(x0 + 0.01, x1 - 0.01, 0.25, 0.04);
            b.wheels(x0 + 0.01, x1 - 0.01, 0.75, 0.04);
            b.cuboid(vec3(x0, 0.035, z0), vec3(x1, 0.12, z1), [0.92, 0.72, 0.18]);
            b.cuboid(vec3(x0 - 0.002, 0.12, z0 + 0.03), vec3(x1 + 0.002, 0.2, z1 - 0.06), [0.32, 0.45, 0.55]);
            b.cuboid(vec3(x0, 0.12, z1 - 0.06), vec3(x1, 0.2, z1), [0.45, 0.6, 0.7]);
            b.cuboid(vec3(x0, 0.12, z0), vec3(x1, 0.2, z0 + 0.03), [0.92, 0.72, 0.18]);
            b.cuboid(vec3(x0, 0.2, z0), vec3(x1, 0.23, z1), [0.95, 0.95, 0.93]);
        }
        _ => return None,
    }
    Some(b.build())
}

/// An airliner for the airports' flights: nose towards +z, about a tile
/// and a half long, centred on its origin.
pub fn plane() -> Mesh {
    let mut b = Builder::default();
    let (white, grey) = ([0.93, 0.93, 0.95], [0.72, 0.72, 0.76]);
    b.cuboid(vec3(-0.09, 0.0, -0.7), vec3(0.09, 0.18, 0.6), white);
    b.cuboid(vec3(-0.06, 0.03, 0.6), vec3(0.06, 0.15, 0.78), white);
    b.cuboid(vec3(-0.62, 0.05, -0.12), vec3(0.62, 0.08, 0.14), grey);
    b.cuboid(vec3(-0.3, 0.1, -0.7), vec3(0.3, 0.12, -0.55), grey);
    b.cuboid(vec3(-0.012, 0.18, -0.7), vec3(0.012, 0.48, -0.5), [0.8, 0.2, 0.2]);
    for x in [-0.33, 0.25] {
        b.cuboid(vec3(x, 0.0, -0.08), vec3(x + 0.08, 0.06, 0.12), [0.4, 0.42, 0.45]);
    }
    b.cuboid(vec3(-0.07, 0.11, 0.3), vec3(0.07, 0.15, 0.62), [0.3, 0.4, 0.5]);
    b.build()
}

/// Scale from Kenney kit units to tiles: a sedan is about 0.4 of a tile.
const KIT_SCALE: f32 = 0.155;

/// Meshes for an `.obj` vehicle (Kenney kits), in the same frame as the
/// legacy cars: centred on the right lane (x ≈ 0.3) of a 0..1 tile, nose
/// towards +z, faces shaded by their direction. Long models (ships,
/// carriages) shrink to fit a tile.
pub fn obj_meshes(obj: &opencity_assets::obj::ObjMesh, texture: Option<Texture2D>) -> Vec<Mesh> {
    let (lo, hi) = obj.bounds();
    let length = (hi[2] - lo[2]).max(1e-3);
    let s = KIT_SCALE.min(0.9 / length);
    let (cx, cz) = ((lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0);
    let light = vec3(0.4, 1.0, 0.3).normalize();
    let mut out = Vec::new();
    let mut b = Builder::default();
    for t in &obj.triangles {
        if b.vertices.len() + 3 > u16::MAX as usize {
            out.push(Mesh { vertices: std::mem::take(&mut b.vertices), indices: std::mem::take(&mut b.indices), texture: texture.clone() });
        }
        let p = t.map(|i| {
            let q = obj.positions[i as usize];
            vec3(0.3 + (q[0] - cx) * s, q[1] * s, 0.5 + (q[2] - cz) * s)
        });
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let k = 0.55 + 0.45 * n.dot(light).max(0.0);
        let base = b.vertices.len() as u16;
        for (j, pos) in p.into_iter().enumerate() {
            let uv = obj.uvs[t[j] as usize];
            b.vertices.push(Vertex { position: pos, uv: vec2(uv[0], uv[1]), color: Color::new(k, k, k, 1.0).into(), normal: n.extend(0.0) });
        }
        b.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    if !b.indices.is_empty() {
        out.push(Mesh { vertices: b.vertices, indices: b.indices, texture });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vehicle_has_a_look() {
        for k in VehicleKind::ALL {
            assert!(k.graphic().is_some() != mesh(k).is_some(), "{k:?}");
        }
        let m = mesh(VehicleKind::HorseCart).unwrap();
        assert!(m.vertices.iter().all(|v| (0.0..=1.0).contains(&v.position.x) && (0.0..=1.0).contains(&v.position.z)));
    }
}
