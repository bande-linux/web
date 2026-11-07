use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char, c_void};
use std::os::fd::{AsFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

use bande_graphics::{Cursor, Event, Modifiers, PointerButton, ResizeEdge, Scene, ScrollSource};
use bande_toolkit_platform::{Role, Surfaces, Wait, Window, WindowEvent, step_surfaces};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_client::{Connection, EventQueue};

pub const EVENT_WAKE: u32 = 1;
pub const EVENT_POINTER_ENTER: u32 = 2;
pub const EVENT_POINTER_LEAVE: u32 = 3;
pub const EVENT_POINTER_MOVE: u32 = 4;
pub const EVENT_POINTER_DOWN: u32 = 5;
pub const EVENT_POINTER_UP: u32 = 6;
pub const EVENT_SCROLL: u32 = 7;
pub const EVENT_KEY: u32 = 8;
pub const EVENT_FOCUS: u32 = 9;
pub const EVENT_CLOSE: u32 = 10;
pub const EVENT_TOPLEVEL_STATE: u32 = 11;
pub const EVENT_RESIZED: u32 = 12;
pub const EVENT_APPEARANCE: u32 = 13;

const MOD_SHIFT: u32 = 1;
const MOD_CONTROL: u32 = 2;
const MOD_ALT: u32 = 4;
const MOD_LOGO: u32 = 8;

const STATE_MAXIMIZED: u32 = 1;
const STATE_FULLSCREEN: u32 = 2;
const STATE_ACTIVATED: u32 = 4;
const STATE_TILED: u32 = 8;
const STATE_SUSPENDED: u32 = 16;

const BUTTON_PRIMARY: u32 = 0;
const BUTTON_MIDDLE: u32 = 1;
const BUTTON_SECONDARY: u32 = 2;
const BUTTON_BACK: u32 = 3;
const BUTTON_FORWARD: u32 = 4;

const SCROLL_WHEEL: u32 = 0;
const SCROLL_FINGER: u32 = 1;
const SCROLL_CONTINUOUS: u32 = 2;

#[repr(C)]
pub struct BandeEvent {
    pub kind: u32,
    pub window: u64,
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
    pub modifiers: u32,
    pub text: *const c_char,
}

#[derive(Default)]
struct Queued {
    kind: u32,
    window: u64,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    modifiers: u32,
    text: Option<CString>,
}

struct Idle;

struct Host {
    connection: Connection,
    idle: EventQueue<Idle>,
    wake: OwnedFd,
    windows: Vec<Window>,
    ids: Vec<u64>,
    next_id: u64,
    events: VecDeque<Queued>,
    current_text: Option<CString>,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn with_host<T>(fallback: T, action: impl FnOnce(&mut Host) -> T) -> T {
    HOST.with_borrow_mut(|host| match host {
        Some(host) => action(host),
        None => fallback,
    })
}

fn modifier_bits(modifiers: Modifiers) -> u32 {
    let bit = |set: bool, value: u32| if set { value } else { 0 };
    bit(modifiers.shift, MOD_SHIFT)
        | bit(modifiers.control, MOD_CONTROL)
        | bit(modifiers.alt, MOD_ALT)
        | bit(modifiers.logo, MOD_LOGO)
}

fn button_number(button: PointerButton) -> u32 {
    match button {
        PointerButton::Primary => BUTTON_PRIMARY,
        PointerButton::Middle => BUTTON_MIDDLE,
        PointerButton::Secondary => BUTTON_SECONDARY,
        PointerButton::Back => BUTTON_BACK,
        PointerButton::Forward => BUTTON_FORWARD,
    }
}

fn scroll_number(source: ScrollSource) -> u32 {
    match source {
        ScrollSource::Wheel => SCROLL_WHEEL,
        ScrollSource::Finger => SCROLL_FINGER,
        ScrollSource::Continuous => SCROLL_CONTINUOUS,
    }
}

fn pointer_event(window: u64, kind: u32, pointer: bande_graphics::Pointer) -> Queued {
    Queued {
        kind,
        window,
        x: f64::from(pointer.position.x),
        y: f64::from(pointer.position.y),
        a: button_number(pointer.button),
        b: u32::from(pointer.count),
        modifiers: modifier_bits(pointer.modifiers),
        ..Queued::default()
    }
}

impl Host {
    fn position(&self, id: u64) -> Option<usize> {
        self.ids.iter().position(|known| *known == id)
    }

    fn window(&mut self, id: u64) -> Option<&mut Window> {
        let index = self.position(id)?;
        self.windows.get_mut(index)
    }

    fn translate(&mut self, window: u64, event: WindowEvent) {
        let queued = match event {
            WindowEvent::Input(Event::PointerEnter(pointer)) => {
                pointer_event(window, EVENT_POINTER_ENTER, pointer)
            }
            WindowEvent::Input(Event::PointerLeave) => Queued {
                kind: EVENT_POINTER_LEAVE,
                window,
                ..Queued::default()
            },
            WindowEvent::Input(Event::PointerMove(pointer)) => {
                pointer_event(window, EVENT_POINTER_MOVE, pointer)
            }
            WindowEvent::Input(Event::PointerDown(pointer)) => {
                pointer_event(window, EVENT_POINTER_DOWN, pointer)
            }
            WindowEvent::Input(Event::PointerUp(pointer)) => {
                pointer_event(window, EVENT_POINTER_UP, pointer)
            }
            WindowEvent::Input(Event::Scroll(scroll)) => Queued {
                kind: EVENT_SCROLL,
                window,
                x: f64::from(scroll.position.x),
                y: f64::from(scroll.position.y),
                dx: f64::from(scroll.delta.x),
                dy: f64::from(scroll.delta.y),
                a: scroll_number(scroll.source),
                modifiers: modifier_bits(scroll.modifiers),
                ..Queued::default()
            },
            WindowEvent::KeyDetail(detail) => Queued {
                kind: EVENT_KEY,
                window,
                a: detail.code,
                b: u32::from(detail.pressed) | (u32::from(detail.repeat) << 1),
                c: detail.keysym,
                d: detail.base_keysym,
                modifiers: modifier_bits(detail.modifiers),
                text: CString::new(detail.text).ok(),
                ..Queued::default()
            },
            WindowEvent::Focus(focused) => Queued {
                kind: EVENT_FOCUS,
                window,
                a: u32::from(focused),
                ..Queued::default()
            },
            WindowEvent::CloseRequested | WindowEvent::QuitRequested => Queued {
                kind: EVENT_CLOSE,
                window,
                ..Queued::default()
            },
            WindowEvent::ToplevelChanged(state) => {
                let bit = |set: bool, value: u32| if set { value } else { 0 };
                Queued {
                    kind: EVENT_TOPLEVEL_STATE,
                    window,
                    a: bit(state.maximized, STATE_MAXIMIZED)
                        | bit(state.fullscreen, STATE_FULLSCREEN)
                        | bit(state.activated, STATE_ACTIVATED)
                        | bit(state.tiled, STATE_TILED)
                        | bit(state.suspended, STATE_SUSPENDED),
                    ..Queued::default()
                }
            }
            WindowEvent::HostedResized((width, height)) => Queued {
                kind: EVENT_RESIZED,
                window,
                a: width,
                b: height,
                ..Queued::default()
            },
            WindowEvent::AppearanceChanged(_) => Queued {
                kind: EVENT_APPEARANCE,
                window,
                ..Queued::default()
            },
            _ => return,
        };
        self.events.push_back(queued);
    }

    fn drain_wake(&mut self) {
        let mut bytes = [0_u8; 64];
        while matches!(rustix::io::read(&self.wake, &mut bytes), Ok(read) if read > 0) {}
        if !self
            .events
            .iter()
            .any(|queued| queued.kind == EVENT_WAKE)
        {
            self.events.push_back(Queued {
                kind: EVENT_WAKE,
                ..Queued::default()
            });
        }
    }

    fn step_idle(&mut self, block: bool) -> bool {
        let Some(guard) = self.idle.prepare_read() else {
            return false;
        };
        let connection_fd = guard.connection_fd();
        let wake = self.wake.as_fd();
        let mut fds = [
            PollFd::new(&connection_fd, PollFlags::IN),
            PollFd::new(&wake, PollFlags::IN),
        ];
        let zero = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if poll(&mut fds, (!block).then_some(&zero)).is_err() {
            return false;
        }
        let (wayland, woken) = (!fds[0].revents().is_empty(), !fds[1].revents().is_empty());
        if wayland && guard.read().is_err() {
            return false;
        }
        if woken {
            self.drain_wake();
        }
        true
    }

    fn step(&mut self, block: bool) -> bool {
        let before = self.events.len();
        if self.windows.is_empty() {
            return self.step_idle(block);
        }
        if step_surfaces(self, Wait::Poll).is_err() {
            return false;
        }
        if block && self.events.len() == before && step_surfaces(self, Wait::Block).is_err() {
            return false;
        }
        block && step_surfaces(self, Wait::Poll).is_ok() || !block
    }
}

impl Surfaces for Host {
    fn windows(&self) -> &[Window] {
        &self.windows
    }

    fn windows_mut(&mut self) -> &mut [Window] {
        &mut self.windows
    }

    fn scene(&mut self, _index: usize) -> Scene {
        Scene::new(1, 1)
    }

    fn event(&mut self, index: usize, event: WindowEvent) {
        if let Some(id) = self.ids.get(index).copied() {
            self.translate(id, event);
        }
    }

    fn sources(&self) -> Vec<BorrowedFd<'_>> {
        vec![self.wake.as_fd()]
    }

    fn source_ready(&mut self, _index: usize) {
        self.drain_wake();
    }

    fn closed(&mut self) -> bool {
        false
    }
}

fn text_of(text: *const c_char) -> String {
    match text.is_null() {
        true => String::new(),
        false => unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned(),
    }
}

fn backdrop_of(window: &Window) -> [u8; 4] {
    let chassis = window.appearance().palette().chassis;
    [chassis.b, chassis.g, chassis.r, u8::MAX]
}

/// # Safety
/// `wake_fd` is an open file descriptor whose ownership passes to this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_init(wake_fd: RawFd) -> *mut c_void {
    let Ok(connection) = Connection::connect_to_env() else {
        return std::ptr::null_mut();
    };
    let display = connection.backend().display_ptr().cast();
    let idle = connection.new_event_queue();
    let wake = unsafe { OwnedFd::from_raw_fd(wake_fd) };
    HOST.set(Some(Host {
        connection,
        idle,
        wake,
        windows: Vec::new(),
        ids: Vec::new(),
        next_id: 1,
        events: VecDeque::new(),
        current_text: None,
    }));
    display
}

/// # Safety
/// `title` and `app_id` are null or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_open(
    title: *const c_char,
    app_id: *const c_char,
    width: u32,
    height: u32,
) -> u64 {
    with_host(0, |host| {
        let role = Role::hosted(&text_of(title), &text_of(app_id));
        let opened = match host.windows.first() {
            Some(first) => first.open_beside(role, (width, height)),
            None => Window::open_with(host.connection.clone(), role, (width, height)),
        };
        let Ok(mut window) = opened else {
            return 0;
        };
        window.set_key_detail(true);
        let backdrop = backdrop_of(&window);
        if window.set_hosted_backdrop(backdrop).is_err() {
            return 0;
        }
        let id = host.next_id;
        host.next_id += 1;
        host.windows.push(window);
        host.ids.push(id);
        id
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_close(id: u64) {
    with_host((), |host| {
        if let Some(index) = host.position(id) {
            host.windows.remove(index);
            host.ids.remove(index);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_surface(id: u64) -> *mut c_void {
    with_host(std::ptr::null_mut(), |host| {
        host.window(id)
            .map_or(std::ptr::null_mut(), |window| window.surface_ptr())
    })
}

/// # Safety
/// `title` is null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_set_title(id: u64, title: *const c_char) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            window.set_title(&text_of(title));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_set_size(id: u64, width: u32, height: u32) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            let _ = window.set_hosted_size((width, height));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_size(id: u64, width: *mut u32, height: *mut u32) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            let (shown_width, shown_height) = window.size();
            unsafe {
                *width = shown_width;
                *height = shown_height;
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_set_limits(
    id: u64,
    min_width: u32,
    min_height: u32,
    max_width: u32,
    max_height: u32,
) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            window.set_hosted_size_limits((min_width, min_height), (max_width, max_height));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_scale(id: u64) -> f64 {
    with_host(1.0, |host| host.window(id).map_or(1.0, |window| window.scale()))
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_start_move(id: u64) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            let _ = window.start_move();
        }
    });
}

fn resize_edge(edge: u32) -> Option<ResizeEdge> {
    Some(match edge {
        0 => ResizeEdge::Top,
        1 => ResizeEdge::Bottom,
        2 => ResizeEdge::Left,
        3 => ResizeEdge::Right,
        4 => ResizeEdge::TopLeft,
        5 => ResizeEdge::TopRight,
        6 => ResizeEdge::BottomLeft,
        7 => ResizeEdge::BottomRight,
        _ => return None,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_start_resize(id: u64, edge: u32) {
    with_host((), |host| {
        if let (Some(window), Some(edge)) = (host.window(id), resize_edge(edge)) {
            let _ = window.start_resize(edge);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_minimize(id: u64) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            let _ = window.minimize();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_toggle_zoom(id: u64) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            let _ = window.toggle_zoom();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_set_full_screen(id: u64, full_screen: bool) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            let _ = window.set_full_screen(full_screen);
        }
    });
}

fn cursor_of(kind: u32) -> Cursor {
    match kind {
        1 => Cursor::Pointer,
        2 => Cursor::Text,
        3 => Cursor::Crosshair,
        4 => Cursor::Move,
        5 => Cursor::Grab,
        6 => Cursor::Grabbing,
        7 => Cursor::NotAllowed,
        8 => Cursor::ContextMenu,
        9 => Cursor::Copy,
        10 => Cursor::Alias,
        11 => Cursor::ColumnResize,
        12 => Cursor::RowResize,
        13 => Cursor::ResizeTop,
        14 => Cursor::ResizeBottom,
        15 => Cursor::ResizeLeft,
        16 => Cursor::ResizeRight,
        17 => Cursor::ResizeTopLeft,
        18 => Cursor::ResizeTopRight,
        19 => Cursor::ResizeBottomLeft,
        20 => Cursor::ResizeBottomRight,
        _ => Cursor::Default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_set_cursor(id: u64, kind: u32) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            window.set_cursor(cursor_of(kind));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_flush() {
    with_host((), |host| {
        let _ = host.connection.flush();
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_step(block: bool) -> bool {
    with_host(false, |host| host.step(block))
}

/// # Safety
/// `event` points to a writable `BandeEvent`. Its `text` stays valid until the next call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_next_event(event: *mut BandeEvent) -> bool {
    with_host(false, |host| {
        let Some(queued) = host.events.pop_front() else {
            return false;
        };
        host.current_text = queued.text;
        let text = host
            .current_text
            .as_ref()
            .map_or(std::ptr::null(), |text| text.as_ptr());
        unsafe {
            event.write(BandeEvent {
                kind: queued.kind,
                window: queued.window,
                x: queued.x,
                y: queued.y,
                dx: queued.dx,
                dy: queued.dy,
                a: queued.a,
                b: queued.b,
                c: queued.c,
                d: queued.d,
                modifiers: queued.modifiers,
                text,
            });
        }
        true
    })
}
