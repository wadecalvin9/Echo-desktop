use std::path::PathBuf;
use std::time::Duration;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{PageLoadEvent, WebContext, WebViewBuilder};

#[derive(Debug, Clone, Copy)]
enum AuthEvent {
    CheckCookies,
}

fn get_webview_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Echo")
        .join("auth_webview")
}

fn main() {
    let data_dir = get_webview_data_dir();
    let _ = std::fs::create_dir_all(&data_dir);

    let event_loop = EventLoopBuilder::<AuthEvent>::with_user_event().build();

    let window = match WindowBuilder::new()
        .with_title("Sign in - Google / YouTube Music")
        .with_inner_size(LogicalSize::new(560.0, 720.0))
        .with_resizable(true)
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create window: {e}");
            std::process::exit(1);
        }
    };

    let proxy = event_loop.create_proxy();
    let proxy_load = proxy.clone();

    // Chrome 131 Desktop User Agent so Google Sign-In won't block the webview
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    let login_url = "https://accounts.google.com/ServiceLogin?service=youtube&hl=en&continue=https%3A%2F%2Fmusic.youtube.com%2F";

    let mut web_context = WebContext::new(Some(data_dir));

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_user_agent(user_agent)
        .with_url(login_url)
        .with_on_page_load_handler(move |event, _url| {
            if let PageLoadEvent::Finished = event {
                let _ = proxy_load.send_event(AuthEvent::CheckCookies);
            }
        });

    let webview = match builder.build(&window) {
        Ok(wv) => wv,
        Err(e) => {
            eprintln!("Failed to build WebView2: {e}");
            std::process::exit(1);
        }
    };

    // Periodic check every 1s in case cookie updates without triggering PageLoadEvent
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            if proxy.send_event(AuthEvent::CheckCookies).is_err() {
                break;
            }
        }
    });

    let mut captured_cookie: Option<String> = None;
    let check_cookie = move || -> Option<String> {
        let mut all_cookies = Vec::new();
        if let Ok(c) = webview.cookies_for_url("https://music.youtube.com") {
            all_cookies.extend(c);
        }
        if let Ok(c) = webview.cookies_for_url("https://youtube.com") {
            all_cookies.extend(c);
        }
        if let Ok(c) = webview.cookies() {
            all_cookies.extend(c);
        }

        // De-duplicate cookies by name
        let mut map = std::collections::HashMap::new();
        for cookie in all_cookies {
            let name = cookie.name().to_string();
            let value = cookie.value().to_string();
            map.insert(name, value);
        }

        // Crucial YouTube auth cookie
        if map.contains_key("SAPISID") || map.contains_key("__Secure-3PAPISID") {
            let formatted = map
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Some(formatted);
        }
        None
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(AuthEvent::CheckCookies) => {
                if let Some(cookie) = check_cookie() {
                    captured_cookie = Some(cookie);
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => (),
        }

        if *control_flow == ControlFlow::Exit {
            if let Some(c) = &captured_cookie {
                println!("{c}");
                std::process::exit(0);
            } else {
                std::process::exit(1);
            }
        }
    });
}
