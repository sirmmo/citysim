//! 3D rendering of the city with macroquad (replaces the legacy fixed-function
//! OpenGL `Renderer`, `GraphicManager` and display lists).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use macroquad::models::{draw_mesh, Mesh, Vertex};
use macroquad::prelude::*;
use opencity_assets::{ac3d, Catalog};
use opencity_core::{layer::TileKind, terrain::HEIGHT_MAX, City, Direction, GraphicCode, Rect, Terrain};

/// Direction of the sun used to bake shading into vertex colours.
const LIGHT: Vec3 = Vec3::new(-0.45, 0.85, -0.3);
const WATER_LEVEL: f32 = -0.05;

fn shade(normal: Vec3) -> f32 {
    0.55 + 0.45 * normal.normalize_or_zero().dot(LIGHT.normalize()).max(0.0)
}

fn rgba(c: [f32; 4], k: f32) -> [u8; 4] {
    [(c[0] * k * 255.0).min(255.0) as u8, (c[1] * k * 255.0).min(255.0) as u8, (c[2] * k * 255.0).min(255.0) as u8, (c[3] * 255.0) as u8]
}

/// A model uploaded to the GPU.
pub struct GpuModel {
    meshes: Vec<Mesh>,
}

pub struct Renderer {
    catalog: Catalog,
    models: HashMap<GraphicCode, Option<GpuModel>>,
    textures: HashMap<PathBuf, Texture2D>,
    terrain: Vec<Mesh>,
    terrain_key: Option<Vec<i8>>,
    /// Ground colour per height step, sampled from the legacy terrain texture.
    palette: Vec<[f32; 3]>,
    white: Texture2D,
}

pub fn load_texture_file(path: &Path) -> Option<Texture2D> {
    let bytes = opencity_assets::read_bytes(path).map_err(|e| log::warn!("texture {}: {e}", path.display())).ok()?;
    let img = image::load_from_memory(&bytes).map_err(|e| log::warn!("texture {}: {e}", path.display())).ok()?.to_rgba8();
    let tex = Texture2D::from_rgba8(img.width() as u16, img.height() as u16, img.as_raw());
    tex.set_filter(FilterMode::Linear);
    Some(tex)
}

impl Renderer {
    pub fn new(catalog: Catalog) -> Self {
        let palette = Self::terrain_palette(&catalog.path("graphism/texture/terrain_64x4096_texture.png"));
        let white = Texture2D::from_rgba8(1, 1, &[255, 255, 255, 255]);
        Self { catalog, models: HashMap::new(), textures: HashMap::new(), terrain: Vec::new(), terrain_key: None, palette, white }
    }

    /// The legacy terrain texture is a 3D texture: 64 slices of 64x64 texels
    /// stacked vertically, indexed by height. We keep the mean colour of each slice.
    fn terrain_palette(path: &Path) -> Vec<[f32; 3]> {
        let fallback = || (0..64).map(|i| if i < 9 { [0.2, 0.3, 0.6] } else { [0.3 + i as f32 / 200.0, 0.5, 0.25] }).collect();
        let Some(img) = opencity_assets::read_bytes(path).ok().and_then(|b| image::load_from_memory(&b).ok()) else { return fallback() };
        let img = img.to_rgb8();
        let slice = img.width();
        let slices = img.height() / slice;
        (0..slices)
            .map(|s| {
                let mut sum = [0f32; 3];
                for y in s * slice..(s + 1) * slice {
                    for x in 0..slice {
                        let p = img.get_pixel(x, y);
                        for c in 0..3 {
                            sum[c] += p[c] as f32;
                        }
                    }
                }
                let n = (slice * slice) as f32 * 255.0;
                [sum[0] / n, sum[1] / n, sum[2] / n]
            })
            .collect()
    }

    fn ground_color(&self, h: f32) -> [f32; 3] {
        let max = HEIGHT_MAX as f32;
        let r = if h >= 0.0 { (h / max) * 55.0 / 64.0 + 9.0 / 64.0 } else { (max + h) / max * 8.0 / 64.0 };
        let i = (r * self.palette.len() as f32).clamp(0.0, self.palette.len() as f32 - 1.0) as usize;
        self.palette[i]
    }

    fn texture(&mut self, path: &Path) -> Texture2D {
        if let Some(t) = self.textures.get(path) {
            return t.clone();
        }
        let t = load_texture_file(path).unwrap_or_else(|| self.white.clone());
        self.textures.insert(path.to_owned(), t.clone());
        t
    }

    pub fn ui_texture(&mut self, rel: &str) -> Texture2D {
        let p = self.catalog.path(rel);
        self.texture(&p)
    }

    fn model(&mut self, gc: GraphicCode) -> Option<&GpuModel> {
        if !self.models.contains_key(&gc) {
            let loaded = self.load_model(gc);
            self.models.insert(gc, loaded);
        }
        self.models.get(&gc).and_then(Option::as_ref)
    }

    fn load_model(&mut self, gc: GraphicCode) -> Option<GpuModel> {
        let path = self.catalog.get(gc)?.model.clone();
        let model = ac3d::load(&path).map_err(|e| log::warn!("{e}")).ok()?;
        let mut meshes = Vec::new();
        for m in &model.meshes {
            let texture = m.texture.as_deref().map(|p| self.texture(p));
            // macroquad meshes use u16 indices; split big meshes.
            for chunk in m.indices.chunks(3 * 20_000) {
                let mut vertices = Vec::new();
                let mut remap = HashMap::new();
                let mut indices = Vec::with_capacity(chunk.len());
                for &i in chunk {
                    let ni = *remap.entry(i).or_insert_with(|| {
                        let i = i as usize;
                        let n = Vec3::from(m.normals[i]);
                        vertices.push(Vertex {
                            position: Vec3::from(m.positions[i]),
                            uv: Vec2::from(m.uvs[i]),
                            color: rgba(m.colors[i], shade(n)),
                            normal: n.extend(0.0),
                        });
                        (vertices.len() - 1) as u16
                    });
                    indices.push(ni);
                }
                meshes.push(Mesh { vertices, indices, texture: texture.clone() });
            }
        }
        Some(GpuModel { meshes })
    }

    /// Rebuilds the terrain mesh when the heights changed.
    fn update_terrain(&mut self, t: &Terrain) {
        if self.terrain_key.as_deref() == Some(t.corner_heights()) {
            return;
        }
        self.terrain_key = Some(t.corner_heights().to_vec());
        self.terrain.clear();
        let (mut vertices, mut indices) = (Vec::new(), Vec::new());
        for l in 0..t.length() {
            for w in 0..t.width() {
                let c = t.tile_corners(w, l);
                let p = [
                    vec3(w as f32, c[0] as f32, l as f32),
                    vec3(w as f32, c[1] as f32, l as f32 + 1.0),
                    vec3(w as f32 + 1.0, c[2] as f32, l as f32 + 1.0),
                    vec3(w as f32 + 1.0, c[3] as f32, l as f32),
                ];
                let n = (p[1] - p[0]).cross(p[3] - p[0]) + (p[3] - p[2]).cross(p[1] - p[2]);
                let k = shade(n) * if (w + l) % 2 == 0 { 1.0 } else { 0.96 };
                let base = vertices.len() as u16;
                for (i, v) in p.iter().enumerate() {
                    let g = self.ground_color(c[i] as f32);
                    vertices.push(Vertex { position: *v, uv: vec2(0.0, 0.0), color: rgba([g[0], g[1], g[2], 1.0], k), normal: n.extend(0.0) });
                }
                indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                if vertices.len() > 60_000 {
                    self.terrain.push(Mesh { vertices: std::mem::take(&mut vertices), indices: std::mem::take(&mut indices), texture: None });
                }
            }
        }
        if !vertices.is_empty() {
            self.terrain.push(Mesh { vertices, indices, texture: None });
        }
    }

    fn draw_model(&mut self, gc: GraphicCode, matrix: Mat4) {
        if self.model(gc).is_none() {
            return;
        }
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.push_model_matrix(matrix);
        for m in &self.models[&gc].as_ref().unwrap().meshes {
            draw_mesh(m);
        }
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.pop_model_matrix();
    }

    /// Model matrix that shears a model onto a sloped tile (legacy `Model::DisplayList`).
    fn tile_matrix(t: &Terrain, w: u32, l: u32) -> Mat4 {
        let c = t.tile_corners(w, l);
        let (dx, dz) = ((c[3] - c[0]) as f32, (c[1] - c[0]) as f32);
        Mat4::from_cols(vec4(1.0, dx, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, dz, 1.0, 0.0), vec4(w as f32, c[0] as f32 + 0.015, l as f32, 1.0))
    }

    pub fn draw_city(&mut self, city: &City) {
        self.update_terrain(&city.terrain);
        for m in &self.terrain {
            draw_mesh(m);
        }
        let items: Vec<(u32, u32, GraphicCode)> = city
            .layer
            .tiles()
            .filter_map(|((w, l), t)| match &t.kind {
                TileKind::Main(s) => Some((w, l, s.graphic)),
                TileKind::Part { .. } => None,
            })
            .collect();
        for (w, l, gc) in items {
            self.draw_model(gc, Self::tile_matrix(&city.terrain, w, l));
        }
        for v in &city.vehicles {
            let p = v.pose();
            let angle = match p.heading {
                Direction::South => 0.0f32,
                Direction::East => 90.0,
                Direction::North => 180.0,
                Direction::West => -90.0,
            }
            .to_radians();
            let m = Mat4::from_translation(vec3(p.x, p.y + 0.02, p.z)) * Mat4::from_rotation_y(angle) * Mat4::from_translation(vec3(-0.5, 0.0, -0.5));
            self.draw_model(v.kind.graphic(), m);
        }
        // Translucent water plane, drawn last.
        let (w, l) = (city.width() as f32, city.length() as f32);
        draw_plane(vec3(w / 2.0, WATER_LEVEL, l / 2.0), vec2(w / 2.0, l / 2.0), None, Color::new(0.15, 0.35, 0.75, 0.55));
    }

    /// Translucent overlay hugging the terrain over `r`.
    pub fn draw_overlay(&self, t: &Terrain, r: Rect, color: Color) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.texture(None);
        gl.draw_mode(DrawMode::Triangles);
        let col: [u8; 4] = color.into();
        let lift = 0.04;
        for (w, l) in r.tiles() {
            if w >= t.width() || l >= t.length() {
                continue;
            }
            let c = t.tile_corners(w, l);
            let v = |x: f32, h: i8, z: f32| Vertex { position: vec3(x, h as f32 + lift, z), uv: Vec2::ZERO, color: col, normal: Vec4::ZERO };
            let (x, z) = (w as f32, l as f32);
            gl.geometry(&[v(x, c[0], z), v(x, c[1], z + 1.0), v(x + 1.0, c[2], z + 1.0), v(x + 1.0, c[3], z)], &[0, 1, 2, 0, 2, 3]);
        }
    }
}

/// Bilinear terrain height at a map position (tile units).
pub fn height_at(t: &Terrain, x: f32, z: f32) -> f32 {
    let w = (x.floor() as i64).clamp(0, t.width() as i64 - 1) as u32;
    let l = (z.floor() as i64).clamp(0, t.length() as i64 - 1) as u32;
    let (fx, fz) = ((x - w as f32).clamp(0.0, 1.0), (z - l as f32).clamp(0.0, 1.0));
    let c = t.tile_corners(w, l).map(|h| h as f32);
    let north = c[0] + (c[3] - c[0]) * fx;
    let south = c[1] + (c[2] - c[1]) * fx;
    north + (south - north) * fz
}

/// Tile under the screen position, by marching the view ray over the terrain.
pub fn pick_tile(camera: &Camera3D, t: &Terrain, mouse: Vec2) -> Option<(u32, u32)> {
    let inv = camera.matrix().inverse();
    let ndc = vec2(mouse.x / screen_width() * 2.0 - 1.0, 1.0 - mouse.y / screen_height() * 2.0);
    let near = inv.project_point3(vec3(ndc.x, ndc.y, -1.0));
    let far = inv.project_point3(vec3(ndc.x, ndc.y, 1.0));
    let dir = (far - near).normalize();
    let mut p = near;
    let step = 0.05;
    for _ in 0..40_000 {
        let inside = p.x >= 0.0 && p.z >= 0.0 && p.x < t.width() as f32 && p.z < t.length() as f32;
        if inside && p.y <= height_at(t, p.x, p.z).max(WATER_LEVEL) {
            return Some((p.x as u32, p.z as u32));
        }
        if p.y < -20.0 {
            return None;
        }
        p += dir * step;
    }
    None
}
