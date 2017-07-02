use gfx;
use gfx::Device;
use gfx::Factory;
use gfx::traits::FactoryExt;
use gfx_window_glutin;
use glutin;
use gfx_device_gl;
use genmesh::generators::{Plane, SharedVertex, IndexedPolygon};
use genmesh::{Triangulate, Vertices};
use image;

use resources::{self, TILE_SHEET_SPEC, TILE_SHEET_IMAGE};
use tile_buffer::TileBufferCell;
use simple_file;
use tile_desc::TileDesc;

pub type ColourFormat = gfx::format::Srgba8;
pub type DepthFormat = gfx::format::DepthStencil;

const WIDTH_TILES: u32 = 20;
const HEIGHT_TILES: u32 = 20;
const TILE_SIZE: u32 = 48;
const TILE_IDX_BITS: u32 = 5;

const WIDTH_PX: u32 = WIDTH_TILES * TILE_SIZE;
const HEIGHT_PX: u32 = HEIGHT_TILES * TILE_SIZE;
const NUM_TILES: u32 = WIDTH_TILES * HEIGHT_TILES;

const CLEAR_COLOUR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

gfx_defines!{
    vertex Vertex {
        pos: [f32; 2] = "a_Pos",
        tex_pos: [f32; 2] = "a_TexPos",
        cell_pos: [f32; 2] = "a_CellPos",
    }

    constant TileMapData {
        data: [f32; 4] = "data",
    }

    constant TileMapInfo {
        ratio: [f32; 2] = "u_TexRatio",
    }

    pipeline pipe {
        vbuf: gfx::VertexBuffer<Vertex> = (),
        tex: gfx::TextureSampler<[f32; 4]> = "t_Texture",
        tile_table: gfx::ConstantBuffer<TileMapData> = "b_TileMap",
        tile_map_info: gfx::ConstantBuffer<TileMapInfo> = "b_TileMapInfo",
        out: gfx::BlendTarget<ColourFormat> =
            ("Target0", gfx::state::MASK_ALL, gfx::preset::blend::ALPHA),
    }
}

impl TileMapData {
    fn new_empty() -> Self {
        let status: f32 = f32::from_bits((1<<5) | 1);
        let first: f32 = f32::from_bits((2 << 8) | 0);
        TileMapData {
            data: [first, 0.0, 0.0, status],
        }
    }
}

impl TileMapInfo {
    fn new(tile_size: u32, tex_width: u32, tex_height: u32) -> Self {
        TileMapInfo {
            ratio: [
                tile_size as f32 / tex_width as f32,
                tile_size as f32 / tex_height as f32,
            ]
        }
    }
}

impl<'a> From<&'a TileBufferCell> for TileMapData {
    fn from(_cell: &TileBufferCell) -> Self {
        unimplemented!()
    }
}

pub struct GlutinGameRenderer {
    encoder: gfx::Encoder<gfx_device_gl::Resources, gfx_device_gl::CommandBuffer>,
    window: glutin::Window,
    device: gfx_device_gl::Device,
    slice: gfx::Slice<gfx_device_gl::Resources>,
    pso: gfx::PipelineState<gfx_device_gl::Resources, pipe::Meta>,
    pipeline_data: pipe::Data<gfx_device_gl::Resources>,
    tile_map: Vec<TileMapData>,
}

pub struct GlutinGameInput {
    events_loop: glutin::EventsLoop,
}

pub fn create() -> (GlutinGameRenderer, GlutinGameInput) {

    let tile_path = resources::resource_path(TILE_SHEET_IMAGE);
    let img = image::open(tile_path).expect("failed to open image").to_rgba();
    let (img_width, img_height) = img.dimensions();

    let builder = glutin::WindowBuilder::new()
        .with_decorations(true)
        .with_dimensions(WIDTH_PX, HEIGHT_PX)
        .with_min_dimensions(WIDTH_PX, HEIGHT_PX)
        .with_max_dimensions(WIDTH_PX, HEIGHT_PX)
        .with_title("Veil".to_string());

    let events_loop = glutin::EventsLoop::new();

    let (window, device, mut factory, colour_view, _main_depth) =
        gfx_window_glutin::init::<ColourFormat, DepthFormat>(builder, &events_loop);

    let tex_kind = gfx::texture::Kind::D2(img_width as u16, img_height as u16, gfx::texture::AaMode::Single);
    let (_, texture) = factory.create_texture_immutable_u8::<ColourFormat>(tex_kind, &[&img])
        .expect("Failed to create texture");
    let sampler = factory.create_sampler_linear();

    let pso = factory.create_pipeline_simple(
        include_bytes!("shaders/shdr_330.vert"),
        include_bytes!("shaders/shdr_330.frag"),
        pipe::new()
    ).expect("Failed to create pipeline");

    let mut encoder: gfx::Encoder<_, _> = factory.create_command_buffer().into();


    let plane = Plane::subdivide(1, 1);

    let vertex_data: Vec<Vertex> = plane.shared_vertex_iter().map(|vertex| {

            let raw_x = vertex.pos[0];
            let raw_y = vertex.pos[1];

            let x = raw_x / 2.0 + 0.5;
            let y = 0.5 - raw_y / 2.0;

            Vertex {
                pos: [raw_x, raw_y],
                tex_pos: [x, y],
                cell_pos: [x * WIDTH_TILES as f32, y * HEIGHT_TILES as f32],
            }
        })
        .collect();

    let index_data: Vec<u32> = plane.indexed_polygon_iter()
        .triangulate()
        .vertices()
        .map(|i| i as u32)
        .collect();

    let (vertex_buffer, slice) = factory.create_vertex_buffer_with_slice(&vertex_data, &index_data[..]);

    let tile_buffer = factory.create_constant_buffer(NUM_TILES as usize);
    let tile_map_info = factory.create_constant_buffer(1);

    let data = pipe::Data {
        vbuf: vertex_buffer,
        tex: (texture, sampler),
        tile_map_info: tile_map_info,
        tile_table: tile_buffer,
        out: colour_view,
    };

    let tile_desc: TileDesc = simple_file::read_toml(&resources::resource_path(TILE_SHEET_SPEC))
        .expect("Failed to read tile spec");

    encoder.update_buffer(&data.tile_map_info, &[TileMapInfo::new(tile_desc.tile_size_scaled(), img_width, img_height)], 0)
        .expect("Failed to update texture ratio");

    let mut tile_map = Vec::new();
    for _ in 0..NUM_TILES {
        tile_map.push(TileMapData::new_empty());
    }

    let renderer = GlutinGameRenderer {
        encoder: encoder,
        window: window,
        device: device,
        slice: slice,
        pso: pso,
        pipeline_data: data,
        tile_map: tile_map,
    };

    let input = GlutinGameInput {
        events_loop: events_loop,
    };

    (renderer, input)
}

pub fn example(renderer: &mut GlutinGameRenderer, input: &mut GlutinGameInput) {
    'main: loop {
        let mut running = true;
        input.events_loop.poll_events(|e| {
            let event = if let glutin::Event::WindowEvent { event, .. } = e {
                event
            } else {
                return;
            };
            match event {
                glutin::WindowEvent::Closed => running = false,
                _ => {}
            }
        });

        if !running {
            break;
        }

        renderer.encoder.clear(&renderer.pipeline_data.out, CLEAR_COLOUR);

        renderer.encoder.update_buffer(&renderer.pipeline_data.tile_table, &renderer.tile_map, 0)
            .expect("Failed to update tile buffer");

        renderer.encoder.draw(&renderer.slice, &renderer.pso, &renderer.pipeline_data);
        renderer.encoder.flush(&mut renderer.device);
        renderer.window.swap_buffers().expect("Failed to swap buffers");
        renderer.device.cleanup();
    }

}
