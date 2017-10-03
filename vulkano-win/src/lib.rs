extern crate vulkano;
extern crate winit;

#[cfg(target_os = "macos")]
extern crate objc;
#[cfg(target_os = "macos")]
extern crate cocoa;
#[cfg(target_os = "macos")]
extern crate metal_rs as metal;

use std::error;
use std::fmt;
use std::ops::Deref;
#[cfg(target_os = "windows")]
use std::ptr;
use std::sync::Arc;

use vulkano::instance::Instance;
use vulkano::instance::InstanceExtensions;
use vulkano::swapchain::Surface;
use vulkano::swapchain::SurfaceCreationError;
use vulkano::swapchain::WindowAbstract;
use winit::{EventsLoop, WindowBuilder};
use winit::CreationError as WindowCreationError;

#[cfg(target_os = "macos")]
use cocoa::appkit::{NSView, NSWindow};
#[cfg(target_os = "macos")]
use cocoa::base::id as cocoa_id;
#[cfg(target_os = "macos")]
use metal::*;
#[cfg(target_os = "macos")]
use objc::runtime::YES;

#[cfg(target_os = "macos")]
use std::mem;

pub fn required_extensions() -> InstanceExtensions {
    let ideal = InstanceExtensions {
        khr_surface: true,
        khr_xlib_surface: true,
        khr_xcb_surface: true,
        khr_wayland_surface: true,
        khr_mir_surface: true,
        khr_android_surface: true,
        khr_win32_surface: true,
        mvk_ios_surface: true,
        mvk_macos_surface: true,
        ..InstanceExtensions::none()
    };

    match InstanceExtensions::supported_by_core() {
        Ok(supported) => supported.intersection(&ideal),
        Err(_) => InstanceExtensions::none(),
    }
}

pub trait VkSurfaceBuild {
    fn build_vk_surface(self, events_loop: &EventsLoop, instance: Arc<Instance>)
                        -> Result<Window, CreationError>;
}

impl VkSurfaceBuild for WindowBuilder {
    fn build_vk_surface(self, events_loop: &EventsLoop, instance: Arc<Instance>)
                        -> Result<Window, CreationError> {
        let underlying_window = self.build(events_loop)?;
        let window = Arc::new(WinitWindow(underlying_window));
        let surface = unsafe { winit_to_surface(instance, window.clone()) }?;

        Ok(Window {
               window: window,
               surface: surface,
           })
    }
}

pub struct WinitWindow(winit::Window);

impl WindowAbstract for WinitWindow {
}

impl Deref for WinitWindow {
    type Target = winit::Window;

    fn deref(&self) -> &winit::Window {
        &self.0
    }
}

pub struct Window {
    window: Arc<WinitWindow>,
    surface: Arc<Surface>,
}

impl Window {
    #[inline]
    pub fn window(&self) -> &winit::Window {
        &self.window
    }

    #[inline]
    pub fn surface(&self) -> &Arc<Surface> {
        &self.surface
    }
}

/// Error that can happen when creating a window.
#[derive(Debug)]
pub enum CreationError {
    /// Error when creating the surface.
    SurfaceCreationError(SurfaceCreationError),
    /// Error when creating the window.
    WindowCreationError(WindowCreationError),
}

impl error::Error for CreationError {
    #[inline]
    fn description(&self) -> &str {
        match *self {
            CreationError::SurfaceCreationError(_) => "error while creating the surface",
            CreationError::WindowCreationError(_) => "error while creating the window",
        }
    }

    #[inline]
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            CreationError::SurfaceCreationError(ref err) => Some(err),
            CreationError::WindowCreationError(ref err) => Some(err),
        }
    }
}

impl fmt::Display for CreationError {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", error::Error::description(self))
    }
}

impl From<SurfaceCreationError> for CreationError {
    #[inline]
    fn from(err: SurfaceCreationError) -> CreationError {
        CreationError::SurfaceCreationError(err)
    }
}

impl From<WindowCreationError> for CreationError {
    #[inline]
    fn from(err: WindowCreationError) -> CreationError {
        CreationError::WindowCreationError(err)
    }
}

#[cfg(target_os = "android")]
unsafe fn winit_to_surface(instance: Arc<Instance>, win: Arc<WinitWindow>)
                           -> Result<Arc<Surface>, SurfaceCreationError> {
    use winit::os::android::WindowExt;
    Surface::from_anativewindow(instance, win.get_native_window(), Some(win))
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
unsafe fn winit_to_surface(instance: Arc<Instance>, win: Arc<WinitWindow>)
                           -> Result<Arc<Surface>, SurfaceCreationError> {
    use winit::os::unix::WindowExt;
    match (win.get_wayland_display(), win.get_wayland_surface()) {
        (Some(display), Some(surface)) => Surface::from_wayland(instance,
                                                                display,
                                                                surface,
                                                                Some(win)),
        _ => {
            // No wayland display found, check if we can use xlib.
            // If not, we use xcb.
            if instance.loaded_extensions().khr_xlib_surface {
                Surface::from_xlib(instance,
                                   win.get_xlib_display().unwrap(),
                                   win.get_xlib_window().unwrap() as _,
                                   Some(win))
            } else {
                Surface::from_xcb(instance,
                                  win.get_xcb_connection().unwrap(),
                                  win.get_xlib_window().unwrap() as _,
                                  Some(win))
            }
        },
    }
}

#[cfg(target_os = "windows")]
unsafe fn winit_to_surface(instance: Arc<Instance>, win: Arc<WinitWindow>)
                           -> Result<Arc<Surface>, SurfaceCreationError> {
    use winit::os::windows::WindowExt;
    Surface::from_hwnd(instance,
                       ptr::null() as *const (), // FIXME
                       win.get_hwnd(),
                       Some(win))
}

#[cfg(target_os = "macos")]
unsafe fn winit_to_surface(instance: Arc<Instance>, win: Arc<WinitWindow>)
                           -> Result<Arc<Surface>, SurfaceCreationError> {
    use winit::os::macos::WindowExt;

    let wnd: cocoa_id = mem::transmute(win.get_nswindow());

    let layer = CAMetalLayer::new();

    layer.set_edge_antialiasing_mask(0);
    layer.set_presents_with_transaction(false);
    layer.remove_all_animations();

    let view = wnd.contentView();

    layer.set_contents_scale(view.backingScaleFactor());
    view.setLayer(mem::transmute(layer.0)); // Bombs here with out of memory
    view.setWantsLayer(YES);

    Surface::from_macos_moltenvk(instance, win.get_nsview() as *const (), Some(win))
}
