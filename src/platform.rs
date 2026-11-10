//! What the game asks of the system around it: a window with an OpenGL context, and a way to report a failure
//! to start.

use std::ffi::CString;
use std::num::NonZeroU32;

use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow as _};
use raw_window_handle::HasWindowHandle;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

use crate::Result;

/// A window and its OpenGL 4.0 core context, current on the calling thread.
pub struct GlWindow {
    // The surface and the context go before the window they belong to.
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    pub window: Window,
}

impl GlWindow {
    /// Gives the window a double-buffered pixel format, with 8x or 4x multisampling where the driver has it,
    /// and makes an OpenGL 4.0 core context current on it, with vertical sync where the driver allows.
    pub fn new(event_loop: &ActiveEventLoop, attributes: WindowAttributes) -> Result<GlWindow> {
        // The pixel formats come with 8 bits for each color, a depth of 24 bits and a stencil of 8.
        let best = |configs: Box<dyn Iterator<Item = Config> + '_>| configs.min_by_key(rank).expect("the display offers at least one pixel format");
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(event_loop, ConfigTemplateBuilder::new(), best)
            .map_err(|error| format!("No OpenGL pixel format is available for the window: {error}"))?;
        let window = window.ok_or("The window could not be created.")?;
        let handle = window.window_handle()?.as_raw();
        #[cfg(windows)]
        let config = find_multisampled(&config, handle).unwrap_or(config);
        let display = config.display();

        let context_attributes =
            ContextAttributesBuilder::new().with_context_api(ContextApi::OpenGl(Some(Version::new(4, 0)))).with_profile(GlProfile::Core).build(Some(handle));
        let context = unsafe { display.create_context(&config, &context_attributes) }
            .map_err(|error| format!("OpenGL 4.0 is not available. Update the graphics driver.\n\n{error}"))?;

        let surface_attributes = window.build_surface_attributes(Default::default())?;
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes)? };
        let context = context.make_current(&surface)?;
        gl::load_with(|name| CString::new(name).map_or(std::ptr::null(), |name| display.get_proc_address(&name)));

        // Vertical sync: swapping the buffers waits for the screen refresh.
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN));
        Ok(GlWindow { surface, context, window })
    }

    pub fn swap_buffers(&self) {
        let _ = self.surface.swap_buffers(&self.context);
    }

    /// Follows the window's size; some systems size the surface apart from the window.
    pub fn resize(&self, width: u32, height: u32) {
        if let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            self.surface.resize(&self.context, width, height);
        }
    }
}

/// How a pixel format suits the game, the less the better: 8 samples for a pixel, or else 4, or else as few as
/// there are.
fn rank(config: &Config) -> (u8, u8) {
    let samples = config.num_samples();
    let group = match samples {
        8.. => 0,
        4.. => 1,
        _ => 2,
    };
    (group, samples)
}

/// Windows lists only so many pixel formats, the plain ones first; a multisampled one is asked for by its
/// samples when the list had none.
#[cfg(windows)]
fn find_multisampled(found: &Config, handle: raw_window_handle::RawWindowHandle) -> Option<Config> {
    let display = found.display();
    [8, 4].into_iter().take_while(|samples| found.num_samples() < *samples).find_map(|samples| {
        let template = ConfigTemplateBuilder::new().with_multisampling(samples).compatible_with_native_window(handle).build();
        let configs = unsafe { display.find_configs(template) }.ok()?;
        // A driver without multisampling answers with the plain formats.
        configs.filter(|config| config.num_samples() >= samples).min_by_key(rank)
    })
}

/// Tells the user why the game could not start.
#[cfg(windows)]
pub fn show_error(title: &str, message: &str) {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    const MB_ICONERROR: u32 = 0x10;

    let wide = |text: &str| std::ffi::OsStr::new(text).encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (message, title) = (wide(message), wide(title));
    unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), MB_ICONERROR) };
}

#[cfg(not(windows))]
pub fn show_error(title: &str, message: &str) {
    eprintln!("{title}: {message}");
}
