use crate::GuiResponses;
use crate::GuiActions;
use crossbeam_channel::Sender;
use crossbeam_channel::Receiver;
use web_view::*;

use winapi::shared::winerror;
use winapi::um::knownfolders;
use winapi::um::combaseapi;
use winapi::um::shlobj;
use winapi::um::shtypes;
use winapi::um::winbase;
use winapi::um::winnt;

use std::path::PathBuf;

const HTML_HEAD: &str = include_str!("ui/head.html");
const HTML_CSS: &str = include_str!("ui/style.css");
const HTML_JS_DEPS: &str = include_str!("ui/cash.min.js");
const HTML_JS_APP: &str = include_str!("ui/app.js");
const HTML_REST: &str = include_str!("ui/rest.html");

/*
fn escape_html_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c)
        };
    }
}
*/

pub fn spawn_gui(background_tx: Sender<GuiActions>, gui_rx: Receiver<GuiResponses>) {
    set_dpi_aware();

    let mut html = String::new();
    html.push_str(HTML_HEAD);
    html.push_str("<style>\n");
    html.push_str(HTML_CSS);
    html.push_str("\n</style><script>\n");
    html.push_str(HTML_JS_DEPS);
    html.push_str(HTML_JS_APP);
    html.push_str("\n</script>\n");
    html.push_str(HTML_REST);

    std::fs::write("test.html", &html).unwrap();

    let webview = web_view::builder()
        .title("Compactor")
        .content(Content::Html(html))
        .size(1000, 900)
        .resizable(true)
        .debug(true)
        .user_data(())
        .invoke_handler(|mut webview, arg| {
            match arg {
                "choose" => {
                    match select_dir(&mut webview)? {
                        Some(path) => webview.dialog().info("Dir", path.to_string_lossy())?,
                        None => webview
                            .dialog()
                            .warning("Warning", "You didn't choose a file.")?,
                    };
                },
                _ if arg.starts_with("http") => {
                    open_url(arg);
                },
                _ => { println!("Invoke: {}", arg); }
            }
            Ok(())
        })
        .build().expect("WebView");

    let handle = webview.handle();

    let gui_thread = std::thread::spawn(move || {
        for event in gui_rx {
            match event {
                GuiResponses::FolderStatus(fi) => { handle.dispatch(|wv| wv.eval("App.folder_status('test');")).expect("dispatch"); },
                GuiResponses::Output(String) => { handle.dispatch(|wv| wv.eval("App.output('test');")).expect("dispatch"); },
                GuiResponses::Exit => {
                    handle.dispatch(|wv| { wv.terminate(); Ok(()) }).expect("dispatch");
                    break;
                },
            }
        }
    });

    let _ = webview.run();
    gui_thread.join();
}

fn open_url<U: AsRef<str>>(url: U) {
    let _ = open::that(url.as_ref());
}

fn set_dpi_aware() {
    use winapi::um::shellscalingapi::{PROCESS_SYSTEM_DPI_AWARE, SetProcessDpiAwareness};

    unsafe { SetProcessDpiAwareness(PROCESS_SYSTEM_DPI_AWARE) };
}

fn program_files() -> PathBuf {
    known_folder(&knownfolders::FOLDERID_ProgramFiles).expect("Program files path")
}

// stolen from directories crate
// Copyright (c) 2018 directories-rs contributors
// (MIT license)
fn known_folder(folder_id: shtypes::REFKNOWNFOLDERID) -> Option<PathBuf> {
    unsafe {
        let mut path_ptr: winnt::PWSTR = std::ptr::null_mut();
        let result = shlobj::SHGetKnownFolderPath(folder_id, 0, std::ptr::null_mut(), &mut path_ptr);
        if result == winerror::S_OK {
            let len = winbase::lstrlenW(path_ptr) as usize;
            let path = std::slice::from_raw_parts(path_ptr, len);
            let ostr: std::ffi::OsString = std::os::windows::ffi::OsStringExt::from_wide(path);
            combaseapi::CoTaskMemFree(path_ptr as *mut winapi::ctypes::c_void);
            Some(PathBuf::from(ostr))
        } else {
            None
        }
    }
}

use lazy_static::lazy_static;
use std::sync::Mutex;

lazy_static! {
    static ref LAST_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
}

// WebView has an irritatingly stupid bug here, failing to initialize variables
// causing the dialog to fail to open and this to return None depending on the
// random junk on the stack. (#214)
//
// The message loop seems to be broken on Windows too (#220, #221).
//
// Sadly nobody seems interested in merging these.  For now, use a locally modified
// copy.
fn select_dir<T>(webview: &mut web_view::WebView<'_, T>) -> WVResult<Option<PathBuf>> {
    let mut last = LAST_FILE.lock().unwrap();
    if let Some(path) = webview.dialog().choose_directory(
        "Select Directory",
        last.clone().unwrap_or_else(program_files),
    )? {
        last.replace(path.clone());

        Ok(Some(path))
    } else {
        Ok(None)
    }
}
