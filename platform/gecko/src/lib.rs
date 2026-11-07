use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char, c_void};
use std::os::fd::{AsFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

use bande_graphics::{Color, Cursor, Rect, Event, Modifiers, PointerButton, ResizeEdge, Scene, ScrollSource};
use bande_toolkit_platform::{DragAction, DragEvent, Role, Surfaces, Transfer, Wait, Window, WindowEvent, step_surfaces};
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
pub const EVENT_CLOSE: u32 = 10;
pub const EVENT_TOPLEVEL_STATE: u32 = 11;
pub const EVENT_RESIZED: u32 = 12;
pub const EVENT_APPEARANCE: u32 = 13;
pub const EVENT_FD_READY: u32 = 14;
pub const EVENT_CLIPBOARD_DATA: u32 = 15;
pub const EVENT_CLIPBOARD_CHANGED: u32 = 16;
pub const EVENT_POPUP_MOVED: u32 = 17;
pub const EVENT_POPUP_DISMISSED: u32 = 18;
pub const EVENT_DRAG_ENTER: u32 = 19;
pub const EVENT_DRAG_MOVE: u32 = 20;
pub const EVENT_DRAG_LEAVE: u32 = 21;
pub const EVENT_DRAG_DROP: u32 = 22;
pub const EVENT_DRAG_FINISHED: u32 = 23;
pub const EVENT_IME_PREEDIT: u32 = 24;
pub const EVENT_IME_COMMIT: u32 = 25;

const DRAG_COPY: u32 = 1;
const DRAG_MOVE: u32 = 2;

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
    pub data: *const u8,
    pub len: usize,
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
    data: Vec<u8>,
}

struct Idle;

struct Host {
    connection: Connection,
    idle: EventQueue<Idle>,
    wake: OwnedFd,
    watched: Vec<RawFd>,
    windows: Vec<Window>,
    ids: Vec<u64>,
    parents: Vec<(u64, u64)>,
    text_areas: Vec<(u64, Rect)>,
    next_id: u64,
    events: VecDeque<Queued>,
    current_text: Option<CString>,
    current_data: Vec<u8>,
    focused: Option<u64>,
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

fn action_number(action: Option<DragAction>) -> u32 {
    match action {
        Some(DragAction::Copy) => DRAG_COPY,
        Some(DragAction::Move) => DRAG_MOVE,
        None => 0,
    }
}

fn action_of(number: u32) -> Option<DragAction> {
    match number {
        DRAG_COPY => Some(DragAction::Copy),
        DRAG_MOVE => Some(DragAction::Move),
        _ => None,
    }
}

fn drag_event(window: u64, drag: DragEvent) -> Queued {
    match drag {
        DragEvent::Entered {
            position,
            uris,
            text,
        } => Queued {
            kind: EVENT_DRAG_ENTER,
            window,
            x: f64::from(position.x),
            y: f64::from(position.y),
            a: u32::from(text),
            data: uris.join("\n").into_bytes(),
            ..Queued::default()
        },
        DragEvent::Moved(position) => Queued {
            kind: EVENT_DRAG_MOVE,
            window,
            x: f64::from(position.x),
            y: f64::from(position.y),
            ..Queued::default()
        },
        DragEvent::Left => Queued {
            kind: EVENT_DRAG_LEAVE,
            window,
            ..Queued::default()
        },
        DragEvent::Dropped {
            position,
            transfer,
            action,
        } => Queued {
            kind: EVENT_DRAG_DROP,
            window,
            x: f64::from(position.x),
            y: f64::from(position.y),
            a: action_number(action),
            text: CString::new(transfer.text).ok(),
            data: transfer.uris.join("\n").into_bytes(),
            ..Queued::default()
        },
        DragEvent::Finished(action) => Queued {
            kind: EVENT_DRAG_FINISHED,
            window,
            a: action_number(action),
            ..Queued::default()
        },
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
                window: self.toplevel_of(window),
                a: detail.code,
                b: u32::from(detail.pressed) | (u32::from(detail.repeat) << 1),
                c: detail.keysym,
                d: detail.base_keysym,
                modifiers: modifier_bits(detail.modifiers),
                text: CString::new(detail.text).ok(),
                ..Queued::default()
            },
            WindowEvent::PastedData { mime, bytes } => Queued {
                kind: EVENT_CLIPBOARD_DATA,
                window,
                text: CString::new(mime).ok(),
                data: bytes,
                ..Queued::default()
            },
            WindowEvent::ClipboardChanged => Queued {
                kind: EVENT_CLIPBOARD_CHANGED,
                window,
                ..Queued::default()
            },
            WindowEvent::HostedMoved((x, y)) => Queued {
                kind: EVENT_POPUP_MOVED,
                window,
                x: f64::from(x),
                y: f64::from(y),
                ..Queued::default()
            },
            WindowEvent::Drag(drag) => drag_event(window, drag),
            WindowEvent::Input(Event::Commit(text)) => Queued {
                kind: EVENT_IME_COMMIT,
                window: self.toplevel_of(window),
                text: CString::new(text).ok(),
                ..Queued::default()
            },
            WindowEvent::Input(Event::Preedit { text, cursor }) => Queued {
                kind: EVENT_IME_PREEDIT,
                window: self.toplevel_of(window),
                a: cursor.map_or(0, |(start, _)| u32::try_from(start).unwrap_or(0) + 1),
                b: cursor.map_or(0, |(_, end)| u32::try_from(end).unwrap_or(0) + 1),
                text: CString::new(text).ok(),
                ..Queued::default()
            },
            WindowEvent::PopupDismissed => Queued {
                kind: EVENT_POPUP_DISMISSED,
                window,
                ..Queued::default()
            },
            WindowEvent::CloseRequested | WindowEvent::QuitRequested => Queued {
                kind: EVENT_CLOSE,
                window,
                ..Queued::default()
            },
            WindowEvent::ToplevelChanged(state) => {
                let bit = |set: bool, value: u32| if set { value } else { 0 };
                if state.activated {
                    self.focused = Some(window);
                }
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

    fn toplevel_of(&self, window: u64) -> u64 {
        let mut current = window;
        while let Some((_, parent)) = self.parents.iter().find(|(child, _)| *child == current) {
            current = *parent;
        }
        current
    }

    fn clipboard_window(&mut self) -> Option<&mut Window> {
        let index = self
            .focused
            .and_then(|id| self.position(id))
            .unwrap_or(0);
        self.windows.get_mut(index)
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
        let sources = self.watched_fds();
        let mut fds = vec![PollFd::new(&connection_fd, PollFlags::IN)];
        fds.extend(sources.iter().map(|fd| PollFd::new(fd, PollFlags::IN)));
        let zero = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if poll(&mut fds, (!block).then_some(&zero)).is_err() {
            return false;
        }
        let wayland = !fds[0].revents().is_empty();
        let ready: Vec<usize> = (0..sources.len())
            .filter(|at| !fds[at + 1].revents().is_empty())
            .collect();
        drop(fds);
        drop(sources);
        if wayland && guard.read().is_err() {
            return false;
        }
        for index in ready {
            self.source_is_ready(index);
        }
        true
    }

    fn watched_fds(&self) -> Vec<BorrowedFd<'_>> {
        let mut fds = vec![self.wake.as_fd()];
        fds.extend(
            self.watched
                .iter()
                .map(|fd| unsafe { BorrowedFd::borrow_raw(*fd) }),
        );
        fds
    }

    fn source_is_ready(&mut self, index: usize) {
        match index.checked_sub(1).and_then(|at| self.watched.get(at)) {
            Some(fd) => self.events.push_back(Queued {
                kind: EVENT_FD_READY,
                a: u32::try_from(*fd).unwrap_or(0),
                ..Queued::default()
            }),
            None => self.drain_wake(),
        }
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
        Scene {
            width: 1,
            height: 1,
            background: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            items: Vec::new(),
        }
    }

    fn event(&mut self, index: usize, event: WindowEvent) {
        if let Some(id) = self.ids.get(index).copied() {
            self.translate(id, event);
        }
    }

    fn text_input_area(&self, index: usize) -> Option<Rect> {
        let id = self.ids.get(index)?;
        self.text_areas
            .iter()
            .find(|(window, _)| window == id)
            .map(|(_, area)| *area)
    }

    fn sources(&self) -> Vec<BorrowedFd<'_>> {
        self.watched_fds()
    }

    fn source_ready(&mut self, index: usize) {
        self.source_is_ready(index);
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
        watched: Vec::new(),
        windows: Vec::new(),
        ids: Vec::new(),
        parents: Vec::new(),
        text_areas: Vec::new(),
        next_id: 1,
        events: VecDeque::new(),
        current_text: None,
        current_data: Vec::new(),
        focused: None,
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

fn area(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(x as f32, y as f32, width as f32, height as f32)
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_open_popup(
    parent: u64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    grab: bool,
) -> u64 {
    with_host(0, |host| {
        let Some(parent_window) = host.window(parent) else {
            return 0;
        };
        let Ok(role) = parent_window.hosted_popup(area(x, y, width, height), grab) else {
            return 0;
        };
        let size = (width.round().max(1.0) as u32, height.round().max(1.0) as u32);
        let Ok(mut window) = parent_window.open_beside(role, size) else {
            return 0;
        };
        window.set_key_detail(true);
        if window.set_hosted_backdrop([0, 0, 0, 0]).is_err() {
            return 0;
        }
        let id = host.next_id;
        host.next_id += 1;
        host.windows.push(window);
        host.ids.push(id);
        host.parents.push((id, parent));
        id
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_move_popup(id: u64, x: f64, y: f64, width: f64, height: f64) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            let size = (width.round().max(1.0) as u32, height.round().max(1.0) as u32);
            let _ = window.set_hosted_size(size);
            let _ = window.move_hosted_popup(area(x, y, width, height));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_close(id: u64) {
    with_host((), |host| {
        if let Some(index) = host.position(id) {
            host.windows.remove(index);
            host.ids.remove(index);
        }
        host.parents.retain(|(child, _)| *child != id);
        if host.focused == Some(id) {
            host.focused = None;
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
pub extern "C" fn bande_gecko_watch_fd(fd: RawFd) {
    with_host((), |host| host.watched.push(fd));
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_unwatch_fd(fd: RawFd) {
    with_host((), |host| host.watched.retain(|known| *known != fd));
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
        host.current_data = queued.data;
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
                data: host.current_data.as_ptr(),
                len: host.current_data.len(),
            });
        }
        true
    })
}

unsafe fn transfer_of(
    count: usize,
    mimes: *const *const c_char,
    data: *const *const u8,
    lengths: *const usize,
) -> Transfer {
    let mut transfer = Transfer::default();
    for index in 0..count {
        let (mime, bytes) = unsafe {
            (
                text_of(*mimes.add(index)),
                std::slice::from_raw_parts(*data.add(index), *lengths.add(index)).to_vec(),
            )
        };
        match mime.as_str() {
            "text/plain;charset=utf-8" => transfer.text = String::from_utf8_lossy(&bytes).into_owned(),
            "image/png" => transfer.png = bytes,
            "text/uri-list" => {
                transfer.uris = String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(str::to_owned)
                    .collect();
            }
            _ => transfer = transfer.with_data(&mime, bytes),
        }
    }
    transfer
}

/// # Safety
/// `mimes`, `data` and `lengths` each point to `count` valid entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_copy(
    primary: bool,
    concealed: bool,
    count: usize,
    mimes: *const *const c_char,
    data: *const *const u8,
    lengths: *const usize,
) {
    let mut transfer = unsafe { transfer_of(count, mimes, data, lengths) };
    if concealed {
        transfer = transfer.concealed();
    }
    with_host((), |host| {
        if let Some(window) = host.clipboard_window() {
            match primary {
                true => window.select_primary(&transfer.text),
                false => window.copy_transfer(transfer),
            }
        }
    });
}

/// # Safety
/// `out` points to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_clipboard_types(out: *mut u8, capacity: usize) -> usize {
    with_host(0, |host| {
        let types = host
            .clipboard_window()
            .map(|window| window.clipboard_types().join("\n"))
            .unwrap_or_default();
        let bytes = types.as_bytes();
        let length = bytes.len().min(capacity);
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, length) };
        length
    })
}

/// # Safety
/// `mime` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_paste(mime: *const c_char) -> bool {
    let mime = text_of(mime);
    with_host(false, |host| match host.clipboard_window() {
        Some(window) => window.paste_data(&mime).is_ok(),
        None => false,
    })
}

/// # Safety
/// `mimes`, `data` and `lengths` each point to `count` valid entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bande_gecko_start_drag(
    id: u64,
    actions: u32,
    count: usize,
    mimes: *const *const c_char,
    data: *const *const u8,
    lengths: *const usize,
) -> bool {
    let transfer = unsafe { transfer_of(count, mimes, data, lengths) };
    let allowed: Vec<DragAction> = [DRAG_COPY, DRAG_MOVE]
        .into_iter()
        .filter(|bit| actions & bit != 0)
        .filter_map(action_of)
        .collect();
    with_host(false, |host| match host.window(id) {
        Some(window) => window.start_drag(transfer, &allowed, None).is_ok(),
        None => false,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_accept_drag(id: u64, action: u32) {
    with_host((), |host| {
        if let Some(window) = host.window(id) {
            window.accept_drag(action_of(action));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn bande_gecko_set_text_input(
    id: u64,
    enabled: bool,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    with_host((), |host| {
        host.text_areas.retain(|(window, _)| *window != id);
        if enabled {
            host.text_areas.push((id, area(x, y, width, height)));
        }
    });
}
