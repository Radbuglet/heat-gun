use std::sync::Arc;

use anyhow::Context as _;
use futures::executor::block_on;
use glam::UVec2;
use hg_ctx2d::{wgpu::WgpuContext, Context as _};
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
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,

    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_size: UVec2,
    surface_format: Option<wgpu::TextureFormat>,

    renderer: Option<WgpuContext>,
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

        let mut app = AppState {
            adapter,
            device,
            queue,

            window,
            surface,
            surface_size: UVec2::new(u32::MAX, u32::MAX),
            surface_format: None,
            renderer: None,
        };

        app.window.set_visible(true);
        Self::maybe_reconfigure_surface(&mut app);

        app.renderer = Some(WgpuContext::new(
            app.device.clone(),
            app.surface_format.unwrap(),
        ));

        Ok(app)
    }

    fn draw_surface(state: &mut AppState) -> anyhow::Result<()> {
        Self::maybe_reconfigure_surface(state);

        state.device.start_capture();

        let renderer = state.renderer.as_mut().unwrap();
        renderer.reset();
        renderer.fill_rect(0.0, 0.0, 0.1, 0.1);
        renderer.fill_rect(0.2, 0.3, 0.05, 0.1);
        renderer.prepare(&state.queue);

        let frame = state.surface.get_current_texture()?;

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.,
                        g: 0.1,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            // TODO: Depth texture
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        renderer.render(&mut pass);

        drop(pass);

        state.queue.submit([encoder.finish()]);
        frame.present();

        state.device.stop_capture();

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
            .get_default_config(&state.adapter, win_size.x, win_size.y)
            .unwrap();

        state.surface.configure(&state.device, &config);

        state.surface_size = win_size;
        state.surface_format = Some(config.format);
    }
}
