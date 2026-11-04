//! Retained GPU drawing for the static scene (terrain, roads, buildings).
//!
//! macroquad re-uploads every vertex every frame and issues one draw call per
//! model matrix, which made large cities slow. Here static geometry lives in
//! GPU buffers and buildings are drawn with instancing: one draw call per
//! (model, texture) whatever the number of buildings.

use macroquad::miniquad::{
    self as mq, BufferId, BufferLayout, BufferSource, BufferType, BufferUsage, Comparison, PassAction, PipelineParams, ShaderMeta, ShaderSource, TextureId,
    UniformBlockLayout, UniformDesc, UniformType, UniformsSource, VertexAttribute, VertexFormat, VertexStep,
};
use macroquad::models::Vertex;
use macroquad::prelude::*;

/// Per-instance data: tile origin (x, base height, z), slope shear
/// (dy/dx, dy/dz), and a turn about the vertical axis (cos, sin, then the
/// x, z offset that brings the turned model back onto its footprint).
pub type Instance = [f32; 9];

/// Instance that leaves geometry where it is (terrain, roads).
pub const IDENTITY: Instance = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

/// The turn part of an instance for a model of `(w, l)` tiles turned
/// `facing` quarters clockwise (see `Structure::facing`).
pub fn turn(facing: u8, (w, l): (u32, u32)) -> [f32; 4] {
    let (w, l) = (w as f32, l as f32);
    match facing % 4 {
        0 => [1.0, 0.0, 0.0, 0.0],
        1 => [0.0, 1.0, l, 0.0],
        2 => [-1.0, 0.0, w, l],
        _ => [0.0, -1.0, 0.0, w],
    }
}

const VERTEX: &str = r#"#version 100
attribute vec3 in_pos;
attribute vec2 in_uv;
attribute vec4 in_color;
attribute vec3 inst_pos;
attribute vec2 inst_shear;
attribute vec4 inst_turn;
uniform mat4 mvp;
uniform vec4 light;
uniform vec4 blend;
varying lowp vec4 color;
varying lowp vec2 uv;
void main() {
    vec3 p = vec3(inst_turn.x * in_pos.x - inst_turn.y * in_pos.z + inst_turn.z, in_pos.y, inst_turn.y * in_pos.x + inst_turn.x * in_pos.z + inst_turn.w);
    p.y += p.x * inst_shear.x + p.z * inst_shear.y;
    gl_Position = mvp * vec4(p + inst_pos, 1.0);
    vec4 c = in_color / 255.0;
    // The season's colour (snow, autumn) over the ground, then the light.
    c.rgb = mix(c.rgb, blend.rgb, blend.a);
    color = vec4(c.rgb * light.rgb, c.a);
    uv = in_uv;
}"#;

const FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D tex;
void main() {
    lowp vec4 c = texture2D(tex, uv) * color;
    if (c.a < 0.5) {
        discard;
    }
    gl_FragColor = c;
}"#;

#[repr(C)]
struct Uniforms {
    mvp: Mat4,
    light: Vec4,
    blend: Vec4,
}

/// Light (rgb tint) and a colour blended in (rgb, amount) for a draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    pub light: Vec4,
    pub blend: Vec4,
}

impl Default for Tint {
    fn default() -> Self {
        Self { light: Vec4::ONE, blend: Vec4::ZERO }
    }
}

pub struct GpuMesh {
    vb: BufferId,
    ib: BufferId,
    count: i32,
    tex: TextureId,
}

pub struct Gpu {
    pipeline: mq::Pipeline,
    white: TextureId,
    identity: BufferId,
}

fn ctx() -> &'static mut dyn mq::RenderingBackend {
    unsafe { get_internal_gl() }.quad_context
}

impl Gpu {
    /// `None` when the GPU lacks instancing (some GLES2 devices); the caller
    /// then keeps macroquad's immediate path.
    pub fn new() -> Option<Self> {
        let ctx = ctx();
        if !ctx.info().features.instancing {
            log::info!("GPU instancing unavailable, using the compatibility renderer");
            return None;
        }
        let meta = ShaderMeta {
            images: vec!["tex".to_string()],
            uniforms: UniformBlockLayout {
                uniforms: vec![
                    UniformDesc::new("mvp", UniformType::Mat4),
                    UniformDesc::new("light", UniformType::Float4),
                    UniformDesc::new("blend", UniformType::Float4),
                ],
            },
        };
        let shader = match ctx.new_shader(ShaderSource::Glsl { vertex: VERTEX, fragment: FRAGMENT }, meta) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("instanced shader failed ({e:?}), using the compatibility renderer");
                return None;
            }
        };
        let pipeline = ctx.new_pipeline(
            &[
                BufferLayout { stride: std::mem::size_of::<Vertex>() as i32, ..Default::default() },
                BufferLayout { stride: std::mem::size_of::<Instance>() as i32, step_func: VertexStep::PerInstance, step_rate: 1 },
            ],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float3),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Byte4),
                VertexAttribute::with_buffer("inst_pos", VertexFormat::Float3, 1),
                VertexAttribute::with_buffer("inst_shear", VertexFormat::Float2, 1),
                VertexAttribute::with_buffer("inst_turn", VertexFormat::Float4, 1),
            ],
            shader,
            PipelineParams { depth_test: Comparison::LessOrEqual, depth_write: true, ..Default::default() },
        );
        let white = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);
        let identity = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(&[IDENTITY]));
        Some(Self { pipeline, white, identity })
    }

    pub fn upload(&self, vertices: &[Vertex], indices: &[u16], texture: Option<&Texture2D>) -> GpuMesh {
        let ctx = ctx();
        GpuMesh {
            vb: ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(vertices)),
            ib: ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(indices)),
            count: indices.len() as i32,
            tex: texture.map_or(self.white, Texture2D::raw_miniquad_id),
        }
    }

    pub fn delete(&self, mesh: GpuMesh) {
        let ctx = ctx();
        ctx.delete_buffer(mesh.vb);
        ctx.delete_buffer(mesh.ib);
    }

    pub fn instances(&self, data: &[Instance]) -> BufferId {
        ctx().new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(data))
    }

    pub fn delete_buffer(&self, b: BufferId) {
        ctx().delete_buffer(b);
    }

    /// Starts drawing after everything macroquad has batched so far.
    pub fn begin(&self, mvp: Mat4, tint: Tint) {
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        gl.quad_context.begin_default_pass(PassAction::Nothing);
        gl.quad_context.apply_pipeline(&self.pipeline);
        self.tint(mvp, tint);
    }

    /// Changes the light and blend for the draws that follow.
    pub fn tint(&self, mvp: Mat4, tint: Tint) {
        ctx().apply_uniforms(UniformsSource::table(&Uniforms { mvp, light: tint.light, blend: tint.blend }));
    }

    pub fn draw(&self, mesh: &GpuMesh, instances: Option<(BufferId, usize)>) {
        let (buf, n) = instances.unwrap_or((self.identity, 1));
        if n == 0 || mesh.count == 0 {
            return;
        }
        let ctx = ctx();
        ctx.apply_bindings(&mq::Bindings { vertex_buffers: vec![mesh.vb, buf], index_buffer: mesh.ib, images: vec![mesh.tex] });
        ctx.draw(0, mesh.count, n as i32);
    }

    pub fn end(&self) {
        ctx().end_render_pass();
    }
}
