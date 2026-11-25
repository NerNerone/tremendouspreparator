//! Appunti su Wayland nativo. Slint usa il protocollo «data-control», che i compositori non
//! danno alle app nella sandbox (Flatpak): copia e incolla non funzionerebbero. Qui il backend
//! di Slint resta lo stesso, ma copia e incolla passano dal protocollo Wayland standard
//! (`wl_data_device`, quello di GTK e Qt) con smithay-clipboard. Fuori da Wayland (X11) si
//! usano gli appunti di Slint.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use i_slint_backend_winit::Backend;
use i_slint_core::platform::{Clipboard, EventLoopProxy, Platform, PlatformError};
use i_slint_core::window::WindowAdapter;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

pub struct WaylandClipboardPlatform {
    inner: Backend,
    /// finestre create, per arrivare alla connessione Wayland
    windows: RefCell<Vec<Weak<dyn WindowAdapter>>>,
    clip: RefCell<Option<smithay_clipboard::Clipboard>>,
}

/// Id dell'app (Flatpak, file .desktop, icona).
pub const APP_ID: &str = "io.github.nernerone.TremendousPreparator";

impl WaylandClipboardPlatform {
    pub fn new() -> Result<Self, PlatformError> {
        // app_id (Wayland) e classe della finestra (X11) uguali all'id dell'app: così KDE e GNOME
        // la collegano al suo file .desktop e nella barra delle app mostrano la sua icona (senza,
        // compariva l'icona generica di Wayland)
        let inner = Backend::builder()
            .with_window_attributes_hook(|a| {
                let id = APP_ID;
                #[allow(unused_mut)]
                let mut a = a;
                #[cfg(not(target_os = "android"))]
                {
                    a = i_slint_backend_winit::winit::platform::wayland::WindowAttributesExtWayland::with_name(
                        a, id, id,
                    );
                    a = i_slint_backend_winit::winit::platform::x11::WindowAttributesExtX11::with_name(a, id, id);
                }
                a
            })
            .build()?;
        Ok(Self { inner, windows: RefCell::default(), clip: RefCell::default() })
    }

    /// Lo stesso Wayland delle finestre dell'app (`None` su X11 o prima della prima finestra).
    fn with_clip<R>(&self, f: impl FnOnce(&smithay_clipboard::Clipboard) -> R) -> Option<R> {
        if self.clip.borrow().is_none() {
            let display = self.windows.borrow().iter().filter_map(Weak::upgrade).find_map(|w| {
                let handle = w.window().window_handle();
                match handle.display_handle().ok()?.as_raw() {
                    RawDisplayHandle::Wayland(h) => Some(h.display.as_ptr()),
                    _ => None,
                }
            })?;
            // SAFETY: la connessione Wayland delle finestre dura quanto l'app
            *self.clip.borrow_mut() = Some(unsafe { smithay_clipboard::Clipboard::new(display) });
        }
        self.clip.borrow().as_ref().map(f)
    }
}

impl Platform for WaylandClipboardPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let w = self.inner.create_window_adapter()?;
        self.windows.borrow_mut().push(Rc::downgrade(&w));
        Ok(w)
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        self.inner.run_event_loop()
    }

    fn process_events(
        &self,
        timeout: Option<core::time::Duration>,
        token: i_slint_core::InternalToken,
    ) -> Result<core::ops::ControlFlow<()>, PlatformError> {
        self.inner.process_events(timeout, token)
    }

    fn bind_context(&self, ctx: i_slint_core::SlintContextWeak, token: i_slint_core::InternalToken) {
        self.inner.bind_context(ctx, token)
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        self.inner.new_event_loop_proxy()
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.inner.duration_since_start()
    }

    fn click_interval(&self) -> core::time::Duration {
        self.inner.click_interval()
    }

    fn cursor_flash_cycle(&self) -> core::time::Duration {
        self.inner.cursor_flash_cycle()
    }

    fn debug_log(&self, arguments: core::fmt::Arguments) {
        self.inner.debug_log(arguments)
    }

    fn open_url(&self, url: &str) -> Result<(), PlatformError> {
        self.inner.open_url(url)
    }

    fn set_clipboard_text(&self, text: &str, clipboard: Clipboard) {
        let done = self.with_clip(|c| match clipboard {
            Clipboard::DefaultClipboard => c.store(text),
            _ => c.store_primary(text),
        });
        if done.is_none() {
            self.inner.set_clipboard_text(text, clipboard);
        }
    }

    fn clipboard_text(&self, clipboard: Clipboard) -> Option<String> {
        match self.with_clip(|c| match clipboard {
            Clipboard::DefaultClipboard => c.load(),
            _ => c.load_primary(),
        }) {
            Some(r) => r.ok(),
            None => self.inner.clipboard_text(clipboard),
        }
    }
}
