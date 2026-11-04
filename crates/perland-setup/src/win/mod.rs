//! The Windows shell: window, input, accessibility, and the effects the state machine asks for.
//! Every decision is in `model`; this module only carries events in and effects out.

mod a11y;
mod log;
mod system;

use crate::draw::Paths;
use crate::draw::{self, Ui, HEIGHT, WIDTH};
use crate::model::{
    offer, silent_plan, Action, Effect, ErrorCode, Event, Machine, Pacer, Screen, SilentPlan,
};
use crate::options::{installer_args, resolve_folder, shortcuts_to_remove, Choice};
use crate::payload::{embedded, PayloadDir};
use crate::uninstall::{
    cleanup_command, credential_filter, data_targets, delete_paths, dir_size, mode,
    uninstaller_args, Mode, UEffect, UEvent, UMachine, UScreen, CREDENTIAL_SERVICE,
};
use crate::{text, VERSION};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{CursorIcon, Icon, Window, WindowId};

const FRAME: Duration = Duration::from_millis(16);
/// The Done screen stays this long before it fades, so the player sees it worked.
const OPENING_HOLD: Duration = Duration::from_millis(900);
const FADE: Duration = Duration::from_millis(200);

#[derive(Debug)]
enum UserEvent {
    Model(Event),
    Uninstall(UEvent),
    Access(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        UserEvent::Access(e)
    }
}

/// Runs the setup and returns the process exit code.
pub fn run() -> i32 {
    // One setup at a time: two would run the installer into the same folder at once.
    let Some(_instance) = system::SingleInstance::claim() else {
        system::focus_window(text::strings(system::ui_lang()).window_title);
        return 0;
    };
    log::line(&format!("Perland Launcher Setup {VERSION}"));
    if std::env::args()
        .skip(1)
        .any(|a| a.eq_ignore_ascii_case("/S"))
    {
        return run_silent();
    }
    let installed = system::installed();
    log::line(&format!("installed: {installed:?}"));
    let machine = Machine::new(
        offer(installed.as_ref().map(|(v, _)| v.as_str()), VERSION),
        embedded().is_some(),
    );
    let folders = system::folders();
    let paths = Paths {
        install: installed
            .as_ref()
            .and_then(|(_, exe)| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| folders.local_appdata.join("Perland Launcher")),
        data: folders.roaming_appdata.join("PerlandLauncher"),
        installed_bytes: crate::options::installed_bytes(),
        choice: Choice::default(),
        data_bytes: None,
        delete_data: false,
    };
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            log::line(&format!("no event loop: {e}"));
            return 1;
        }
    };
    let proxy = event_loop.create_proxy();
    let title = text::strings(system::ui_lang()).window_title;
    let mut app = App::new(Flow::Setup(machine), proxy, paths, folders, title);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::line(&format!("event loop ended with {e}"));
        return 1;
    }
    0
}

/// `/S`: no window, never closes a running launcher, never downgrades, never opens the launcher.
fn run_silent() -> i32 {
    let installed = system::installed().map(|(v, _)| v);
    let plan = silent_plan(installed.as_deref(), VERSION, system::launcher_running());
    log::line(&format!("silent: installed {installed:?}, plan {plan:?}"));
    let SilentPlan::Run = plan else {
        let SilentPlan::Exit(code) = plan else {
            unreachable!()
        };
        return code;
    };
    let Some(bytes) = embedded() else {
        log::line("silent: no payload");
        return 1;
    };
    let payload = match PayloadDir::write(bytes, &std::env::temp_dir()) {
        Ok(payload) => payload,
        Err(e) => {
            log::line(&format!("silent: write failed: {e}"));
            return 1;
        }
    };
    let code = run_installer(
        &payload,
        &installer_args(&Choice::default(), installed.is_none()),
    );
    log::line(&format!("silent: installer exited {code}"));
    remove_payload(payload);
    code
}

fn remove_payload(payload: PayloadDir) {
    let dir = payload.dir().to_path_buf();
    if let Err(e) = payload.remove() {
        log::line(&format!("could not remove {}: {e}", dir.display()));
    }
}

fn run_installer(payload: &PayloadDir, args: &[String]) -> i32 {
    log::line(&format!("installer arguments: {args:?}"));
    match std::process::Command::new(payload.exe())
        .args(args)
        .status()
    {
        Ok(status) => status.code().unwrap_or(-1),
        Err(e) => {
            log::line(&format!("installer did not start: {e}"));
            -1
        }
    }
}

struct Graphics {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    access: accesskit_winit::Adapter,
}

/// Which app this window is: the setup or the uninstaller.
enum Flow {
    Setup(Machine),
    Uninstall(Uninstalling),
}

struct Uninstalling {
    machine: UMachine,
    /// The install folder being removed.
    dir: PathBuf,
    /// The temp folder this copy of the uninstaller runs from, removed after it exits.
    own_temp: Option<PathBuf>,
}

struct App {
    flow: Flow,
    title: &'static str,
    folders: system::Folders,
    ui: Ui,
    proxy: EventLoopProxy<UserEvent>,
    graphics: Option<Graphics>,
    payload: Arc<Mutex<Option<PayloadDir>>>,
    error: Option<ErrorCode>,
    pressed: Option<Action>,
    shift: bool,
    cursor_y: f32,
    pacer: Pacer,
    /// The progress bar's width in whole pixels when last drawn, so a tick redraws only on change.
    drawn_progress: i32,
    /// When the Opening screen starts fading, and when the window closes.
    closing: Option<Instant>,
}

impl App {
    fn new(
        flow: Flow,
        proxy: EventLoopProxy<UserEvent>,
        paths: Paths,
        folders: system::Folders,
        title: &'static str,
    ) -> App {
        let ui = Ui {
            lang: system::ui_lang(),
            payload_version: VERSION.into(),
            hover: None,
            focus: None,
            fade: 1.0,
            paths,
        };
        App {
            flow,
            title,
            folders,
            ui,
            proxy,
            graphics: None,
            payload: Arc::new(Mutex::new(None)),
            error: None,
            pressed: None,
            shift: false,
            cursor_y: f32::MAX,
            pacer: Pacer::default(),
            drawn_progress: -1,
            closing: None,
        }
    }

    fn screen(&self) -> &Screen {
        match &self.flow {
            Flow::Setup(m) => &m.screen,
            Flow::Uninstall(u) => &u.machine.screen,
        }
    }

    fn scale(&self) -> f64 {
        self.graphics
            .as_ref()
            .map(|g| g.window.scale_factor())
            .unwrap_or(1.0)
    }

    fn redraw(&mut self) {
        let tree = a11y::tree(self.screen(), &self.ui, self.scale(), self.title);
        if let Some(g) = self.graphics.as_mut() {
            g.access.update_if_active(|| tree);
            g.window.request_redraw();
        }
    }

    fn paint(&mut self) {
        let screen = match &self.flow {
            Flow::Setup(m) => &m.screen,
            Flow::Uninstall(u) => &u.machine.screen,
        };
        let ui = &self.ui;
        let Some(g) = self.graphics.as_mut() else {
            return;
        };
        let size = g.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if g.surface.resize(w, h).is_err() {
            return;
        }
        let pixmap = draw::render(screen, ui, g.window.scale_factor() as f32);
        let Ok(mut buffer) = g.surface.buffer_mut() else {
            return;
        };
        let (pw, ph) = (pixmap.width() as usize, pixmap.height() as usize);
        let data = pixmap.data();
        for y in 0..(h.get() as usize) {
            for x in 0..(w.get() as usize) {
                let (sx, sy) = (x.min(pw - 1), y.min(ph - 1));
                let i = (sy * pw + sx) * 4;
                let (r, g, b) = (data[i] as u32, data[i + 1] as u32, data[i + 2] as u32);
                buffer[y * w.get() as usize + x] = (r << 16) | (g << 8) | b;
            }
        }
        let _ = buffer.present();
    }

    fn handle(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        let tick = matches!(event, Event::Tick(_));
        if !tick {
            log::line(&format!("event {event:?}"));
        }
        let Flow::Setup(machine) = &mut self.flow else {
            return;
        };
        let effects = machine.on(event);
        if !tick {
            log::line(&format!("screen {:?}, effects {effects:?}", machine.screen));
        }
        if let Screen::Error(code) = machine.screen {
            self.error = Some(code);
        }
        self.ui.paths.choice = machine.choice.clone();
        for effect in effects {
            self.run(event_loop, effect);
        }
        let progress = match self.screen() {
            Screen::Installing { progress, .. } => (progress * draw::BAR_WIDTH).round() as i32,
            _ => -1,
        };
        if !tick || progress != self.drawn_progress {
            self.drawn_progress = progress;
            self.redraw();
        }
    }

    fn handle_uninstall(&mut self, event_loop: &ActiveEventLoop, event: UEvent) {
        log::line(&format!("event {event:?}"));
        let Flow::Uninstall(u) = &mut self.flow else {
            return;
        };
        let effects = u.machine.on(event);
        log::line(&format!(
            "screen {:?}, effects {effects:?}",
            u.machine.screen
        ));
        self.ui.paths.delete_data = u.machine.delete_data;
        self.ui.paths.data_bytes = u.machine.data_bytes;
        for effect in effects {
            self.run_uninstall_effect(event_loop, effect);
        }
        self.redraw();
    }

    fn send_uninstall_later(&self, work: impl FnOnce() -> UEvent + Send + 'static) {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let event = work();
            let _ = proxy.send_event(UserEvent::Uninstall(event));
        });
    }

    fn run_uninstall_effect(&mut self, event_loop: &ActiveEventLoop, effect: UEffect) {
        let Flow::Uninstall(u) = &self.flow else {
            return;
        };
        let dir = u.dir.clone();
        match effect {
            UEffect::CheckRunning => {
                self.send_uninstall_later(|| UEvent::RunningChecked(system::launcher_running()))
            }
            UEffect::RunUninstaller => {
                self.send_uninstall_later(move || UEvent::UninstallerExited(remove_app(&dir)))
            }
            UEffect::DeleteData => {
                let targets =
                    data_targets(&self.folders.roaming_appdata, &self.folders.local_appdata);
                let sign_ins = text::uninstall_words(self.ui.lang).sign_ins;
                self.send_uninstall_later(move || {
                    let mut not_deleted = delete_paths(&targets);
                    match system::delete_credentials(&credential_filter(CREDENTIAL_SERVICE)) {
                        Ok(n) => log::line(&format!("deleted {n} saved sign-ins")),
                        Err(e) => {
                            log::line(&format!("saved sign-ins: {e}"));
                            not_deleted.push(sign_ins.to_string());
                        }
                    }
                    log::line(&format!("not deleted: {not_deleted:?}"));
                    UEvent::DataDeleted(not_deleted)
                });
            }
            UEffect::CopyDetails => {
                let detail = match &u.machine.screen {
                    Screen::Uninstall(UScreen::Failed { code, not_deleted }) => {
                        format!("Code: {code}\nNot deleted: {}", not_deleted.join(", "))
                    }
                    _ => String::new(),
                };
                let details = format!(
                    "Perland Launcher Uninstall {VERSION}\n{detail}\nLog: {}",
                    log::path().display()
                );
                self.copy(&details);
            }
            UEffect::Exit => {
                if let Some(temp) = &u.own_temp {
                    let (program, args) = cleanup_command(temp);
                    if !system::spawn_hidden(&program, &args) {
                        log::line("could not start the temp folder cleanup");
                    }
                }
                event_loop.exit();
            }
        }
    }

    fn copy(&self, text: &str) {
        // The clipboard needs an owner window, or SetClipboardData fails.
        let owner =
            self.graphics
                .as_ref()
                .and_then(|g| match g.window.window_handle().ok()?.as_raw() {
                    RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
                    _ => None,
                });
        if !owner.is_some_and(|hwnd| system::copy_to_clipboard(hwnd, text)) {
            log::line("could not copy the details to the clipboard");
        }
    }

    fn send_later(&self, work: impl FnOnce() -> Event + Send + 'static) {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let event = work();
            let _ = proxy.send_event(UserEvent::Model(event));
        });
    }

    fn run(&mut self, event_loop: &ActiveEventLoop, effect: Effect) {
        match effect {
            Effect::CheckRunning => {
                self.send_later(|| Event::RunningChecked(system::launcher_running()))
            }
            Effect::WritePayload => {
                let slot = self.payload.clone();
                self.send_later(move || {
                    let Some(bytes) = embedded() else {
                        return Event::PayloadFailed(ErrorCode::PayloadMissing);
                    };
                    match PayloadDir::write(bytes, &std::env::temp_dir()) {
                        Ok(payload) => {
                            log::line(&format!("payload at {}", payload.exe().display()));
                            *slot.lock().expect("payload lock") = Some(payload);
                            Event::PayloadWritten
                        }
                        Err(e) => {
                            log::line(&format!("payload write failed: {e}"));
                            Event::PayloadFailed(ErrorCode::TempUnwritable)
                        }
                    }
                });
            }
            Effect::RunInstaller => {
                self.pacer = Pacer::default();
                let slot = self.payload.clone();
                let args = match &self.flow {
                    Flow::Setup(m) => installer_args(&m.choice, m.fresh()),
                    Flow::Uninstall(_) => return,
                };
                self.send_later(move || {
                    let guard = slot.lock().expect("payload lock");
                    let code = guard
                        .as_ref()
                        .map(|p| run_installer(p, &args))
                        .unwrap_or(-1);
                    Event::InstallerExited(code)
                });
            }
            Effect::ConfirmInstall => self.send_later(|| {
                let installed = system::installed();
                log::line(&format!("after install: {installed:?}"));
                Event::Confirmed(
                    matches!(installed, Some((v, exe)) if v.trim() == VERSION && exe.exists()),
                )
            }),
            Effect::Launch => self.send_later(|| {
                let launched = system::installed().is_some_and(|(_, exe)| system::launch(&exe));
                Event::Launched(launched)
            }),
            Effect::DeletePayload => {
                // The installer has exited, or the writer has reported, so nothing else holds it.
                if let Some(payload) = self.payload.lock().expect("payload lock").take() {
                    remove_payload(payload);
                }
            }
            Effect::CopyDetails => {
                let code = self.error.map(|e| e.code()).unwrap_or("unknown");
                let details = format!(
                    "Perland Launcher Setup {VERSION}\nError: {code}{}\nLog: {}",
                    match self.error {
                        Some(ErrorCode::InstallerFailed(n)) => format!(" ({n})"),
                        _ => String::new(),
                    },
                    log::path().display()
                );
                self.copy(&details);
            }
            Effect::PickFolder => {
                let mut dialog = rfd::FileDialog::new()
                    .set_title(text::options_words(self.ui.lang).install_to)
                    .set_directory(self.ui.paths.target().parent().unwrap_or(Path::new("C:\\")));
                if let Some(g) = self.graphics.as_ref() {
                    dialog = dialog.set_parent(&*g.window);
                }
                // Modal: Windows runs the dialog's own message loop on this thread.
                let picked = dialog.pick_folder();
                log::line(&format!("folder picked: {picked:?}"));
                let _ = self
                    .proxy
                    .send_event(UserEvent::Model(Event::FolderPicked(picked)));
            }
            Effect::ResolveFolder(picked) => self.send_later(move || {
                let resolved = resolve_folder(&picked, &system::protected_roots());
                log::line(&format!("folder {}: {resolved:?}", picked.display()));
                Event::FolderResolved(resolved)
            }),
            Effect::RemoveShortcuts => {
                if let Flow::Setup(m) = &self.flow {
                    let start_menu = self.folders.programs.join("Perland Launcher.lnk");
                    let desktop = self.folders.desktop.join("Perland Launcher.lnk");
                    for lnk in shortcuts_to_remove(&m.choice, m.fresh(), &start_menu, &desktop) {
                        match std::fs::remove_file(&lnk) {
                            Ok(()) => log::line(&format!("removed {}", lnk.display())),
                            Err(e) => {
                                log::line(&format!("could not remove {}: {e}", lnk.display()))
                            }
                        }
                    }
                }
            }
            Effect::Exit => {
                if matches!(self.screen(), Screen::Opening) {
                    self.closing = Some(Instant::now() + OPENING_HOLD);
                } else {
                    if let Some(payload) = self.payload.lock().expect("payload lock").take() {
                        remove_payload(payload);
                    }
                    event_loop.exit();
                }
            }
        }
    }

    fn press(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        match self.flow {
            Flow::Setup(_) => self.handle(event_loop, Event::Pressed(action)),
            Flow::Uninstall(_) => self.handle_uninstall(event_loop, UEvent::Pressed(action)),
        }
    }

    fn move_focus(&mut self) {
        let actions: Vec<Action> = draw::buttons(self.screen(), &self.ui)
            .iter()
            .map(|b| b.action)
            .collect();
        if actions.is_empty() {
            self.ui.focus = None;
            return;
        }
        let at = self
            .ui
            .focus
            .and_then(|f| actions.iter().position(|a| *a == f));
        let next = match (at, self.shift) {
            (None, false) => 0,
            (None, true) => actions.len() - 1,
            (Some(i), false) => (i + 1) % actions.len(),
            (Some(i), true) => (i + actions.len() - 1) % actions.len(),
        };
        self.ui.focus = Some(actions[next]);
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let mut attrs = Window::default_attributes()
            .with_title(self.title)
            .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            // Without a frame Windows draws no shadow, and the window would sit flat on the desktop.
            .with_undecorated_shadow(true)
            .with_window_icon(window_icon());
        if let Some(monitor) = event_loop.primary_monitor() {
            let scale = monitor.scale_factor();
            let (w, h) = (WIDTH as f64 * scale, HEIGHT as f64 * scale);
            let (mx, my) = (monitor.position().x as f64, monitor.position().y as f64);
            let (mw, mh) = (monitor.size().width as f64, monitor.size().height as f64);
            attrs = attrs.with_position(PhysicalPosition::new(
                mx + (mw - w) / 2.0,
                my + (mh - h) / 2.0,
            ));
        }
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Rc::new(window),
            Err(e) => {
                log::line(&format!("no window: {e}"));
                event_loop.exit();
                return;
            }
        };
        let access = accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.proxy.clone(),
        );
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|context| softbuffer::Surface::new(&context, window.clone()));
        let surface = match surface {
            Ok(surface) => surface,
            Err(e) => {
                log::line(&format!("no drawing surface: {e}"));
                event_loop.exit();
                return;
            }
        };
        window.set_visible(true);
        self.graphics = Some(Graphics {
            window,
            surface,
            access,
        });
        if matches!(self.flow, Flow::Uninstall(_)) {
            let data = self.ui.paths.data.clone();
            self.send_uninstall_later(move || UEvent::DataSize(dir_size(&data)));
        }
        self.redraw();
    }
}

/// The exe's own icon (`src-tauri/icons/icon.ico`, resource 1, embedded by build.rs).
fn window_icon() -> Option<Icon> {
    Icon::from_resource(1, None).ok()
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_none() {
            self.create_window(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(g) = self.graphics.as_mut() {
            g.access.process_event(&g.window, &event);
        }
        let scale = self.scale() as f32;
        match event {
            WindowEvent::CloseRequested => self.press(event_loop, Action::Close),
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.redraw(),
            WindowEvent::ModifiersChanged(m) => self.shift = m.state().shift_key(),
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32 / scale, position.y as f32 / scale);
                self.cursor_y = y;
                let hover = draw::hit(self.screen(), &self.ui, x, y);
                if hover != self.ui.hover {
                    self.ui.hover = hover;
                    if let Some(g) = self.graphics.as_ref() {
                        g.window.set_cursor(if hover.is_some() {
                            CursorIcon::Pointer
                        } else {
                            CursorIcon::Default
                        });
                    }
                    self.redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.ui.hover = None;
                self.redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    self.pressed = self.ui.hover;
                    self.ui.focus = None;
                    if self.pressed.is_none() && draw::drag_region(self.cursor_y) {
                        if let Some(g) = self.graphics.as_ref() {
                            let _ = g.window.drag_window();
                        }
                    }
                    self.redraw();
                }
                ElementState::Released => {
                    if let Some(action) = self.pressed.take().filter(|a| Some(*a) == self.ui.hover)
                    {
                        self.press(event_loop, action);
                    }
                }
            },
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Enter) => {
                        let action = self.ui.focus.unwrap_or(Action::Primary);
                        self.press(event_loop, action);
                    }
                    Key::Named(NamedKey::Space) => {
                        if let Some(action) = self.ui.focus {
                            self.press(event_loop, action);
                        }
                    }
                    Key::Named(NamedKey::Escape) => self.press(event_loop, Action::Close),
                    Key::Named(NamedKey::Tab) => {
                        self.move_focus();
                        self.redraw();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Model(event) => self.handle(event_loop, event),
            UserEvent::Uninstall(event) => self.handle_uninstall(event_loop, event),
            UserEvent::Access(e) => match e.window_event {
                accesskit_winit::WindowEvent::InitialTreeRequested => self.redraw(),
                accesskit_winit::WindowEvent::ActionRequested(request) => {
                    let Some(action) = a11y::action_of(request.target_node) else {
                        return;
                    };
                    match request.action {
                        accesskit::Action::Click => self.press(event_loop, action),
                        accesskit::Action::Focus => {
                            self.ui.focus = Some(action);
                            self.redraw();
                        }
                        _ => {}
                    }
                }
                accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
            },
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if let Some(fade_at) = self.closing {
            if now >= fade_at + FADE {
                event_loop.exit();
                return;
            }
            if now < fade_at {
                event_loop.set_control_flow(ControlFlow::WaitUntil(fade_at));
                return;
            }
            // A redraw request wakes the loop at once, so frames are paced here, not by the wake.
            match self.pacer.frame(now, FRAME) {
                Ok(_) => {
                    self.ui.fade = 1.0 - (now - fade_at).as_secs_f32() / FADE.as_secs_f32();
                    self.redraw();
                    event_loop.set_control_flow(ControlFlow::WaitUntil(now + FRAME));
                }
                Err(next) => event_loop.set_control_flow(ControlFlow::WaitUntil(next)),
            }
            return;
        }
        if matches!(self.screen(), Screen::Installing { .. }) {
            match self.pacer.frame(now, FRAME) {
                Ok(dt) => {
                    self.handle(event_loop, Event::Tick(dt));
                    event_loop.set_control_flow(ControlFlow::WaitUntil(now + FRAME));
                }
                Err(next) => event_loop.set_control_flow(ControlFlow::WaitUntil(next)),
            }
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

/// Runs NSIS's uninstaller in place for `dir` and waits; then removes what it leaves behind.
fn remove_app(dir: &Path) -> i32 {
    let exe = dir.join("uninstall.exe");
    let args = uninstaller_args(dir);
    log::line(&format!("running {} {args:?}", exe.display()));
    let code = match std::process::Command::new(&exe).args(&args).status() {
        Ok(status) => status.code().unwrap_or(-1),
        Err(e) => {
            log::line(&format!("uninstaller did not start: {e}"));
            -1
        }
    };
    log::line(&format!("uninstaller exited {code}"));
    if code == 0 {
        finish_removal(dir);
    }
    code
}

/// After NSIS ran in place (`_?=`): its own exe, our uninstaller and the folder, only if empty.
fn finish_removal(dir: &Path) {
    for name in ["uninstall.exe", "perland-uninstall.exe"] {
        let file = dir.join(name);
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::line(&format!("could not remove {}: {e}", file.display())),
        }
    }
    if let Err(e) = std::fs::remove_dir(dir) {
        log::line(&format!("left {} ({e})", dir.display()));
    }
}

/// The branded uninstaller (`perland-uninstall.exe`): see `uninstall::mode` for how it starts.
pub fn run_uninstall() -> i32 {
    log::set_name("perland-uninstall.log");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let mode = mode(&args, &exe_dir);
    log::line(&format!("Perland Launcher Uninstall {VERSION}: {mode:?}"));
    match mode {
        Mode::Handoff { dir, args } => {
            // Tauri's own wizard asked: run its uninstaller exactly as it would have.
            let exe = dir.join("uninstall.exe");
            match std::process::Command::new(&exe).args(&args).status() {
                Ok(status) => status.code().unwrap_or(-1),
                Err(e) => {
                    log::line(&format!("handoff: {e}"));
                    -1
                }
            }
        }
        Mode::Silent { dir } => {
            if system::launcher_running() {
                return 2;
            }
            remove_app(&dir)
        }
        Mode::Relaunch { dir } => match std::fs::read(&exe).and_then(|bytes| {
            PayloadDir::write_named(&bytes, &std::env::temp_dir(), "perland-uninstall.exe")
        }) {
            Ok(copy) => {
                // The copy must outlive this process: keep the folder; the copy removes it.
                let (copy_exe, copy_dir) = (copy.exe().to_path_buf(), copy.dir().to_path_buf());
                std::mem::forget(copy);
                let started = std::process::Command::new(&copy_exe)
                    .arg("--from-temp")
                    .arg(&dir)
                    .arg("--own-temp")
                    .arg(&copy_dir)
                    .spawn()
                    .is_ok();
                if started {
                    0
                } else {
                    log::line("could not start the temp copy; running in place");
                    uninstall_window(dir, None)
                }
            }
            Err(e) => {
                log::line(&format!("could not copy to temp ({e}); running in place"));
                uninstall_window(dir, None)
            }
        },
        Mode::Window { dir } => {
            let own_temp = args
                .iter()
                .position(|a| a == "--own-temp")
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from);
            uninstall_window(dir, own_temp)
        }
    }
}

fn uninstall_window(dir: PathBuf, own_temp: Option<PathBuf>) -> i32 {
    let lang = system::ui_lang();
    let title = text::uninstall_words(lang).window_title;
    let Some(_instance) = system::SingleInstance::claim_named("Local\\PerlandLauncherUninstall")
    else {
        system::focus_window(title);
        return 0;
    };
    let folders = system::folders();
    let paths = Paths {
        install: dir.clone(),
        data: folders.roaming_appdata.join("PerlandLauncher"),
        installed_bytes: None,
        choice: Choice::default(),
        data_bytes: None,
        delete_data: false,
    };
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            log::line(&format!("no event loop: {e}"));
            return 1;
        }
    };
    let proxy = event_loop.create_proxy();
    let flow = Flow::Uninstall(Uninstalling {
        machine: UMachine::new(),
        dir,
        own_temp,
    });
    let mut app = App::new(flow, proxy, paths, folders, title);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::line(&format!("event loop ended with {e}"));
        return 1;
    }
    0
}
