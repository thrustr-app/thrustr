#[cfg(target_os = "macos")]
use muda::{Menu, PredefinedMenuItem, Submenu};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{PageLoadEvent, WebViewBuilder};

const EXTRACT_PAGE_SCRIPT: &str = r#"
(function() {
    window.ipc.postMessage(JSON.stringify({
        url: window.location.href,
        body: document.body.innerText
    }));
})();
"#;

enum UserEvent {
    NavigationMatched,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let url = args.next().unwrap_or_else(usage_error);
    let target = args.next().unwrap_or_else(usage_error);

    #[cfg(target_os = "macos")]
    init_macos_menu();

    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Authenticate")
        .with_inner_size(LogicalSize::new(500.0, 700.0))
        .build(&event_loop)
        .expect("failed to create window");

    let webview = WebViewBuilder::new()
        .with_url(&url)
        .with_ipc_handler(|msg| {
            println!("{}", msg.body());
            std::process::exit(0);
        })
        .with_on_page_load_handler(move |event, nav_url| {
            if let PageLoadEvent::Finished = event
                && nav_url.starts_with(&target)
            {
                proxy.send_event(UserEvent::NavigationMatched).ok();
            }
        })
        .build(&window)
        .expect("failed to create webview");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(UserEvent::NavigationMatched) => {
                if let Err(err) = webview.evaluate_script(EXTRACT_PAGE_SCRIPT) {
                    eprintln!("failed to evaluate extraction script: {err}");
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => std::process::exit(1),
            _ => {}
        }
    });
}

fn usage_error() -> String {
    eprintln!("usage: webview-helper <url> <target-url-prefix>");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn init_macos_menu() {
    let menu = Menu::new();
    let edit = Submenu::new("Edit", true);
    edit.append_items(&[
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
    ])
    .expect("failed to build Edit menu");
    menu.append(&edit).expect("failed to append Edit menu");
    menu.init_for_nsapp();
}
