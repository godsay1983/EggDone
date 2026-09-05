use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct CaptureInput {
    pub target: String,
    pub title: String,
    pub body: String,
    pub source_url: String,
    pub source_app: String,
}

#[derive(Clone, Serialize)]
pub struct CaptureRequest {
    pub id: u64,
    pub input: CaptureInput,
}

#[derive(Default)]
pub struct CaptureInbox {
    queue: Mutex<VecDeque<CaptureRequest>>,
    sequence: AtomicU64,
    rejected: AtomicBool,
}

// This is an explicit CLI, not a URL handler. Unknown or repeated fields fail closed.
pub fn parse_args(args: &[String]) -> Result<Option<CaptureInput>, String> {
    if args.get(1).map(String::as_str) != Some("--capture") {
        return Ok(None);
    }
    if args.len() > 12 || args.iter().map(String::len).sum::<usize>() > 400_000 {
        return Err("CAPTURE_TOO_LARGE".into());
    }
    let mut input = CaptureInput {
        target: "note".into(),
        ..Default::default()
    };
    let mut seen = std::collections::HashSet::new();
    let mut fields = args[2..].chunks_exact(2);
    for pair in &mut fields {
        if !seen.insert(pair[0].as_str()) {
            return Err("CAPTURE_INVALID".into());
        }
        match pair[0].as_str() {
            "--target" => input.target = pair[1].clone(),
            "--title" => input.title = pair[1].clone(),
            "--text" => input.body = pair[1].clone(),
            "--url" => input.source_url = pair[1].clone(),
            _ => return Err("CAPTURE_INVALID".into()),
        }
    }
    if !fields.remainder().is_empty() || !matches!(input.target.as_str(), "todo" | "note") {
        return Err("CAPTURE_INVALID".into());
    }
    if !input.source_url.is_empty() {
        let url = url::Url::parse(&input.source_url).map_err(|_| "CAPTURE_INVALID")?;
        if !matches!(url.scheme(), "https" | "http")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("CAPTURE_INVALID".into());
        }
    }
    Ok(Some(input))
}

pub fn enqueue(app: &tauri::AppHandle, input: CaptureInput) -> Result<(), String> {
    let inbox = app.state::<CaptureInbox>();
    let mut queue = inbox.queue.lock().map_err(|_| "CAPTURE_BUSY")?;
    if queue.len() >= 8 {
        drop(queue);
        reject(app);
        return Err("CAPTURE_BUSY".into());
    }
    queue.push_back(CaptureRequest {
        id: inbox.sequence.fetch_add(1, Ordering::Relaxed) + 1,
        input,
    });
    drop(queue);
    crate::tray::show_panel(app, None);
    let _ = app.emit_to("main", "capture-available", ());
    Ok(())
}

pub fn reject(app: &tauri::AppHandle) {
    app.state::<CaptureInbox>()
        .rejected
        .store(true, Ordering::Relaxed);
    crate::tray::show_panel(app, None);
    let _ = app.emit_to("main", "capture-available", ());
}

#[tauri::command]
pub fn take_capture_error(inbox: tauri::State<'_, CaptureInbox>) -> bool {
    inbox.rejected.swap(false, Ordering::Relaxed)
}

#[tauri::command]
pub fn peek_capture(inbox: tauri::State<'_, CaptureInbox>) -> Option<CaptureRequest> {
    inbox.queue.lock().ok()?.front().cloned()
}

#[tauri::command]
pub fn dismiss_capture(id: u64, inbox: tauri::State<'_, CaptureInbox>) {
    if let Ok(mut queue) = inbox.queue.lock() {
        if queue.front().is_some_and(|request| request.id == id) {
            queue.pop_front();
        }
    }
}

#[tauri::command]
pub fn quick_capture_note(app: tauri::AppHandle) -> Result<(), String> {
    enqueue(
        &app,
        CaptureInput {
            target: "note".into(),
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Option<CaptureInput>, String> {
        parse_args(
            &args
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn ordinary_launch_and_explicit_empty_draft() {
        assert!(parse(&["app", "--autostart"]).unwrap().is_none());
        assert_eq!(
            parse(&["app", "--capture"]).unwrap().unwrap().target,
            "note"
        );
    }
    #[test]
    fn accepts_text_and_link_without_interpreting_text() {
        let draft = parse(&[
            "app",
            "--capture",
            "--target",
            "todo",
            "--text",
            "hello & 中文",
            "--url",
            "https://example.com/a",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(draft.body, "hello & 中文");
    }
    #[test]
    fn rejects_ambiguous_or_executable_inputs() {
        for fields in [
            vec!["--target", "bad"],
            vec!["--text"],
            vec!["--file", "C:/secret"],
            vec!["--text", "a", "--text", "b"],
            vec!["--url", "javascript:alert(1)"],
            vec!["--url", "file:///C:/secret"],
            vec!["--url", "https://user:pass@example.com"],
        ] {
            let mut args = vec!["app", "--capture"];
            args.extend(fields);
            assert!(parse(&args).is_err());
        }
    }
}
