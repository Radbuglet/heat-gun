use std::env;

use cocoa::{appkit::NSView, base::id as cocoa_id};
use core_graphics_types::geometry::CGSize;
use foreign_types_shared::ForeignType;
use foreign_types_shared::ForeignTypeRef;
use metal_rs::{
    CaptureDescriptor, CaptureManager, CommandQueue, Device, MTLCaptureDestination, MTLPixelFormat,
    MetalLayer,
};
use objc::{rc::autoreleasepool, runtime::YES};
use skia_safe::{
    gpu::{self, backend_render_targets, mtl, DirectContext, SurfaceOrigin},
    images::deferred_from_encoded_data,
    scalar, Canvas, Color4f, ColorType, Data, Paint, Rect,
};
use winit::keyboard::{Key, NamedKey};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};
use winit::{
    dpi::{LogicalSize, Size},
    raw_window_handle::HasWindowHandle,
    window::{Window, WindowAttributes},
};

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    struct Application {
        context: Option<Context>,
        should_capture: bool,
    }

    let mut application = Application {
        context: None,
        should_capture: false,
    };

    impl ApplicationHandler for Application {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            assert!(self.context.is_none());
            self.context = Some(Context::new(event_loop))
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            let context = self.context.as_mut().unwrap();

            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    context
                        .metal_layer
                        .set_drawable_size(CGSize::new(size.width as f64, size.height as f64));
                    context.window.request_redraw()
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.logical_key == Key::Named(NamedKey::Space) && event.state.is_pressed()
                    {
                        println!("Capturing!");
                        self.should_capture = true;
                        context.window.request_redraw();
                    }
                }
                WindowEvent::RedrawRequested => {
                    if self.should_capture {
                        let descriptor = CaptureDescriptor::new();
                        descriptor.set_output_url(&format!(
                            "file://{}",
                            env::current_dir()
                                .unwrap()
                                .join("capture.gputrace")
                                .display()
                        ));
                        descriptor.set_destination(MTLCaptureDestination::GpuTraceDocument);
                        descriptor.set_capture_device(&context.device);

                        CaptureManager::shared().start_capture(&descriptor).unwrap();
                    }

                    if let Some(drawable) = context.metal_layer.next_drawable() {
                        let (drawable_width, drawable_height) = {
                            let size = context.metal_layer.drawable_size();
                            (size.width as scalar, size.height as scalar)
                        };

                        let mut surface = unsafe {
                            let texture_info =
                                mtl::TextureInfo::new(drawable.texture().as_ptr() as mtl::Handle);

                            let backend_render_target = backend_render_targets::make_mtl(
                                (drawable_width as i32, drawable_height as i32),
                                &texture_info,
                            );

                            gpu::surfaces::wrap_backend_render_target(
                                &mut context.skia,
                                &backend_render_target,
                                SurfaceOrigin::TopLeft,
                                ColorType::BGRA8888,
                                None,
                                None,
                            )
                            .unwrap()
                        };

                        draw(surface.canvas());

                        context.skia.flush_and_submit();
                        drop(surface);

                        let command_buffer = context.command_queue.new_command_buffer();
                        command_buffer.present_drawable(drawable);
                        command_buffer.commit();
                    }

                    if self.should_capture {
                        self.should_capture = false;
                        CaptureManager::shared().stop_capture();
                    }
                }
                _ => (),
            }
        }
    }

    // As of winit 0.30.0, this is crashing on exit:
    // https://github.com/rust-windowing/winit/issues/3668
    autoreleasepool(|| {
        event_loop.run_app(&mut application).expect("run() failed");
    })
}

struct Context {
    pub window: Window,
    pub metal_layer: MetalLayer,
    pub command_queue: CommandQueue,
    pub skia: DirectContext,
    pub device: Device,
}

impl Context {
    fn new(event_loop: &ActiveEventLoop) -> Self {
        let size = LogicalSize::new(800, 600);
        let mut window_attributes = WindowAttributes::default();
        window_attributes.inner_size = Some(Size::new(size));
        window_attributes.title = "Skia Metal Winit Example".to_string();

        let window = event_loop
            .create_window(window_attributes)
            .expect("Failed to create Window");

        let window_handle = window
            .window_handle()
            .expect("Failed to retrieve a window handle");

        let raw_window_handle = window_handle.as_raw();

        let device = Device::system_default().expect("no device found");

        let metal_layer = {
            let draw_size = window.inner_size();
            let layer = MetalLayer::new();
            layer.set_device(&device);
            layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            layer.set_presents_with_transaction(false);
            // Disabling this option allows Skia's Blend Mode to work.
            // More about: https://developer.apple.com/documentation/quartzcore/cametallayer/1478168-framebufferonly
            layer.set_framebuffer_only(false);

            unsafe {
                let view = match raw_window_handle {
                    raw_window_handle::RawWindowHandle::AppKit(appkit) => appkit.ns_view.as_ptr(),
                    _ => panic!("Wrong window handle type"),
                } as cocoa_id;
                view.setWantsLayer(YES);
                view.setLayer(layer.as_ref() as *const _ as _);
            }
            layer.set_drawable_size(CGSize::new(draw_size.width as f64, draw_size.height as f64));
            layer
        };

        let command_queue = device.new_command_queue();

        let backend = unsafe {
            mtl::BackendContext::new(
                device.as_ptr() as mtl::Handle,
                command_queue.as_ptr() as mtl::Handle,
            )
        };

        let skia_context = gpu::direct_contexts::make_metal(&backend, None).unwrap();

        Self {
            window,
            metal_layer,
            command_queue,
            skia: skia_context,
            device,
        }
    }
}

/// Renders a rectangle that occupies exactly half of the canvas
fn draw(canvas: &Canvas) {
    let canvas_size = skia_safe::Size::from(canvas.base_layer_size());

    canvas.clear(Color4f::new(1.0, 1.0, 1.0, 1.0));

    let afl = deferred_from_encoded_data(Data::new_copy(include_bytes!("afl.jpg")), None).unwrap();

    let holland =
        deferred_from_encoded_data(Data::new_copy(include_bytes!("holland.jpg")), None).unwrap();

    fastrand::seed(4);

    for _ in 0..1000 {
        let x = fastrand::f32() * canvas_size.width;
        let y = fastrand::f32() * canvas_size.height;

        canvas.draw_image_rect(
            if fastrand::bool() { &afl } else { &holland },
            None,
            Rect::from_xywh(x, y, 100., 100.),
            &Paint::new(Color4f::new(1., 1., 1., 1.), None),
        );
    }
}
