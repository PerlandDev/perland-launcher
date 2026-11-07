//! The real window behind `launcher_app::Host`.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use launcher_app::{AppEvent, Host, SignInWindow};
use tauri::{
    webview::NewWindowResponse, AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

pub const APP_EVENT: &str = "launcher://event";

/// Each sign-in window's label is this and a number, so closing one never closes a newer one.
const SIGN_IN_WINDOW: &str = "microsoft-sign-in-";

pub fn is_sign_in_window(label: &str) -> bool {
    label.starts_with(SIGN_IN_WINDOW)
}

pub struct TauriHost {
    app: AppHandle,
    /// The number of the next sign-in window.
    next_sign_in: AtomicU64,
    /// The label of the sign-in window opened last.
    sign_in: Mutex<Option<String>>,
}

impl TauriHost {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            next_sign_in: AtomicU64::new(1),
            sign_in: Mutex::new(None),
        }
    }

    fn close_window(app: &AppHandle, label: &str) {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
        }
    }

    /// Close a window from inside one of its webview's own callbacks: later, off that callback,
    /// so the webview isn't torn down while it is still reporting.
    fn close_window_later(app: &AppHandle, label: &str) {
        let (app, label) = (app.clone(), label.to_string());
        tauri::async_runtime::spawn(async move { Self::close_window(&app, &label) });
    }
}

/// Where a sign-in window's answer goes: the redirect it reached, or `None` once it closed.
/// Only the first answer counts.
type Answer = Arc<Mutex<Option<oneshot::Sender<Option<String>>>>>;

fn send(answer: &Answer, landed: Option<String>) {
    let sender = answer.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(sender) = sender {
        let _ = sender.send(landed);
    }
}

/// Closes its sign-in window when the app stops waiting for it.
struct CloseOnDrop {
    app: AppHandle,
    label: String,
}

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        TauriHost::close_window(&self.app, &self.label);
    }
}

impl Host for TauriHost {
    fn emit(&self, event: AppEvent) {
        let _ = self.app.emit(APP_EVENT, event);
    }

    fn minimize(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.minimize();
        }
    }

    fn restore(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    /// The app only passes http(s) URLs to a Perland Engine it built itself; anything else is
    /// refused here too. The error isn't logged: it may quote the URL.
    fn open_url(&self, url: &str) {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            tracing::warn!("refused to open a URL that isn't http(s)");
            return;
        }
        if self.app.opener().open_url(url, None::<&str>).is_err() {
            tracing::warn!("couldn't open the browser");
        }
    }

    /// Microsoft's page in a window of the launcher's own, in front of the main one. It keeps
    /// no cookies (incognito), so every sign-in can pick an account. The redirect that ends the
    /// sign-in is caught as it starts and never loaded; the page load check is a second net for
    /// a webview that doesn't report a server redirect as a navigation. Pages it would open in a
    /// new window aren't opened. No URL is logged: the redirect carries the code.
    fn open_sign_in_window(
        &self,
        window: SignInWindow,
    ) -> launcher_app::runner::BoxFuture<Option<String>> {
        let (sender, answered) = oneshot::channel();
        let answer: Answer = Arc::new(Mutex::new(Some(sender)));
        let label = format!(
            "{SIGN_IN_WINDOW}{}",
            self.next_sign_in.fetch_add(1, Ordering::SeqCst)
        );
        let before = self
            .sign_in
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(label.clone());
        if let Some(before) = before {
            Self::close_window(&self.app, &before);
        }
        let close = CloseOnDrop {
            app: self.app.clone(),
            label: label.clone(),
        };
        let url = match Url::parse(window.url) {
            Ok(url) if url.scheme() == "https" => url,
            _ => {
                tracing::warn!("refused to open a sign-in page that isn't https");
                return Box::pin(async { None });
            }
        };

        let done = window.done;
        let (on_navigation, on_load) = (answer.clone(), answer.clone());
        let (app, own) = (self.app.clone(), label.clone());
        let (load_app, load_own) = (self.app.clone(), label.clone());
        let mut builder = WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(url))
            .title(window.title)
            .inner_size(500.0, 680.0)
            .min_inner_size(380.0, 480.0)
            .center()
            .incognito(true)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_navigation(move |url| {
                if done(url.as_str()) {
                    send(&on_navigation, Some(url.to_string()));
                    TauriHost::close_window_later(&app, &own);
                    return false;
                }
                matches!(url.scheme(), "https" | "about" | "data" | "blob")
            })
            .on_page_load(move |_, payload| {
                if done(payload.url().as_str()) {
                    send(&on_load, Some(payload.url().to_string()));
                    TauriHost::close_window_later(&load_app, &load_own);
                }
            });
        if let Some(main) = self.app.get_webview_window("main") {
            builder = match builder.parent(&main) {
                Ok(child) => child,
                Err(_) => {
                    tracing::warn!("couldn't put the sign-in window in front of the launcher");
                    return Box::pin(async { None });
                }
            };
        }
        match builder.build() {
            Ok(opened) => {
                let closed = answer.clone();
                opened.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Destroyed) {
                        send(&closed, None);
                    }
                });
            }
            Err(e) => {
                tracing::warn!("couldn't open the Microsoft sign-in window: {e}");
                send(&answer, None);
            }
        }
        Box::pin(async move {
            let _close = close;
            answered.await.ok().flatten()
        })
    }
}

/// Total RAM in MB, for the memory slider's soft warning.
pub fn system_memory_mb() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory() / (1024 * 1024)
}

/// "Windows 11 Pro 26200" or "Linux 24.04 Ubuntu", for diagnostics.
pub fn os_description() -> String {
    sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string())
}
