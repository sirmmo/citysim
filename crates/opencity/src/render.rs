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
    /// Vehicles drawn from boxes (walkers, carts), built on first use.
    vehicle_meshes: HashMap<opencity_core::traffic::VehicleKind, Option<Mesh>>,
    /// `.obj` vehicle models by path under `<data>/models/` (None: missing).
    obj_meshes: HashMap<String, Option<Vec<Mesh>>>,
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
    instances: Vec<(GraphicCode, Option<macroquad::miniquad::BufferId>, Vec<Instance>)>,
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
            obj_meshes: HashMap::new(),
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
        let entry = self.catalog.get(gc)?;
        let (path, fp) = (entry.model.clone(), (entry.footprint.width, entry.footprint.length));
        let model = opencity_assets::load_building_model(&path, fp).map_err(|e| log::warn!("{e}")).ok()?;
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
            if s.code == StructureCode::Road && !roads::uses_model(s.connections, s.road) {
                continue;
            }
            let c = t.tile_corners(w, l);
            let [tc, ts, tw, tl] = crate::gpu::turn(s.facing, city.footprints().size(s.graphic));
            groups.entry(s.graphic).or_default().push([
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
        let tex = self.model(ROAD_TEXTURE_MODEL)?.cpu.iter().find_map(|m| m.texture.clone())?;
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
            let at = |q: Vec2| vec3(q.x, height_at(t, q.x, q.y) + lift, q.y);
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
                let items: Vec<(GraphicCode, Mat4)> =
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
            let m = Mat4::from_translation(vec3(p.x, p.y + 0.02, p.z)) * Mat4::from_rotation_y(angle) * Mat4::from_translation(vec3(-0.5, 0.0, -0.5));
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
                Some(gc) => self.draw_model_cpu(gc, m),
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
        // Translucent water plane, drawn last.
        let (w, l) = (city.width() as f32, city.length() as f32);
        let c = self.look.water;
        let li = self.atmosphere.light;
        draw_plane(vec3(w / 2.0, WATER_LEVEL, l / 2.0), vec2(w / 2.0, l / 2.0), None, Color::new(c[0] * li.x, c[1] * li.y, c[2] * li.z, c[3]));
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

    /// The water pipes underground, seen through everything: a dark-edged
    /// blue main from tile centre to tile centre, with a joint at each tile.
    pub fn draw_pipes(&self, city: &City) {
        let t = &city.terrain;
        let layer = &city.layer;
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.depth_test(false);
        gl.texture(None);
        gl.draw_mode(DrawMode::Triangles);
        let vert = |p: Vec3, c: Color| Vertex { position: p, uv: Vec2::ZERO, color: c.into(), normal: Vec4::ZERO };
        let centre = |w: u32, l: u32| {
            let (x, z) = (w as f32 + 0.5, l as f32 + 0.5);
            vec3(x, height_at(t, x, z).max(WATER_LEVEL) + 0.06, z)
        };
        let edge = Color::new(0.02, 0.10, 0.20, 0.95);
        let core = Color::new(0.30, 0.70, 1.00, 1.0);
        for (half, c) in [(0.13, edge), (0.08, core)] {
            for (w, l) in layer.pipes() {
                let p = centre(w, l);
                for (dw, dl) in [(1, 0), (0, 1)] {
                    if !layer.has_pipe(w + dw, l + dl) {
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

/// Rain or snow falling around a point, at a moment (seconds).
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
