//! The toast, through WinRT. It borrows the AppUserModelID that Windows PowerShell registers (the A' launcher does
//! the same), so nothing is registered and the toast reads "Windows PowerShell".

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::core::HSTRING;

const AUMID: &str = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

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

fn show(lines: &[String]) -> windows::core::Result<()> {
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
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)
}

/// A toast being shown on a worker thread, so that op.exe does not wait for the notification platform.
pub struct Pending(mpsc::Receiver<String>);

/// Starts showing the toast.
pub fn start(lines: Vec<String>) -> Pending {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let r = match show(&lines) {
            Ok(()) => "shown".to_owned(),
            Err(e) => format!("failed: {}", super::opexe::short(&e.message(), 120)),
        };
        let _ = tx.send(r);
    });
    Pending(rx)
}

impl Pending {
    /// The `toast` field of the record (waits up to three seconds).
    pub fn result(self) -> String {
        self.0
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|_| "failed: no answer within 3 seconds".to_owned())
    }
}
