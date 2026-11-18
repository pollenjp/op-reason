//! The Windows notification, through WinRT. It borrows the AppUserModelID that Windows PowerShell registers (the A'
//! launcher does the same), so nothing is registered and the notification reads "Windows PowerShell".
//!
//! A window's notification ([`start_for_window`]) has a tag of its own and the group [`GROUP`], and goes when its window
//! closes ([`Pending::hide`]: from the screen and from the notification center); the one-time notices ([`start`]) stay
//! (design `2026-11-18-pass-toasts-design.md` §6).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier};
use windows::core::HSTRING;

/// Windows PowerShell's AppUserModelID.
pub const AUMID: &str =
    r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";
/// The group of the windows' notifications.
pub const GROUP: &str = "op-reason";

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// A notification on the screen that can be removed: what removing it needs.
struct Shown {
    notifier: ToastNotifier,
    toast: ToastNotification,
    tag: HSTRING,
}

impl Shown {
    /// Removes it from the screen and from the notification center (errors ignored: it may be gone already).
    fn remove(&self) {
        let _ = self.notifier.Hide(&self.toast);
        if let Ok(history) = ToastNotificationManager::History() {
            let _ = history.RemoveGroupedTagWithId(
                &self.tag,
                &HSTRING::from(GROUP),
                &HSTRING::from(AUMID),
            );
        }
    }
}

/// Shows the notification; with a tag, also in [`GROUP`], and returns what removing it needs.
fn show(lines: &[String], tag: Option<&str>) -> windows::core::Result<Option<Shown>> {
    let mut xml = String::from(r#"<toast><visual><binding template="ToastGeneric">"#);
    for l in lines {
        xml.push_str("<text>");
        xml.push_str(&escape(l));
        xml.push_str("</text>");
    }
    xml.push_str("</binding></visual></toast>");
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    if let Some(tag) = tag {
        toast.SetTag(&HSTRING::from(tag))?;
        toast.SetGroup(&HSTRING::from(GROUP))?;
    }
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?;
    notifier.Show(&toast)?;
    Ok(tag.map(|tag| Shown {
        notifier,
        toast,
        tag: HSTRING::from(tag),
    }))
}

/// A notification being shown on a worker thread, so that op.exe does not wait for the notification platform.
pub struct Pending {
    result: mpsc::Receiver<String>,
    /// Tells a window's worker to remove its notification (none for a one-time notice).
    hide: Option<mpsc::Sender<()>>,
    /// Told when the worker has removed it.
    removed: mpsc::Receiver<()>,
    /// [`Pending::hide`] was called.
    hidden: bool,
}

/// Starts showing a notice that stays: a one-time notice of the relay, the shared approval of a WSL call.
pub fn start(lines: Vec<String>) -> Pending {
    spawn(lines, None)
}

/// Starts showing the notification of a window, to be removed when the window closes ([`Pending::hide`]).
pub fn start_for_window(lines: Vec<String>) -> Pending {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let tag = format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    );
    spawn(lines, Some(tag))
}

/// The worker shows the notification and sends the result; a window's worker then waits: told to hide, it removes the
/// notification (also when the window closed before it was shown); its `Pending` dropped instead, it keeps it.
fn spawn(lines: Vec<String>, tag: Option<String>) -> Pending {
    let (tx, rx) = mpsc::channel();
    let (hide_tx, hide_rx) = mpsc::channel::<()>();
    let (removed_tx, removed_rx) = mpsc::channel::<()>();
    let tagged = tag.is_some();
    thread::spawn(move || match show(&lines, tag.as_deref()) {
        Ok(shown) => {
            let _ = tx.send("shown".to_owned());
            if let Some(shown) = shown
                && hide_rx.recv().is_ok()
            {
                shown.remove();
                let _ = removed_tx.send(());
            }
        }
        Err(e) => {
            let _ = tx.send(format!(
                "failed: {}",
                super::opexe::short(&e.message(), 120)
            ));
        }
    });
    Pending {
        result: rx,
        hide: tagged.then_some(hide_tx),
        removed: removed_rx,
        hidden: false,
    }
}

impl Pending {
    /// The `toast` field of the record (waits up to three seconds). After [`Pending::hide`] it also waits (up to two
    /// seconds) until the notification has been removed, so that a process that exits next does not leave it behind.
    pub fn result(self) -> String {
        let r = self
            .result
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|_| "failed: no answer within 3 seconds".to_owned());
        if self.hidden {
            let _ = self.removed.recv_timeout(Duration::from_secs(2));
        }
        r
    }

    /// Removes a window's notification, once it has been shown (nothing for a one-time notice, or the second time).
    pub fn hide(&mut self) {
        if let Some(tx) = self.hide.take() {
            self.hidden = tx.send(()).is_ok();
        }
    }
}
