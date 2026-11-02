//! 3D rendering of the city (replaces the legacy fixed-function OpenGL
//! `Renderer`, `GraphicManager` and display lists).
//!
//! The static scene (terrain, road ribbons, buildings) is rebuilt only when
//! the city changes. With [`Gpu`] available it lives in GPU buffers and
//! buildings are instanced; otherwise macroquad's immediate path is used.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use macroquad::models::{draw_mesh, Mesh, Vertex};
use macroquad::prelude::*;
use opencity_assets::{ac3d, Catalog, Look};
use opencity_core::{layer::TileKind, terrain::HEIGHT_MAX, City, Direction, GraphicCode, Rect, StructureCode, Terrain};

use crate::gpu::{Gpu, GpuMesh, Instance};
use crate::roads;

/// Direction of the sun used to bake shading into vertex colours.
const LIGHT: Vec3 = Vec3::new(-0.45, 0.85, -0.3);
const WATER_LEVEL: f32 = -0.05;
/// Models sit slightly above the ground to avoid z-fighting with the terrain.
const MODEL_LIFT: f32 = 0.015;
const ROAD_LIFT: f32 = 0.02;
const ROAD_HALF_WIDTH: f32 = 0.5;
/// Graphic code of the straight north-south road, whose texture the ribbons use.
const ROAD_TEXTURE_MODEL: GraphicCode = GraphicCode(154);
/// Vertices per static mesh chunk (u16 indices).
const CHUNK_VERTICES: usize = 60_000;

fn shade(normal: Vec3) -> f32 {
    0.55 + 0.45 * normal.normalize_or_zero().dot(LIGHT.normalize()).max(0.0)
}

fn rgba(c: [f32; 4], k: f32) -> [u8; 4] {
    [(c[0] * k * 255.0).min(255.0) as u8, (c[1] * k * 255.0).min(255.0) as u8, (c[2] * k * 255.0).min(255.0) as u8, (c[3] * 255.0) as u8]
}

/// Geometry kept on the CPU (fallback, vehicles) and, when possible, on the GPU.
#[derive(Default)]
struct Static {
    cpu: Vec<Mesh>,
    gpu: Vec<GpuMesh>,
}

impl Static {
    fn clear(&mut self, gpu: Option<&Gpu>) {
        self.cpu.clear();
        for m in self.gpu.drain(..) {
            if let Some(g) = gpu {
                g.delete(m);
            }
        }
    }

    fn push(&mut self, gpu: Option<&Gpu>, mesh: Mesh) {
        if let Some(g) = gpu {
            self.gpu.push(g.upload(&mesh.vertices, &mesh.indices, mesh.texture.as_ref()));
        }
        self.cpu.push(mesh);
    }

    fn draw(&self, gpu: Option<&Gpu>) {
        match gpu {
            Some(g) => self.gpu.iter().for_each(|m| g.draw(m, None)),
            None => self.cpu.iter().for_each(draw_mesh),
        }
    }
}

/// Splits triangle lists into u16-indexable meshes.
struct MeshBuilder {
    texture: Option<Texture2D>,
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
    done: Vec<Mesh>,
}

impl MeshBuilder {
    fn new(texture: Option<Texture2D>) -> Self {
        Self { texture, vertices: Vec::new(), indices: Vec::new(), done: Vec::new() }
    }

    fn quad(&mut self, v: [Vertex; 4]) {
        if self.vertices.len() + 4 > CHUNK_VERTICES {
            self.flush();
        }
        let base = self.vertices.len() as u16;
        self.vertices.extend(v);
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn flush(&mut self) {
        if !self.vertices.is_empty() {
            self.done.push(Mesh { vertices: std::mem::take(&mut self.vertices), indices: std::mem::take(&mut self.indices), texture: self.texture.clone() });
        }
    }

    fn finish(mut self) -> Vec<Mesh> {
        self.flush();
        self.done
    }
}

struct Model {
    cpu: Vec<Mesh>,
    gpu: Vec<GpuMesh>,
}

pub struct Renderer {
    catalog: Catalog,
    models: HashMap<GraphicCode, Option<Model>>,
    textures: HashMap<PathBuf, Texture2D>,
    /// Ground colour per height step, sampled from the legacy terrain texture.
    palette: Vec<[f32; 3]>,
    white: Texture2D,
    look: Look,
    gpu: Option<Gpu>,
    terrain: Static,
    terrain_key: Option<Vec<i8>>,
    roads: Static,
    /// (layer revision, terrain revision) the scene was built for.
    scene_key: Option<(u64, u64)>,
    terrain_revision: u64,
    /// Buildings grouped by model: GPU instance buffers, or CPU matrices.
    instances: Vec<(GraphicCode, Option<macroquad::miniquad::BufferId>, Vec<Instance>)>,
}

pub fn load_texture_file(path: &Path) -> Option<Texture2D> {
    let bytes = opencity_assets::read_bytes(path).map_err(|e| log::warn!("texture {}: {e}", path.display())).ok()?;
    let img = image::load_from_memory(&bytes).map_err(|e| log::warn!("texture {}: {e}", path.display())).ok()?.to_rgba8();
    let tex = Texture2D::from_rgba8(img.width() as u16, img.height() as u16, img.as_raw());
    tex.set_filter(FilterMode::Linear);
    Some(tex)
}

fn instance_matrix(i: &Instance) -> Mat4 {
    Mat4::from_cols(vec4(1.0, i[3], 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, i[4], 1.0, 0.0), vec4(i[0], i[1], i[2], 1.0))
}

impl Renderer {
    pub fn new(catalog: Catalog) -> Self {
        let palette = Self::terrain_palette(&catalog.path("graphism/texture/terrain_64x4096_texture.png"));
        let white = Texture2D::from_rgba8(1, 1, &[255, 255, 255, 255]);
        Self {
            catalog,
            models: HashMap::new(),
            textures: HashMap::new(),
            palette,
            white,
            look: Look::default(),
            gpu: Gpu::new(),
            terrain: Static::default(),
            terrain_key: None,
            roads: Static::default(),
            scene_key: None,
            terrain_revision: 0,
            instances: Vec::new(),
        }
    }

    /// Switches skin: new model files and colour treatment; everything is rebuilt lazily.
    pub fn set_skin(&mut self, catalog: Catalog, look: Look) {
        self.catalog = catalog;
        self.look = look;
        for (_, m) in self.models.drain() {
            if let (Some(m), Some(g)) = (m, &self.gpu) {
                m.gpu.into_iter().for_each(|x| g.delete(x));
            }
        }
        self.terrain_key = None;
        self.scene_key = None;
    }

    pub fn look(&self) -> &Look {
        &self.look
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

    fn model(&mut self, gc: GraphicCode) -> Option<&Model> {
        if !self.models.contains_key(&gc) {
            let loaded = self.load_model(gc);
            self.models.insert(gc, loaded);
        }
        self.models.get(&gc).and_then(Option::as_ref)
    }

    fn load_model(&mut self, gc: GraphicCode) -> Option<Model> {
        let path = self.catalog.get(gc)?.model.clone();
        let model = ac3d::load(&path).map_err(|e| log::warn!("{e}")).ok()?;
        let mut cpu = Vec::new();
        for m in &model.meshes {
            let texture = m.texture.as_deref().map(|p| self.texture(p));
            // u16 indices: split big meshes.
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
                            color: rgba(self.look.apply(m.colors[i]), shade(n)),
                            normal: n.extend(0.0),
                        });
                        (vertices.len() - 1) as u16
                    });
                    indices.push(ni);
                }
                cpu.push(Mesh { vertices, indices, texture: texture.clone() });
            }
        }
        let gpu = match &self.gpu {
            Some(g) => cpu.iter().map(|m| g.upload(&m.vertices, &m.indices, m.texture.as_ref())).collect(),
            None => Vec::new(),
        };
        Some(Model { cpu, gpu })
    }

    /// Rebuilds the terrain mesh when the heights changed.
    fn update_terrain(&mut self, t: &Terrain) {
        if self.terrain_key.as_deref() == Some(t.corner_heights()) {
            return;
        }
        self.terrain_key = Some(t.corner_heights().to_vec());
        self.terrain_revision += 1;
        self.terrain.clear(self.gpu.as_ref());
        let mut b = MeshBuilder::new(None);
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
                let v = |i: usize| {
                    let g = self.ground_color(c[i] as f32);
                    Vertex { position: p[i], uv: Vec2::ZERO, color: rgba(self.look.apply([g[0], g[1], g[2], 1.0]), k), normal: n.extend(0.0) }
                };
                b.quad([v(0), v(1), v(2), v(3)]);
            }
        }
        for m in b.finish() {
            self.terrain.push(self.gpu.as_ref(), m);
        }
    }

    /// Rebuilds building instances and road ribbons when the city changed.
    fn update_scene(&mut self, city: &City) {
        let key = (city.layer.revision(), self.terrain_revision);
        if self.scene_key == Some(key) {
            return;
        }
        self.scene_key = Some(key);
        let started = get_time();
        let t = &city.terrain;
        let mut groups: HashMap<GraphicCode, Vec<Instance>> = HashMap::new();
        for ((w, l), tile) in city.layer.tiles() {
            let TileKind::Main(s) = &tile.kind else { continue };
            if s.code == StructureCode::Road && roads::is_ribbon_tile(s.connections) {
                continue;
            }
            let c = t.tile_corners(w, l);
            groups.entry(s.graphic).or_default().push([w as f32, c[0] as f32 + MODEL_LIFT, l as f32, (c[3] - c[0]) as f32, (c[1] - c[0]) as f32]);
        }
        for (_, buf, _) in self.instances.drain(..) {
            if let (Some(b), Some(g)) = (buf, &self.gpu) {
                g.delete_buffer(b);
            }
        }
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_by_key(|(gc, _)| *gc);
        for (gc, list) in groups {
            if self.model(gc).is_none() {
                continue;
            }
            let buf = self.gpu.as_ref().map(|g| g.instances(&list));
            self.instances.push((gc, buf, list));
        }
        self.build_roads(city);
        log::debug!("scene rebuilt in {:.2} ms", (get_time() - started) * 1000.0);
    }

    fn road_texture(&mut self) -> Option<Texture2D> {
        let tex = self.model(ROAD_TEXTURE_MODEL)?.cpu.iter().find_map(|m| m.texture.clone())?;
        unsafe { get_internal_gl() }.quad_context.texture_set_wrap(
            tex.raw_miniquad_id(),
            macroquad::miniquad::TextureWrap::Repeat,
            macroquad::miniquad::TextureWrap::Repeat,
        );
        Some(tex)
    }

    /// Smooth, textured ribbons along road centre lines.
    fn build_roads(&mut self, city: &City) {
        let texture = self.road_texture();
        self.roads.clear(self.gpu.as_ref());
        let t = &city.terrain;
        let color = rgba(self.look.apply([1.0, 1.0, 1.0, 1.0]), shade(Vec3::Y));
        let mut b = MeshBuilder::new(texture);
        for (line, closed) in roads::centre_lines(city) {
            let pts = roads::smooth(&line, closed);
            let n = pts.len();
            let mut along = 0.0f32;
            let edge = |i: usize| -> (Vec3, Vec3) {
                let prev = if i > 0 {
                    pts[i - 1]
                } else if closed {
                    pts[n - 1]
                } else {
                    pts[i]
                };
                let next = if i + 1 < n {
                    pts[i + 1]
                } else if closed {
                    pts[0]
                } else {
                    pts[i]
                };
                let d = vec2(next[0] - prev[0], next[1] - prev[1]).normalize_or_zero();
                let side = vec2(-d.y, d.x) * ROAD_HALF_WIDTH;
                let p = vec2(pts[i][0], pts[i][1]);
                let at = |q: Vec2| vec3(q.x, height_at(t, q.x, q.y) + ROAD_LIFT, q.y);
                (at(p + side), at(p - side))
            };
            let segs = if closed { n } else { n - 1 };
            for i in 0..segs {
                let j = (i + 1) % n;
                let len = vec2(pts[j][0] - pts[i][0], pts[j][1] - pts[i][1]).length();
                let ((l0, r0), (l1, r1)) = (edge(i), edge(j));
                let v = |p: Vec3, u: f32, v: f32| Vertex { position: p, uv: vec2(u, v), color, normal: Vec4::ZERO };
                b.quad([v(l0, 0.0, along), v(r0, 1.0, along), v(r1, 1.0, along + len), v(l1, 0.0, along + len)]);
                along += len;
            }
        }
        for m in b.finish() {
            self.roads.push(self.gpu.as_ref(), m);
        }
    }

    fn draw_model_cpu(&mut self, gc: GraphicCode, matrix: Mat4) {
        if self.model(gc).is_none() {
            return;
        }
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.push_model_matrix(matrix);
        for m in &self.models[&gc].as_ref().unwrap().cpu {
            draw_mesh(m);
        }
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.pop_model_matrix();
    }

    pub fn draw_city(&mut self, city: &City, camera: &Camera3D) {
        self.update_terrain(&city.terrain);
        self.update_scene(city);
        match &self.gpu {
            Some(g) => {
                g.begin(camera.matrix());
                self.terrain.draw(Some(g));
                self.roads.draw(Some(g));
                for (gc, buf, list) in &self.instances {
                    if let (Some(Some(model)), Some(buf)) = (self.models.get(gc), buf) {
                        for m in &model.gpu {
                            g.draw(m, Some((*buf, list.len())));
                        }
                    }
                }
                g.end();
            }
            None => {
                self.terrain.draw(None);
                self.roads.draw(None);
                let items: Vec<(GraphicCode, Mat4)> =
                    self.instances.iter().flat_map(|(gc, _, list)| list.iter().map(move |i| (*gc, instance_matrix(i)))).collect();
                for (gc, m) in items {
                    self.draw_model_cpu(gc, m);
                }
            }
        }
        let vehicles: &[opencity_core::traffic::Vehicle] = if self.look.vehicles { &city.vehicles } else { &[] };
        for v in vehicles {
            let p = v.pose();
            let angle = match p.heading {
                Direction::South => 0.0f32,
                Direction::East => 90.0,
                Direction::North => 180.0,
                Direction::West => -90.0,
            }
            .to_radians();
            let m = Mat4::from_translation(vec3(p.x, p.y + 0.02, p.z)) * Mat4::from_rotation_y(angle) * Mat4::from_translation(vec3(-0.5, 0.0, -0.5));
            self.draw_model_cpu(v.kind.graphic(), m);
        }
        // Translucent water plane, drawn last.
        let (w, l) = (city.width() as f32, city.length() as f32);
        let c = self.look.water;
        draw_plane(vec3(w / 2.0, WATER_LEVEL, l / 2.0), vec2(w / 2.0, l / 2.0), None, Color::new(c[0], c[1], c[2], c[3]));
    }

    /// A data-layer tile drawn over everything (no depth test) so values
    /// stay readable under buildings.
    pub fn draw_data_overlay(&self, t: &Terrain, w: u32, l: u32, color: Color) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(false);
        self.draw_overlay(t, Rect::tile(w, l), color);
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(true);
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

/// Tile under the screen position: march the view ray through the band of
/// possible terrain heights only.
pub fn pick_tile(camera: &Camera3D, t: &Terrain, mouse: Vec2) -> Option<(u32, u32)> {
    let inv = camera.matrix().inverse();
    let ndc = vec2(mouse.x / screen_width() * 2.0 - 1.0, 1.0 - mouse.y / screen_height() * 2.0);
    let near = inv.project_point3(vec3(ndc.x, ndc.y, -1.0));
    let far = inv.project_point3(vec3(ndc.x, ndc.y, 1.0));
    let dir = (far - near).normalize();
    let (top, bottom) = (HEIGHT_MAX as f32 + 1.0, -(HEIGHT_MAX as f32) - 1.0);
    if dir.y >= 0.0 && near.y < bottom {
        return None;
    }
    // Skip to where the ray enters the height band.
    let mut p = if near.y > top && dir.y < 0.0 { near + dir * ((near.y - top) / -dir.y) } else { near };
    let step = 0.05;
    for _ in 0..20_000 {
        let inside = p.x >= 0.0 && p.z >= 0.0 && p.x < t.width() as f32 && p.z < t.length() as f32;
        if inside && p.y <= height_at(t, p.x, p.z).max(WATER_LEVEL) {
            return Some((p.x as u32, p.z as u32));
        }
        if p.y < bottom {
            return None;
        }
        p += dir * step;
    }
    None
}
