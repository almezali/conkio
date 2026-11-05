//! Native file drag-and-drop for Linux.
//!
//! Winit delivers X11 XDND as `WindowEvent::DroppedFile`, but Wayland compositors
//! (Hyprland, niri, COSMIC, GNOME, KDE) never emit that event. This module attaches
//! a second `wl_data_device` to the same Wayland connection as the Slint window so
//! `text/uri-list` drops from any file manager reach the app.

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    collections::HashSet,
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use wayland_backend::sys::client::Backend;
use wayland_client::{
    protocol::{
        wl_data_device::{self, WlDataDevice},
        wl_data_device_manager::{self, DndAction, WlDataDeviceManager},
        wl_data_offer::{self, WlDataOffer},
        wl_registry,
        wl_seat::{self, WlSeat},
    },
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
};

use crate::ConkioWindow;

const URI_MIMES: &[&str] = &[
    "text/uri-list",
    "application/x-kde-urilist",
    "text/x-moz-url",
    "text/plain;charset=utf-8",
    "text/plain",
    "text/x-uri",
];

#[derive(Default)]
struct DndInner {
    manager: Option<WlDataDeviceManager>,
    seats: Vec<WlSeat>,
    devices: Vec<WlDataDevice>,
    offer: Option<WlDataOffer>,
    offer_mimes: HashSet<String>,
    serial: u32,
    action_chosen: bool,
    hovering: bool,
    pending_drop: bool,
    dropped: Vec<PathBuf>,
}

/// Per-offer user data: MIME types arrive on the offer *before* `Enter`.
#[derive(Default)]
struct OfferData {
    mimes: Mutex<Vec<String>>,
}

pub struct WaylandDnd {
    queue: EventQueue<DndInner>,
    state: DndInner,
}

impl WaylandDnd {
    pub fn attach(display: *mut std::ffi::c_void) -> Option<Self> {
        if display.is_null() {
            return None;
        }
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let mut queue = conn.new_event_queue::<DndInner>();
        let qh = queue.handle();
        let _registry = conn.display().get_registry(&qh, ());
        let mut state = DndInner::default();
        queue.roundtrip(&mut state).ok()?;
        if let Some(manager) = state.manager.clone() {
            for seat in &state.seats {
                state.devices.push(manager.get_data_device(seat, &qh, ()));
            }
        }
        if state.devices.is_empty() {
            return None;
        }
        let _ = queue.roundtrip(&mut state);
        Some(Self { queue, state })
    }

    pub fn dispatch(&mut self) -> (bool, Vec<PathBuf>) {
        let _ = self.queue.dispatch_pending(&mut self.state);
        // dispatch_pending() only handles messages already read from the
        // socket. Poll and read the dedicated Wayland connection without
        // blocking the Slint event loop, otherwise Enter/Drop never reaches
        // DndInner on compositors such as KDE Plasma.
        let _ = self.queue.flush();
        if let Some(guard) = self.queue.prepare_read() {
            let mut pollfd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&mut pollfd, 1, 0) } > 0 {
                let _ = guard.read();
                let _ = self.queue.dispatch_pending(&mut self.state);
            }
        }
        if self.state.pending_drop {
            let paths = receive_offer(&mut self.queue, &mut self.state);
            self.state.dropped.extend(paths);
            self.state.pending_drop = false;
            self.state.hovering = false;
            if let Some(offer) = self.state.offer.take() {
                // finish() is a protocol error unless an action was negotiated,
                // and a protocol error kills the shared Wayland connection.
                if self.state.action_chosen {
                    offer.finish();
                }
                offer.destroy();
            }
            self.state.offer_mimes.clear();
            self.state.action_chosen = false;
            let _ = self.queue.flush();
            let _ = self.queue.dispatch_pending(&mut self.state);
        }
        let hovering = self.state.hovering;
        let dropped = std::mem::take(&mut self.state.dropped);
        (hovering, dropped)
    }
}

fn preferred_mime(state: &DndInner) -> Option<String> {
    URI_MIMES
        .iter()
        .find(|m| state.offer_mimes.iter().any(|have| have.eq_ignore_ascii_case(m)))
        .map(|s| (*s).to_string())
        .or_else(|| state.offer_mimes.iter().cloned().next())
}

fn accept_offer(state: &DndInner) {
    let Some(offer) = state.offer.clone() else { return };
    let mime = preferred_mime(state);
    offer.accept(state.serial, mime);
    offer.set_actions(DndAction::Copy, DndAction::Copy);
}

fn receive_offer(queue: &mut EventQueue<DndInner>, state: &mut DndInner) -> Vec<PathBuf> {
    let Some(offer) = state.offer.clone() else { return Vec::new() };
    let Some(mime) = preferred_mime(state) else { return Vec::new() };
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Vec::new();
    }
    let rd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let wr = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    offer.receive(mime, wr.as_fd());
    drop(wr);
    let _ = queue.flush();
    let _ = queue.roundtrip(state);

    // Read with a deadline so a stalled source can never freeze the UI.
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis() as i32;
        if left == 0 {
            break;
        }
        let mut pfd = libc::pollfd { fd: rd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let r = unsafe { libc::poll(&mut pfd, 1, left.min(100)) };
        if r < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if r == 0 {
            continue;
        }
        let n = unsafe { libc::read(rd.as_raw_fd(), chunk.as_mut_ptr().cast(), chunk.len()) };
        if n <= 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
        if buf.len() > (1 << 20) {
            break;
        }
    }
    parse_dropped_text(&String::from_utf8_lossy(&buf))
}

impl Dispatch<wl_registry::WlRegistry, ()> for DndInner {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_data_device_manager" => {
                    state.manager = Some(registry.bind(name, version.min(3), qh, ()));
                }
                "wl_seat" => {
                    state.seats.push(registry.bind(name, version.min(7), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlDataDeviceManager, ()> for DndInner {
    fn event(
        _: &mut Self,
        _: &WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for DndInner {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDevice, ()> for DndInner {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // The new wl_data_offer is created via event_created_child below;
            // its MIME types are stored in its OfferData until Enter/Selection.
            wl_data_device::Event::DataOffer { .. } => {}
            wl_data_device::Event::Enter { serial, id, .. } => {
                if let Some(old) = state.offer.take() {
                    old.destroy();
                }
                state.serial = serial;
                state.hovering = true;
                state.pending_drop = false;
                state.action_chosen = false;
                state.offer_mimes.clear();
                if let Some(offer) = id {
                    if let Some(data) = offer.data::<OfferData>() {
                        if let Ok(m) = data.mimes.lock() {
                            state.offer_mimes = m.iter().cloned().collect::<HashSet<String>>();
                        }
                    }
                    state.offer = Some(offer);
                    accept_offer(state);
                }
            }
            wl_data_device::Event::Leave => {
                // After Drop the offer is still needed for receive(); no Leave
                // should follow a Drop, but never discard it if one does.
                if !state.pending_drop {
                    state.hovering = false;
                    if let Some(offer) = state.offer.take() {
                        offer.destroy();
                    }
                    state.offer_mimes.clear();
                    state.action_chosen = false;
                }
            }
            wl_data_device::Event::Motion { .. } => {}
            wl_data_device::Event::Drop => {
                state.pending_drop = true;
            }
            wl_data_device::Event::Selection { id } => {
                // Clipboard offer: not interesting for DnD.
                if let Some(offer) = id {
                    offer.destroy();
                }
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(DndInner, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, OfferData::default())
    ]);
}

impl Dispatch<WlDataOffer, OfferData> for DndInner {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        data: &OfferData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_offer::Event::Offer { mime_type } => {
                if let Ok(mut m) = data.mimes.lock() {
                    m.push(mime_type.clone());
                }
                // Only the active DnD offer feeds the live state.
                if state.offer.as_ref().map(|o| o == offer).unwrap_or(false) {
                    state.offer_mimes.insert(mime_type);
                }
            }
            wl_data_offer::Event::SourceActions { .. } => {}
            wl_data_offer::Event::Action { dnd_action } => {
                if state.offer.as_ref().map(|o| o == offer).unwrap_or(false) {
                    state.action_chosen =
                        matches!(dnd_action, WEnum::Value(a) if !a.is_empty());
                }
            }
            _ => {}
        }
    }
}

pub fn parse_dropped_text(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in text.split(|c| c == '\n' || c == '\r' || c == '\0') {
        let token = raw.trim().trim_matches('"').trim_matches('\'');
        if token.is_empty() || token.starts_with('#') {
            continue;
        }
        for path in decode_drop_token(token) {
            if seen.insert(path.clone()) {
                out.push(path);
            }
        }
    }
    out
}

fn decode_drop_token(token: &str) -> Vec<PathBuf> {
    let mut value = token.trim().to_string();
    if value.to_lowercase().starts_with("file://") {
        value = percent_decode(&strip_file_uri(&value));
    } else if value.contains('%') {
        value = percent_decode(&value);
    }
    if value.is_empty() {
        return Vec::new();
    }
    vec![PathBuf::from(value)]
}

fn strip_file_uri(uri: &str) -> String {
    let rest = uri.get(7..).unwrap_or(uri);
    if let Some(stripped) = rest.strip_prefix("//") {
        if let Some(slash) = stripped.find('/') {
            let host = &stripped[..slash];
            if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
                return stripped[slash..].to_string();
            }
        }
        return stripped.to_string();
    }
    rest.to_string()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(val);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn emit_paths(ui: &ConkioWindow, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let values: Vec<SharedString> = paths
        .into_iter()
        .map(|p| SharedString::from(p.to_string_lossy().as_ref()))
        .collect();
    ui.invoke_files_dropped(ModelRc::new(VecModel::from(values)));
}

pub fn install_native_file_drop_hook(ui: &ConkioWindow) {
    let weak = ui.as_weak();
    let wayland: Arc<Mutex<Option<WaylandDnd>>> = Arc::new(Mutex::new(None));
    let wayland_ev = wayland.clone();
    let attempted = Arc::new(AtomicBool::new(false));
    ui.window().on_winit_window_event(move |window, event| {
        if matches!(
            event,
            winit::event::WindowEvent::KeyboardInput { event, .. }
                if event.state == winit::event::ElementState::Pressed
                    && matches!(
                        event.physical_key,
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
                    )
        ) {
            if let Some(ui) = weak.upgrade() {
                ui.set_context_visible(false);
                ui.set_show_marketplace(false);
                ui.set_show_download(false);
                ui.set_show_settings(false);
                ui.set_show_about_overlay(false);
                ui.set_show_prompt(false);
            }
            return EventResult::PreventDefault;
        }
        if !attempted.load(Ordering::Relaxed) {
            attempted.store(true, Ordering::Relaxed);
            let attached = window
                .with_winit_window(|winit_window| {
                    let Ok(handle) = winit_window.display_handle() else { return None };
                    match handle.as_raw() {
                        RawDisplayHandle::Wayland(dpy) => WaylandDnd::attach(dpy.display.as_ptr()),
                        _ => None,
                    }
                })
                .flatten();
            if let Some(dnd) = attached {
                if let Ok(mut slot) = wayland_ev.lock() {
                    *slot = Some(dnd);
                }
            }
        }
        match event {
            winit::event::WindowEvent::HoveredFile(_) => {
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_active(true);
                }
            }
            winit::event::WindowEvent::HoveredFileCancelled => {
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_active(false);
                }
            }
            winit::event::WindowEvent::DroppedFile(path) => {
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_active(false);
                    let paths = parse_dropped_text(&path.to_string_lossy());
                    let paths = if paths.is_empty() { vec![path.clone()] } else { paths };
                    emit_paths(&ui, paths);
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });

    let weak = ui.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(32), move || {
        let mut pending = Vec::new();
        let mut hovering = false;
        if let Ok(mut slot) = wayland.lock() {
            if let Some(dnd) = slot.as_mut() {
                let (hover, dropped) = dnd.dispatch();
                hovering = hover;
                pending = dropped;
            }
        }
        if let Some(ui) = weak.upgrade() {
            if hovering || ui.get_drop_active() {
                ui.set_drop_active(hovering);
            }
            if !pending.is_empty() {
                ui.set_drop_active(false);
                emit_paths(&ui, pending);
            }
        }
    });
    std::mem::forget(timer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_file_uri_and_percent() {
        let paths = parse_dropped_text("file:///tmp/My%20Theme.zip\nfile:///tmp/other.conf\n");
        assert!(paths.iter().any(|p| p.ends_with("My Theme.zip")));
        assert!(paths.iter().any(|p| p.ends_with("other.conf")));
    }

    #[test]
    fn ignores_comments_and_empty() {
        assert!(parse_dropped_text("# comment\n\n").is_empty());
    }
}
