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
use opencity_assets::{Catalog, Look};
use opencity_core::{layer::TileKind, terrain::HEIGHT_MAX, City, Direction, GraphicCode, Rect, RoadKind, StructureCode, Terrain};

use crate::gpu::{Gpu, GpuMesh, Instance};
use crate::roads;

/// Direction of the sun used to bake shading into vertex colours.
const LIGHT: Vec3 = Vec3::new(-0.45, 0.85, -0.3);
const WATER_LEVEL: f32 = -0.05;
/// Models sit slightly above the ground to avoid z-fighting with the terrain.
const MODEL_LIFT: f32 = 0.015;
const ROAD_LIFT: f32 = 0.02;
/// Height of a bridge deck (roads and tracks over water).
const BRIDGE_DECK: f32 = 0.12;
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

    fn triangle(&mut self, v: [Vertex; 3]) {
        if self.vertices.len() + 3 > CHUNK_VERTICES {
            self.flush();
        }
        let base = self.vertices.len() as u16;
        self.vertices.extend(v);
        self.indices.extend([base, base + 1, base + 2]);
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

/// A model and its variant (composed models have several, see
/// `opencity_assets::compose`; others only 0).
type ModelKey = (GraphicCode, u16);

pub struct Renderer {
    catalog: Catalog,
    models: HashMap<ModelKey, Option<Model>>,
    /// Variants of each composed model (1 for the others).
    variants: HashMap<GraphicCode, u16>,
    /// Vehicles drawn from boxes (walkers, carts), built on first use.
    vehicle_meshes: HashMap<opencity_core::traffic::VehicleKind, Option<Mesh>>,
    /// The airliner flying from each airport (built on first use).
    plane: Option<Mesh>,
    /// `.obj` vehicle models by path under `<data>/models/` (None: missing).
    obj_meshes: HashMap<String, Option<Vec<Mesh>>>,
    /// The skin's bridge pier model and its colour map, once looked up
    /// (inner None: the skin has none, or it failed to load).
    pier: Option<Option<(opencity_assets::obj::ObjMesh, Texture2D)>>,
    textures: HashMap<PathBuf, Texture2D>,
    /// Ground colour per height step, sampled from the legacy terrain texture.
    palette: Vec<[f32; 3]>,
    white: Texture2D,
    look: Look,
    gpu: Option<Gpu>,
    terrain: Static,
    terrain_key: Option<Vec<i8>>,
    roads: Static,
    /// Generated surfaces of the road kinds other than streets.
    road_surfaces: HashMap<RoadKind, Texture2D>,
    /// (layer revision, terrain revision) the scene was built for.
    scene_key: Option<(u64, u64)>,
    terrain_revision: u64,
    /// Light, sky and ground of the hour, season and weather.
    pub atmosphere: Atmosphere,
    /// Buildings grouped by model: GPU instance buffers, or CPU matrices.
    instances: Vec<(ModelKey, Option<macroquad::miniquad::BufferId>, Vec<Instance>)>,
}

/// Makes a texture repeat (road surfaces run along the whole ribbon).
fn repeat(tex: &Texture2D) {
    unsafe { get_internal_gl() }.quad_context.texture_set_wrap(
        tex.raw_miniquad_id(),
        macroquad::miniquad::TextureWrap::Repeat,
        macroquad::miniquad::TextureWrap::Repeat,
    );
}

pub fn load_texture_file(path: &Path) -> Option<Texture2D> {
    let bytes = opencity_assets::read_bytes(path).map_err(|e| log::warn!("texture {}: {e}", path.display())).ok()?;
    let img = image::load_from_memory(&bytes).map_err(|e| log::warn!("texture {}: {e}", path.display())).ok()?.to_rgba8();
    let tex = Texture2D::from_rgba8(img.width() as u16, img.height() as u16, img.as_raw());
    tex.set_filter(FilterMode::Linear);
    Some(tex)
}

fn instance_matrix(i: &Instance) -> Mat4 {
    let place = Mat4::from_cols(vec4(1.0, i[3], 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, i[4], 1.0, 0.0), vec4(i[0], i[1], i[2], 1.0));
    let turn = Mat4::from_cols(vec4(i[5], 0.0, i[6], 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(-i[6], 0.0, i[5], 0.0), vec4(i[7], 0.0, i[8], 1.0));
    place * turn
}

impl Renderer {
    pub fn new(catalog: Catalog) -> Self {
        let palette = Self::terrain_palette(&catalog.path("graphism/texture/terrain_64x4096_texture.png"));
        let white = Texture2D::from_rgba8(1, 1, &[255, 255, 255, 255]);
        Self {
            catalog,
            models: HashMap::new(),
            vehicle_meshes: HashMap::new(),
            plane: None,
            obj_meshes: HashMap::new(),
            pier: None,
            variants: HashMap::new(),
            atmosphere: Atmosphere::default(),
            textures: HashMap::new(),
            palette,
            white,
            look: Look::default(),
            gpu: Gpu::new(),
            terrain: Static::default(),
            terrain_key: None,
            roads: Static::default(),
            road_surfaces: HashMap::new(),
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
        self.variants.clear();
        self.pier = None;
        self.terrain_key = None;
        self.scene_key = None;
    }

    /// The skin's bridge pier model, loaded on first use.
    fn pier_model(&mut self) -> Option<(opencity_assets::obj::ObjMesh, Texture2D)> {
        if self.pier.is_none() {
            let loaded = self.look.bridge_pier.clone().and_then(|name| {
                let path = self.catalog.path(&format!("models/{name}.obj"));
                let obj = opencity_assets::obj::load(&path).map_err(|e| log::warn!("bridge pier {name}: {e}")).ok()?;
                let tex = path.parent().map(|d| self.texture(&d.join("colormap.png"))).unwrap_or_else(|| self.white.clone());
                Some((obj, tex))
            });
            self.pier = Some(loaded);
        }
        self.pier.clone().flatten()
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

    /// Whether a data file exists (icons that only some tools have).
    pub fn has_file(&self, rel: &str) -> bool {
        opencity_assets::read_bytes(&self.catalog.path(rel)).is_ok()
    }

    fn model(&mut self, key: ModelKey) -> Option<&Model> {
        if !self.models.contains_key(&key) {
            let loaded = self.load_model(key);
            self.models.insert(key, loaded);
        }
        self.models.get(&key).and_then(Option::as_ref)
    }

    /// How many variants a model has.
    fn variant_count(&mut self, gc: GraphicCode) -> u16 {
        if let Some(&n) = self.variants.get(&gc) {
            return n;
        }
        let n = self.catalog.get(gc).map_or(1, |e| opencity_assets::compose::variants(&e.model)).clamp(1, u16::MAX as usize) as u16;
        self.variants.insert(gc, n);
        n
    }

    fn load_model(&mut self, (gc, variant): ModelKey) -> Option<Model> {
        let entry = self.catalog.get(gc)?;
        let (path, fp) = (entry.model.clone(), (entry.footprint.width, entry.footprint.length));
        let model = if opencity_assets::compose::is_composed(&path) {
            opencity_assets::compose::load_variant(&path, fp, variant as usize)
        } else {
            opencity_assets::load_building_model(&path, fp)
        };
        let model = model.map_err(|e| log::warn!("{e}")).ok()?;
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
        let mut groups: HashMap<ModelKey, Vec<Instance>> = HashMap::new();
        for ((w, l), tile) in city.layer.tiles() {
            let TileKind::Main(s) = &tile.kind else { continue };
            if s.code == StructureCode::Road && !roads::uses_model(s.connections, s.road) {
                continue;
            }
            let c = t.tile_corners(w, l);
            let [tc, ts, tw, tl] = crate::gpu::turn(s.facing, city.footprints().size(s.graphic));
            // Each plot keeps its own variant of a composed model.
            let n = self.variant_count(s.graphic) as u32;
            let variant = if n > 1 { (w.wrapping_mul(73_856_093) ^ l.wrapping_mul(19_349_663)) % n } else { 0 } as u16;
            groups.entry((s.graphic, variant)).or_default().push([
                w as f32,
                c[0] as f32 + MODEL_LIFT,
                l as f32,
                (c[3] - c[0]) as f32,
                (c[1] - c[0]) as f32,
                tc,
                ts,
                tw,
                tl,
            ]);
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
        let tex = self.model((ROAD_TEXTURE_MODEL, 0))?.cpu.iter().find_map(|m| m.texture.clone())?;
        repeat(&tex);
        Some(tex)
    }

    /// The surface a kind of road is drawn with.
    fn road_surface(&mut self, kind: RoadKind) -> Option<Texture2D> {
        if kind == RoadKind::Street {
            return self.road_texture();
        }
        if let Some(t) = self.road_surfaces.get(&kind) {
            return Some(t.clone());
        }
        let n = roads::SURFACE_SIZE as u16;
        let tex = Texture2D::from_rgba8(n, n, &roads::surface(kind)?);
        tex.set_filter(FilterMode::Linear);
        repeat(&tex);
        self.road_surfaces.insert(kind, tex.clone());
        Some(tex)
    }

    /// Smooth, textured ribbons along road centre lines, one surface per
    /// kind of road, and flat patches for crossings without models.
    fn build_roads(&mut self, city: &City) {
        self.roads.clear(self.gpu.as_ref());
        let t = &city.terrain;
        let color = rgba(self.look.apply([1.0, 1.0, 1.0, 1.0]), shade(Vec3::Y));
        let lines = roads::centre_lines(city);
        let patches = roads::patches(city);
        for kind in RoadKind::ALL {
            if !lines.iter().any(|l| l.kind == kind) && !patches.iter().any(|p| p.kind == kind) {
                continue;
            }
            let mut b = MeshBuilder::new(self.road_surface(kind));
            let half = roads::half_width(kind);
            // Wider roads sit a hair higher, so they cover the narrower ones they meet.
            let lift = ROAD_LIFT + half * 0.01;
            let at = |q: Vec2| vec3(q.x, deck_height(t, q.x, q.y) + lift, q.y);
            let v = |p: Vec3, u: f32, v: f32| Vertex { position: p, uv: vec2(u, v), color, normal: Vec4::ZERO };
            for line in lines.iter().filter(|l| l.kind == kind) {
                let (pts, closed) = (roads::smooth(&line.points, line.closed), line.closed);
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
                    let side = vec2(-d.y, d.x) * half;
                    let p = vec2(pts[i][0], pts[i][1]);
                    (at(p + side), at(p - side))
                };
                let segs = if closed { n } else { n - 1 };
                for i in 0..segs {
                    let j = (i + 1) % n;
                    let len = vec2(pts[j][0] - pts[i][0], pts[j][1] - pts[i][1]).length();
                    let ((l0, r0), (l1, r1)) = (edge(i), edge(j));
                    b.quad([v(l0, 0.0, along), v(r0, 1.0, along), v(r1, 1.0, along + len), v(l1, 0.0, along + len)]);
                    along += len;
                }
            }
            // Crossings: a square in the middle and an arm to each connected edge,
            // textured with a plain stretch of the surface (no lane markings).
            let (u0, u1) = if kind == RoadKind::Highway { (0.12, 0.24) } else { (0.2, 0.8) };
            for p in patches.iter().filter(|p| p.kind == kind) {
                let c = vec2(p.tile.0 as f32 + 0.5, p.tile.1 as f32 + 0.5);
                let mut rect = |a: Vec2, z: Vec2| {
                    let uv = |q: Vec2| (u0 + (u1 - u0) * (q.x - a.x) / (z.x - a.x).max(1e-3), q.y);
                    let corners = [vec2(a.x, a.y), vec2(z.x, a.y), vec2(z.x, z.y), vec2(a.x, z.y)];
                    let vs = corners.map(|q| {
                        let (uu, vv) = uv(q);
                        v(at(q), uu, vv)
                    });
                    b.quad(vs);
                };
                rect(c - Vec2::splat(half), c + Vec2::splat(half));
                for d in Direction::ALL {
                    if p.mask & d.mask() == 0 {
                        continue;
                    }
                    let (dx, dl) = d.delta();
                    let dir = vec2(dx as f32, dl as f32);
                    let side = vec2(-dir.y, dir.x).abs() * half;
                    let (from, to) = (c + dir * half, c + dir * 0.5);
                    let a = from.min(to) - side;
                    let z = from.max(to) + side;
                    rect(a, z);
                }
            }
            for m in b.finish() {
                self.roads.push(self.gpu.as_ref(), m);
            }
        }
        // Bridge piers under every road tile over water: the skin's model,
        // two per tile across the road, stretched from the bed to the deck;
        // without one, a dark column.
        let pier = self.pier_model();
        let mut piers = MeshBuilder::new(Some(pier.as_ref().map_or_else(|| self.white.clone(), |(_, tex)| tex.clone())));
        let grey = |n: Vec3| rgba(self.look.apply([0.42, 0.42, 0.45, 1.0]), shade(n));
        let white = |n: Vec3| rgba(self.look.apply([1.0, 1.0, 1.0, 1.0]), shade(n));
        for ((w, l), s) in city.layer.structures().filter(|(_, s)| s.code == StructureCode::Road) {
            if !t.is_water(w, l) {
                continue;
            }
            let (cx, cz) = (w as f32 + 0.5, l as f32 + 0.5);
            let (bottom, top) = (t.tile_min(w, l) as f32 - 0.1, BRIDGE_DECK + ROAD_LIFT);
            if let Some((obj, _)) = &pier {
                let (lo, hi) = obj.bounds();
                let (ox, oz) = ((lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0);
                let stretch = (top - bottom) / (hi[1] - lo[1]).max(1e-3);
                // Along the road: a north-south tile has its piers east and west.
                let along_x = s.connections & (opencity_core::codes::connect::EAST | opencity_core::codes::connect::WEST) != 0
                    && s.connections & (opencity_core::codes::connect::NORTH | opencity_core::codes::connect::SOUTH) == 0;
                let spread = (roads::half_width(s.road) * 0.7).max(0.12);
                for side in [-1.0, 1.0] {
                    let (px, pz) = if along_x { (cx, cz + side * spread) } else { (cx + side * spread, cz) };
                    let place = |q: [f32; 3]| vec3(px + (q[0] - ox) * 2.2, bottom + (q[1] - lo[1]) * stretch, pz + (q[2] - oz) * 2.2);
                    for tri in &obj.triangles {
                        let p = tri.map(|i| place(obj.positions[i as usize]));
                        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                        let v = |i: usize| Vertex { position: p[i], uv: Vec2::from(obj.uvs[tri[i] as usize]), color: white(n), normal: n.extend(0.0) };
                        piers.triangle([v(0), v(1), v(2)]);
                    }
                }
                continue;
            }
            let r = (roads::half_width(s.road) * 0.6).min(0.3);
            let v = |x: f32, y: f32, z: f32, n: Vec3| Vertex { position: vec3(x, y, z), uv: Vec2::ZERO, color: grey(n), normal: Vec4::ZERO };
            for (dx, dz, n) in [(1.0, 0.0, Vec3::X), (-1.0, 0.0, Vec3::NEG_X), (0.0, 1.0, Vec3::Z), (0.0, -1.0, Vec3::NEG_Z)] {
                let (ax, az) = (cx + dx * r - dz * r, cz + dz * r + dx * r);
                let (bx, bz) = (cx + dx * r + dz * r, cz + dz * r - dx * r);
                piers.quad([v(ax, bottom, az, n), v(ax, top, az, n), v(bx, top, bz, n), v(bx, bottom, bz, n)]);
            }
        }
        for m in piers.finish() {
            self.roads.push(self.gpu.as_ref(), m);
        }
    }

    /// An `.obj` model (`kenney/car-kit/taxi`), loaded on first use with
    /// the `colormap.png` next to it.
    fn obj_mesh(&mut self, name: &str) -> Option<&Vec<Mesh>> {
        if !self.obj_meshes.contains_key(name) {
            let path = self.catalog.path(&format!("models/{name}.obj"));
            let meshes = opencity_assets::obj::load(&path).map_err(|e| log::warn!("vehicle model {name}: {e}")).ok().map(|obj| {
                let tex = path.parent().and_then(|d| load_texture_file(&d.join("colormap.png")));
                if let Some(t) = &tex {
                    t.set_filter(FilterMode::Nearest);
                }
                crate::vehicles::obj_meshes(&obj, tex)
            });
            self.obj_meshes.insert(name.to_string(), meshes);
        }
        self.obj_meshes.get(name)?.as_ref()
    }

    fn draw_model_cpu(&mut self, gc: ModelKey, matrix: Mat4) {
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
                let a = self.atmosphere;
                let mvp = camera.matrix();
                g.begin(mvp, crate::gpu::Tint { light: a.light, blend: a.ground });
                self.terrain.draw(Some(g));
                g.tint(mvp, crate::gpu::Tint { light: a.light, blend: Vec4::ZERO });
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
                let items: Vec<(ModelKey, Mat4)> =
                    self.instances.iter().flat_map(|(gc, _, list)| list.iter().map(move |i| (*gc, instance_matrix(i)))).collect();
                for (gc, m) in items {
                    self.draw_model_cpu(gc, m);
                }
            }
        }
        // Skins of the past hide cars and trucks; walkers and carts stay.
        let motor = self.look.vehicles;
        for v in city.vehicles.iter().filter(|v| motor || v.kind.graphic().is_none()) {
            let p = v.pose();
            let angle = match p.heading {
                Direction::South => 0.0f32,
                Direction::East => 90.0,
                Direction::North => 180.0,
                Direction::West => -90.0,
            }
            .to_radians();
            // Over water, cars and trains are on a bridge.
            let y = if !v.water && p.y < 0.0 { BRIDGE_DECK } else { p.y };
            let m = Mat4::from_translation(vec3(p.x, y + 0.02, p.z)) * Mat4::from_rotation_y(angle) * Mat4::from_translation(vec3(-0.5, 0.0, -0.5));
            if let Some(meshes) = v.mesh.as_deref().and_then(|name| self.obj_mesh(name)) {
                // Trains run down the middle of the track, and a locomotive
                // is longer than a car.
                let m = if v.water {
                    m * Mat4::from_translation(vec3(0.5, -0.04, 0.5)) * Mat4::from_scale(Vec3::splat(1.2)) * Mat4::from_translation(vec3(-0.3, 0.0, -0.5))
                } else if v.on_rail() {
                    m * Mat4::from_translation(vec3(0.5, 0.0, 0.5)) * Mat4::from_scale(Vec3::splat(1.8)) * Mat4::from_translation(vec3(-0.3, 0.0, -0.5))
                } else {
                    m
                };
                let gl = unsafe { get_internal_gl() }.quad_gl;
                gl.push_model_matrix(m);
                for mesh in meshes {
                    draw_mesh(mesh);
                }
                let gl = unsafe { get_internal_gl() }.quad_gl;
                gl.pop_model_matrix();
                continue;
            }
            match v.kind.graphic() {
                Some(gc) => self.draw_model_cpu((gc, 0), m),
                None => {
                    if let Some(mesh) = self.vehicle_meshes.entry(v.kind).or_insert_with(|| crate::vehicles::mesh(v.kind)) {
                        let gl = unsafe { get_internal_gl() }.quad_gl;
                        gl.push_model_matrix(m);
                        draw_mesh(mesh);
                        let gl = unsafe { get_internal_gl() }.quad_gl;
                        gl.pop_model_matrix();
                    }
                }
            }
        }
        self.draw_planes(city);
        // Translucent water plane, drawn last.
        let (w, l) = (city.width() as f32, city.length() as f32);
        let c = self.look.water;
        let li = self.atmosphere.light;
        draw_plane(vec3(w / 2.0, WATER_LEVEL, l / 2.0), vec2(w / 2.0, l / 2.0), None, Color::new(c[0] * li.x, c[1] * li.y, c[2] * li.z, c[3]));
    }

    /// An airliner at each airport, forever taking off down the runway,
    /// circling beside it and landing again.
    fn draw_planes(&mut self, city: &City) {
        let airports: Vec<((u32, u32), (u32, u32))> =
            city.layer.structures().filter(|(_, s)| s.code == StructureCode::Airport).map(|(p, s)| (p, s.size(city.footprints()))).collect();
        if airports.is_empty() {
            return;
        }
        let mesh = self.plane.get_or_insert_with(crate::vehicles::plane);
        let time = get_time();
        for (i, ((w, l), (sw, sl))) in airports.into_iter().enumerate() {
            let (cx, cz) = (w as f32 + sw as f32 / 2.0, l as f32 + sl as f32 / 2.0);
            let centre = vec3(cx, height_at(&city.terrain, cx, cz).max(0.0), cz);
            // The runway runs along the long side.
            let (u, half) = if sw >= sl { (Vec3::X, sw as f32 / 2.0 - 0.3) } else { (Vec3::Z, sl as f32 / 2.0 - 0.3) };
            let v = vec3(-u.z, 0.0, u.x);
            let t = ((time + i as f64 * 13.0) / 40.0).fract() as f32;
            let (pos, dir, y) = if t < 0.25 {
                // Down the runway, lifting off at the end.
                let s = t / 0.25;
                (centre + u * (half * (2.0 * s - 1.0)), u, 0.08 + 2.0 * (s - 0.8).max(0.0))
            } else {
                // A loop beside the airport, back to the start of the runway.
                let s = (t - 0.25) / 0.75;
                let radius = half * std::f32::consts::SQRT_2;
                let a = (-half).atan2(half) + s * 1.5 * std::f32::consts::PI;
                let o = centre + v * half;
                (o + u * (radius * a.cos()) + v * (radius * a.sin()), -u * a.sin() + v * a.cos(), 0.08 + 5.0 * (std::f32::consts::PI * s).sin())
            };
            let m = Mat4::from_translation(vec3(pos.x, pos.y + y, pos.z)) * Mat4::from_rotation_y(dir.x.atan2(dir.z));
            let gl = unsafe { get_internal_gl() }.quad_gl;
            gl.push_model_matrix(m);
            draw_mesh(mesh);
            let gl = unsafe { get_internal_gl() }.quad_gl;
            gl.pop_model_matrix();
        }
    }

    /// A data-layer tile drawn over everything (no depth test) so values
    /// stay readable under buildings.
    pub fn draw_data_overlay_rect(&self, t: &Terrain, r: Rect, color: Color) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(false);
        self.draw_overlay(t, r, color);
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(true);
    }

    pub fn draw_data_overlay(&self, t: &Terrain, w: u32, l: u32, color: Color) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(false);
        self.draw_overlay(t, Rect::tile(w, l), color);
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(true);
    }

    /// The resource network: camera-facing ribbons along each arc, fading in
    /// from the sender, with dots travelling towards the receiver.
    pub fn draw_flows(&self, arcs: &[crate::flows::Arc], cam: &Camera3D, time: f64, hovered: Option<usize>) {
        use crate::flows::SEGMENTS;
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(false);
        gl.texture(None);
        gl.draw_mode(DrawMode::Triangles);
        let view_dir = (cam.target - cam.position).normalize_or_zero();
        let right = view_dir.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(view_dir).normalize_or_zero();
        let vert = |p: Vec3, c: Color| Vertex { position: p, uv: Vec2::ZERO, color: c.into(), normal: Vec4::ZERO };
        for (i, arc) in arcs.iter().enumerate() {
            let hot = hovered == Some(i);
            let width = if hot { arc.width * 1.8 + 0.05 } else { arc.width };
            // Stretches along the network are straight; only links drawn
            // straight across bulge.
            let segs = if arc.height > 0.0 { SEGMENTS } else { 1 };
            let pts: Vec<Vec3> = (0..=segs).map(|k| arc.point(k as f32 / segs as f32)).collect();
            for k in 0..segs {
                let (p, q) = (pts[k], pts[k + 1]);
                let side = (q - p).cross(view_dir).normalize_or_zero() * width * 0.5;
                let t = k as f32 / segs as f32;
                let mut c = arc.colour;
                if hot {
                    c = Color::new(c.r * 0.5 + 0.5, c.g * 0.5 + 0.5, c.b * 0.5 + 0.5, 1.0);
                }
                let fade = |t: f32| if segs > 1 { 0.3 + 0.6 * t } else { 0.75 };
                let (c0, c1) = (Color { a: fade(t), ..c }, Color { a: fade(t + 1.0 / segs as f32), ..c });
                gl.geometry(&[vert(p - side, c0), vert(p + side, c0), vert(q + side, c1), vert(q - side, c1)], &[0, 1, 2, 0, 2, 3]);
            }
            // Travelling dots: more on longer links, faster on busier ones.
            // Along the network the dots keep step from stretch to stretch,
            // a steady stream; power has none (it does not travel).
            if arc.stretch.way == opencity_core::netmap::Way::Grid {
                continue;
            }
            let dots = 1 + arc.tiles() / 8;
            let speed = (0.8 / arc.tiles().max(1) as f32).max(0.1);
            for d in 0..dots {
                let t = ((time as f32 * speed + d as f32 / dots as f32) % 1.0).abs();
                let p = arc.point(t);
                // A small hexagon facing the camera.
                let r = 0.035 + width * 0.6;
                let c = Color::new(arc.colour.r * 0.4 + 0.6, arc.colour.g * 0.4 + 0.6, arc.colour.b * 0.4 + 0.6, 0.95);
                let mut vs = vec![vert(p, c)];
                for k in 0..6 {
                    let a = k as f32 * std::f32::consts::FRAC_PI_3;
                    vs.push(vert(p + right * (a.cos() * r) + up * (a.sin() * r), c));
                }
                gl.geometry(&vs, &[0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 6, 0, 6, 1]);
            }
        }
        gl.depth_test(true);
    }

    /// What runs underground, seen through everything: water mains (blue)
    /// and metro tunnels (orange, wider, a little lower), dark-edged, from
    /// tile centre to tile centre with a joint at each tile.
    ///
    /// What belongs to them on the surface stands out of the faded city:
    /// metro stations glow orange under a beacon (bus and tram stops stay
    /// dim, so they are not taken for them), pumping stations, water towers
    /// and treatment plants glow blue, cyan and green. With a tool in hand
    /// (`focus`), the other system is drawn faintly.
    pub fn draw_pipes(&self, city: &City, focus: Underground) {
        let layer = &city.layer;
        let faint = |on: bool, c: Color| if on { c } else { Color::new(c.r, c.g, c.b, c.a * 0.22) };
        let (metro, water) = (focus != Underground::Water, focus != Underground::Metro);
        let tunnel = [Color::new(0.20, 0.08, 0.02, 0.95), Color::new(0.95, 0.55, 0.20, 1.0)].map(|c| faint(metro, c));
        self.draw_underground(city, |w, l| layer.has_tunnel(w, l), layer.tunnels(), [0.2, 0.14], tunnel, 0.03);
        let pipe = [Color::new(0.02, 0.10, 0.20, 0.95), Color::new(0.30, 0.70, 1.00, 1.0)].map(|c| faint(water, c));
        self.draw_underground(city, |w, l| layer.has_pipe(w, l), layer.pipes(), [0.13, 0.08], pipe, 0.06);
        for ((w, l), s) in layer.structures() {
            let (colour, shown) = match s.code {
                StructureCode::MetroStation => (Color::new(1.0, 0.55, 0.12, 1.0), metro),
                StructureCode::PumpingStation => (Color::new(0.25, 0.60, 1.0, 1.0), water),
                StructureCode::WaterTower => (Color::new(0.35, 0.90, 1.0, 1.0), water),
                StructureCode::WaterTreatment => (Color::new(0.35, 0.90, 0.55, 1.0), water),
                _ => continue,
            };
            if !shown {
                continue;
            }
            let (sw, sl) = s.size(city.footprints());
            self.draw_data_overlay_rect(&city.terrain, Rect::sized(w, l, sw, sl), Color::new(colour.r, colour.g, colour.b, 0.7));
            // A beacon over it: a mast with a bright head, seen through everything.
            let (x, z) = (w as f32 + sw as f32 / 2.0, l as f32 + sl as f32 / 2.0);
            let ground = height_at(&city.terrain, x, z).max(WATER_LEVEL);
            let gl = unsafe { get_internal_gl() }.quad_gl;
            gl.depth_test(false);
            draw_cube(vec3(x, ground + 1.1, z), vec3(0.1, 2.2, 0.1), None, Color::new(0.95, 0.95, 0.95, 1.0));
            draw_cube(vec3(x, ground + 2.4, z), vec3(0.5, 0.4, 0.5), None, colour);
            let gl = unsafe { get_internal_gl() }.quad_gl;
            gl.depth_test(true);
        }
    }

    fn draw_underground(
        &self,
        city: &City,
        has: impl Fn(u32, u32) -> bool,
        tiles: impl Iterator<Item = (u32, u32)>,
        halves: [f32; 2],
        colours: [Color; 2],
        lift: f32,
    ) {
        let t = &city.terrain;
        let tiles: Vec<(u32, u32)> = tiles.collect();
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(false);
        gl.texture(None);
        gl.draw_mode(DrawMode::Triangles);
        let vert = |p: Vec3, c: Color| Vertex { position: p, uv: Vec2::ZERO, color: c.into(), normal: Vec4::ZERO };
        let centre = |w: u32, l: u32| {
            let (x, z) = (w as f32 + 0.5, l as f32 + 0.5);
            vec3(x, height_at(t, x, z).max(WATER_LEVEL) + lift, z)
        };
        for (half, c) in halves.into_iter().zip(colours) {
            for &(w, l) in &tiles {
                let p = centre(w, l);
                for (dw, dl) in [(1, 0), (0, 1)] {
                    if !has(w + dw, l + dl) {
                        continue;
                    }
                    let q = centre(w + dw, l + dl);
                    let side = vec3(dl as f32, 0.0, dw as f32) * half;
                    gl.geometry(&[vert(p - side, c), vert(p + side, c), vert(q + side, c), vert(q - side, c)], &[0, 1, 2, 0, 2, 3]);
                }
                let j = half * 1.4;
                let (a, b) = (vec3(j, 0.0, 0.0), vec3(0.0, 0.0, j));
                gl.geometry(&[vert(p - a - b, c), vert(p + a - b, c), vert(p + a + b, c), vert(p - a + b, c)], &[0, 1, 2, 0, 2, 3]);
            }
        }
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

/// What the underground view brings out: everything, or the system the
/// tool in hand belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underground {
    All,
    Water,
    Metro,
}

/// Bilinear terrain height at a map position (tile units).
/// Where a road runs: the ground, or a bridge deck over water and down
/// the banks to it.
pub fn deck_height(t: &Terrain, x: f32, z: f32) -> f32 {
    let h = height_at(t, x, z);
    if h < 0.0 {
        BRIDGE_DECK.max(h)
    } else {
        h
    }
}

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

/// The light of an hour, season and weather: a tint for everything, the
/// sky's colour factor, and a colour blended over the ground (snow,
/// autumn gold, spring green).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub light: Vec4,
    pub sky: Vec3,
    pub ground: Vec4,
    /// Falling rain (true) or snow (false), and how much (0..1).
    pub precipitation: Option<(bool, f32)>,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self { light: Vec4::ONE, sky: Vec3::ONE, ground: Vec4::ZERO, precipitation: None }
    }
}

impl Atmosphere {
    pub fn of(hour: f32, season: opencity_core::city::Season, weather: opencity_core::city::Weather, day_night: bool) -> Self {
        use opencity_core::city::{Season, Weather};
        // Daylight: 0 at night, 1 at noon, sunrise at 6 and sunset at 18.
        let day = if day_night { ((hour - 6.0) / 12.0 * std::f32::consts::PI).sin().max(0.0).sqrt() } else { 1.0 };
        let night = vec3(0.42, 0.48, 0.78);
        let noon = Vec3::ONE;
        let mut light = night.lerp(noon, day);
        // Warm light while the sun is low.
        if day > 0.0 && day < 0.55 {
            let k = 1.0 - day / 0.55;
            light *= vec3(1.0, 1.0 - 0.18 * k, 1.0 - 0.32 * k);
        }
        let dim = match weather {
            Weather::Clear => 1.0,
            Weather::Cloudy => 0.88,
            Weather::Rain => 0.76,
            Weather::Storm => 0.62,
            Weather::Snow => 0.94,
        };
        light *= dim;
        let sky = light * if matches!(weather, Weather::Rain | Weather::Storm) { vec3(0.85, 0.88, 0.92) } else { Vec3::ONE };
        let ground = match (season, weather) {
            (_, Weather::Snow) | (Season::Winter, _) => vec4(0.93, 0.95, 1.0, if weather == Weather::Snow { 0.75 } else { 0.5 }),
            (Season::Autumn, _) => vec4(0.72, 0.55, 0.25, 0.3),
            (Season::Spring, _) => vec4(0.45, 0.78, 0.32, 0.12),
            (Season::Summer, _) => Vec4::ZERO,
        };
        let precipitation = match weather {
            Weather::Rain => Some((true, 0.6)),
            Weather::Storm => Some((true, 1.0)),
            Weather::Snow => Some((false, 0.7)),
            _ => None,
        };
        Self { light: light.extend(1.0), sky, ground, precipitation }
    }
}

/// Border gates where roads, tracks and power lines leave a region city:
/// two posts and a bar just past the edge, green when the neighbour has
/// joined the crossing, amber while it waits, grey at the region's edge.
pub fn draw_crossings(city: &opencity_core::City, gates: &[(opencity_core::region::Crossing, Option<bool>)]) {
    let (w, l) = (city.width(), city.length());
    for (c, state) in gates {
        let (x, y) = c.tile(w, l);
        let (cx, cz) = (x as f32 + 0.5, y as f32 + 0.5);
        // Out across the edge, and along it for the posts.
        let (out, along) = match c.side {
            0 => (vec3(0.0, 0.0, -1.0), vec3(1.0, 0.0, 0.0)),
            1 => (vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0)),
            2 => (vec3(0.0, 0.0, 1.0), vec3(1.0, 0.0, 0.0)),
            _ => (vec3(-1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0)),
        };
        let at = vec3(cx, 0.0, cz) + out * 0.45;
        let ground = height_at(&city.terrain, at.x - out.x * 0.1, at.z - out.z * 0.1);
        let colour = match state {
            Some(true) => Color::new(0.30, 0.75, 0.35, 1.0),
            Some(false) => Color::new(0.95, 0.70, 0.20, 1.0),
            None => Color::new(0.55, 0.55, 0.55, 1.0),
        };
        let post = vec3(0.06, 0.7, 0.06);
        for s in [-0.42, 0.42] {
            draw_cube(vec3(at.x, ground + 0.35, at.z) + along * s, post, None, Color::new(0.92, 0.92, 0.9, 1.0));
        }
        let bar = vec3(0.84 * along.x.abs() + 0.05, 0.08, 0.84 * along.z.abs() + 0.05);
        draw_cube(vec3(at.x, ground + 0.66, at.z), bar, None, colour);
    }
}

/// Sensors left on the map: a mast at a corner of the tile with a blue
/// head, tall enough to show beside a house.
pub fn draw_sensors(city: &opencity_core::City) {
    let Some(gb) = &city.glassbox else { return };
    for s in gb.sensors.iter().filter(|s| s.at.0 < city.width() && s.at.1 < city.length()) {
        let (x, z) = (s.at.0 as f32 + 0.12, s.at.1 as f32 + 0.12);
        let ground = height_at(&city.terrain, x, z);
        draw_cube(vec3(x, ground + 0.7, z), vec3(0.04, 1.4, 0.04), None, Color::new(0.85, 0.85, 0.88, 1.0));
        draw_cube(vec3(x, ground + 1.45, z), vec3(0.16, 0.12, 0.16), None, Color::new(0.20, 0.65, 0.95, 1.0));
    }
}

/// Rain or snow falling around a point, at a moment (seconds).
/// The sky panorama behind the scene: the part of an equirectangular image
/// the camera looks at (by its yaw and pitch), tinted by the light. Drawn in
/// screen space, before the 3D scene.
pub fn draw_sky(tex: &Texture2D, yaw: f32, pitch: f32, tint: [f32; 3]) {
    use std::f32::consts::{PI, TAU};
    let (sw, sh) = (screen_width(), screen_height());
    let (tw, th) = (tex.width(), tex.height());
    let vfov = 45f32.to_radians();
    let hfov = 2.0 * ((vfov / 2.0).tan() * sw / sh.max(1.0)).atan();
    // The camera looks down by `pitch`: the middle of the screen is that
    // far under the horizon (v = 0.5).
    let v_mid = 0.5 + pitch / PI;
    let (v0, v1) = ((v_mid - vfov / 2.0 / PI).clamp(0.0, 1.0), (v_mid + vfov / 2.0 / PI).clamp(0.0, 1.0));
    let width = hfov / TAU;
    let u0 = (-yaw / TAU - width / 2.0).rem_euclid(1.0);
    let colour = Color::new(tint[0].min(1.0), tint[1].min(1.0), tint[2].min(1.0), 1.0);
    // In one piece, or two where the view crosses the image's seam.
    let first = width.min(1.0 - u0);
    let mut x = 0.0;
    for (u, w) in [(u0, first), (0.0, width - first)] {
        if w <= 0.0 {
            continue;
        }
        let dw = sw * w / width;
        draw_texture_ex(
            tex,
            x,
            0.0,
            colour,
            DrawTextureParams {
                dest_size: Some(vec2(dw + 1.0, sh)),
                source: Some(macroquad::math::Rect::new(u * tw, v0 * th, w * tw, (v1 - v0) * th)),
                ..Default::default()
            },
        );
        x += dw;
    }
}

/// Smoke over power plants, incinerators and industry: a few puffs per
/// chimney that rise, drift, grow and fade, placed by the time alone (no
/// particle state). `textures` are the white puff and the black smoke.
pub fn draw_smoke(city: &City, camera: &Camera3D, around: Vec3, time: f64, textures: [&Texture2D; 2], light: f32) {
    const PUFFS: u32 = 5;
    const REACH: f32 = 45.0;
    let fwd = (camera.target - camera.position).normalize_or_zero();
    let right = fwd.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(fwd).normalize_or_zero();
    let mut meshes = [Vec::<Vertex>::new(), Vec::new()];
    for ((w, l), s) in city.layer.structures() {
        // (texture, plumes, size) by what burns there.
        let (tex, plumes, size) = match s.code {
            StructureCode::CoalPlant => (1, 2, 0.55),
            StructureCode::Incinerator => (1, 1, 0.5),
            StructureCode::Industrial if s.unit.is_some() && s.level >= 1 => (0, 1, 0.22 + 0.06 * s.level.min(3) as f32),
            _ => continue,
        };
        let fp = city.footprints().get(s.graphic);
        let (cx, cz) = (w as f32 + fp.width.max(1) as f32 / 2.0, l as f32 + fp.length.max(1) as f32 / 2.0);
        if (cx - around.x).abs() > REACH || (cz - around.z).abs() > REACH {
            continue;
        }
        let base = city.terrain.tile_max(w, l) as f32 + fp.height.clamp(0.4, 3.0) * 0.9;
        for plume in 0..plumes {
            let seed = (w.wrapping_mul(73) ^ l.wrapping_mul(151) ^ (plume * 31)) as f32;
            let (ox, oz) = (((seed * 0.37).sin()) * 0.25 * fp.width as f32 + plume as f32 * 0.35, ((seed * 0.73).cos()) * 0.2 * fp.length as f32);
            for k in 0..PUFFS {
                let t = ((time as f32 * 0.12 + k as f32 / PUFFS as f32 + seed * 0.013) % 1.0 + 1.0) % 1.0;
                let rise = t * (1.2 + size * 2.0);
                let centre = vec3(cx + ox + t * t * 0.9, base + rise, cz + oz + t * 0.3);
                let r = size * (0.5 + t * 1.3);
                let alpha = (1.0 - t).min(t * 6.0).clamp(0.0, 1.0) * if tex == 1 { 0.75 } else { 0.55 };
                let c = (light * 255.0).min(255.0) as u8;
                let colour = [c, c, c, (alpha * 255.0) as u8];
                let v = |dx: f32, dy: f32, u: f32, vv: f32| Vertex {
                    position: centre + right * dx * r + up * dy * r,
                    uv: vec2(u, vv),
                    color: colour,
                    normal: Vec4::ZERO,
                };
                let m = &mut meshes[tex];
                if m.len() + 4 > 4000 {
                    continue;
                }
                m.extend([v(-1.0, -1.0, 0.0, 1.0), v(1.0, -1.0, 1.0, 1.0), v(1.0, 1.0, 1.0, 0.0), v(-1.0, 1.0, 0.0, 0.0)]);
            }
        }
    }
    for (vertices, texture) in meshes.into_iter().zip(textures) {
        if vertices.is_empty() {
            continue;
        }
        let indices = (0..vertices.len() as u16 / 4).flat_map(|q| [q * 4, q * 4 + 1, q * 4 + 2, q * 4, q * 4 + 2, q * 4 + 3]).collect();
        draw_mesh(&Mesh { vertices, indices, texture: Some(texture.clone()) });
    }
}

/// Buildings in trouble: a badge floating over each (a flame, a wave, a
/// cross), and flames with black smoke on those that burn. `textures` are
/// the three badges by effect (destroy, damage, sicken) and the smoke.
pub fn draw_afflictions(city: &City, camera: &Camera3D, time: f64, textures: [&Texture2D; 4]) {
    use opencity_core::rules::DisasterEffect;
    let Some(gb) = &city.glassbox else { return };
    if gb.afflictions.is_empty() {
        return;
    }
    let Some(rules) = gb.rules() else { return };
    let fwd = (camera.target - camera.position).normalize_or_zero();
    let right = fwd.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(fwd).normalize_or_zero();
    let mut meshes: [Vec<Vertex>; 4] = Default::default();
    let mut quad = |tex: usize, centre: Vec3, r: f32, colour: [u8; 4]| {
        let v =
            |dx: f32, dy: f32, u: f32, vv: f32| Vertex { position: centre + right * dx * r + up * dy * r, uv: vec2(u, vv), color: colour, normal: Vec4::ZERO };
        if meshes[tex].len() + 4 <= 8000 {
            meshes[tex].extend([v(-1.0, -1.0, 0.0, 1.0), v(1.0, -1.0, 1.0, 1.0), v(1.0, 1.0, 1.0, 0.0), v(-1.0, 1.0, 0.0, 0.0)]);
        }
    };
    let t = time as f32;
    for (key, a) in &gb.afflictions {
        let (w, l) = (key % city.width(), key / city.width());
        let (Some(s), Some(d)) = (city.layer.tile(w, l).and_then(|t| t.structure()), rules.disaster(&a.disaster)) else { continue };
        let fp = city.footprints().get(s.graphic);
        let (fw, fl) = (fp.width.max(1) as f32, fp.length.max(1) as f32);
        let (cx, cz) = (w as f32 + fw / 2.0, l as f32 + fl / 2.0);
        let ground = city.terrain.tile_max(w, l) as f32;
        let roof = ground + fp.height.clamp(0.4, 4.0);
        let seed = (w.wrapping_mul(73) ^ l.wrapping_mul(151)) as f32;
        let tex = match d.effect {
            DisasterEffect::Destroy => 0,
            DisasterEffect::Damage => 1,
            DisasterEffect::Sicken => 2,
        };
        if tex == 0 {
            // Flames licking up the walls and over the roof, and smoke.
            let flames = (3.0 * fw * fl).clamp(3.0, 12.0) as u32;
            for k in 0..flames {
                let ks = seed + k as f32 * 17.3;
                let (ox, oz) = ((ks * 0.37).sin() * 0.4 * fw, (ks * 0.73).cos() * 0.4 * fl);
                let flick = 0.75 + 0.25 * (t * 9.0 + ks).sin();
                let y = ground + (roof - ground) * (0.35 + 0.6 * ((ks * 0.21).sin() * 0.5 + 0.5));
                let r = 0.22 * flick * (0.8 + 0.4 * (ks * 0.11).cos().abs());
                quad(0, vec3(cx + ox, y + r * 0.6, cz + oz), r, [255, 255, 255, 235]);
            }
            for k in 0..6 {
                let p = ((t * 0.18 + k as f32 / 6.0 + seed * 0.013) % 1.0 + 1.0) % 1.0;
                let centre = vec3(cx + p * p * 1.2, roof + p * 2.4, cz + p * 0.4);
                let alpha = (1.0 - p).min(p * 6.0).clamp(0.0, 1.0) * 0.8;
                quad(3, centre, 0.3 * fw.min(2.0) * (0.5 + p * 1.5), [255, 255, 255, (alpha * 255.0) as u8]);
            }
        }
        // The badge, bobbing over the roof; it keeps a readable size from afar.
        let top = vec3(cx, roof + 0.75 + 0.08 * (t * 3.0 + seed).sin(), cz);
        let r = (camera.position.distance(top) * 0.022).clamp(0.3, 1.6);
        quad(tex, top + up * r, r, [255, 255, 255, 255]);
    }
    for (vertices, texture) in meshes.into_iter().zip(textures) {
        if vertices.is_empty() {
            continue;
        }
        let indices = (0..vertices.len() as u16 / 4).flat_map(|q| [q * 4, q * 4 + 1, q * 4 + 2, q * 4, q * 4 + 2, q * 4 + 3]).collect();
        draw_mesh(&Mesh { vertices, indices, texture: Some(texture.clone()) });
    }
}

pub fn draw_precipitation(around: Vec3, time: f64, rain: bool, amount: f32) {
    let n = (600.0 * amount) as u32;
    let (fall, len, colour) = if rain { (9.0, 0.35, Color::new(0.75, 0.8, 0.9, 0.55)) } else { (1.2, 0.06, Color::new(1.0, 1.0, 1.0, 0.9)) };
    for i in 0..n {
        // An integer hash, so each drop has its own place and phase.
        let h = |k: u32| {
            let mut v = i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
            v ^= v >> 16;
            v = v.wrapping_mul(0x7FEB_352D);
            v ^= v >> 15;
            v = v.wrapping_mul(0x846C_A68B);
            v ^= v >> 16;
            (v % 10_000) as f32 / 10_000.0
        };
        let (x, z) = (around.x + (h(1) - 0.5) * 40.0, around.z + (h(2) - 0.5) * 40.0);
        let span = 12.0;
        let y = around.y + span - ((time as f32 * fall + h(3) * span) % span);
        let drift = if rain { 0.0 } else { (time as f32 * 0.8 + h(4) * 6.0).sin() * 0.15 };
        let p = vec3(x + drift, y, z);
        draw_line_3d(p, p - vec3(0.0, len, 0.0), colour);
    }
}
