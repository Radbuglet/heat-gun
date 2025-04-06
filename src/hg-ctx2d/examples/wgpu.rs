use std::sync::Arc;

use anyhow::Context as _;
use futures::executor::block_on;
use glam::{UVec2, Vec2, Vec4};
use hg_ctx2d::{
    base::{AssetManager, FinishDescriptor, GfxContext},
    facade::Canvas,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;

    event_loop.run_app(&mut App { state: None })?;

    Ok(())
}

#[derive(Debug)]
struct App {
    state: Option<AppState>,
}

#[derive(Debug)]
struct AppState {
    gfx: GfxContext,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    depth_texture: Option<wgpu::Texture>,
    surface_size: UVec2,
    surface_format: Option<wgpu::TextureFormat>,
    canvas: Canvas,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            match block_on(Self::init_app(event_loop)) {
                Ok(state) => self.state = Some(state),
                Err(err) => {
                    eprintln!("{err:?}");
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        assert_eq!(window_id, state.window.id());

        match event {
            WindowEvent::RedrawRequested => {
                if let Err(err) = Self::draw_surface(state) {
                    eprintln!("WARN: {err:?}");
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }
}

impl App {
    async fn init_app(event_loop: &ActiveEventLoop) -> anyhow::Result<AppState> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::debugging(),
            backend_options: wgpu::BackendOptions::default(),
        });

        let window = Arc::new(
            event_loop.create_window(
                WindowAttributes::default()
                    .with_title("Ctx2D WGPU Example")
                    .with_visible(false),
            )?,
        );

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .context("failed to find suitable adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await?;

        let gfx = GfxContext::new(adapter, device, queue);
        let mut app = AppState {
            gfx: gfx.clone(),
            window,
            surface,
            depth_texture: None,
            surface_size: UVec2::new(u32::MAX, u32::MAX),
            surface_format: None,
            canvas: Canvas::new(AssetManager::new(), gfx),
        };

        app.window.set_visible(true);
        Self::maybe_reconfigure_surface(&mut app);

        Ok(app)
    }

    fn draw_surface(state: &mut AppState) -> anyhow::Result<()> {
        Self::maybe_reconfigure_surface(state);

        state.gfx.device.start_capture();

        let frame = state.surface.get_current_texture()?;

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = state.depth_texture.as_ref().unwrap();
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = state
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        fastrand::seed(4);

        for _ in 0..1000 {
            state.canvas.fill_rect(
                Vec2::new(fastrand::f32() - 0.5, fastrand::f32() - 0.5),
                Vec2::new(0.2, 0.2),
                Vec4::new(1., 0., 1.0, 1.0),
            );
        }

        state.canvas.finish(FinishDescriptor {
            encoder: &mut encoder,
            color_attachment: &frame_view,
            color_format: frame.texture.format(),
            color_load: wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 1.,
            }),
            depth_attachment: &depth_view,
            depth_format: depth_texture.format(),
            width: frame.texture.width(),
            height: frame.texture.height(),
        });

        state.gfx.queue.submit([encoder.finish()]);
        state.canvas.reclaim();
        frame.present();

        state.gfx.device.stop_capture();

        Ok(())
    }

    fn maybe_reconfigure_surface(state: &mut AppState) {
        let win_size = UVec2::new(
            state.window.inner_size().width,
            state.window.inner_size().height,
        );

        if win_size == state.surface_size {
            return;
        }

        let config = state
            .surface
            .get_default_config(&state.gfx.adapter, win_size.x, win_size.y)
            .unwrap();

        state.surface.configure(&state.gfx.device, &config);

        state.surface_size = win_size;
        state.surface_format = Some(config.format);

        state.depth_texture = Some(state.gfx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: win_size.x,
                height: win_size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    }
}
