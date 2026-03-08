use muda::{Menu, PredefinedMenuItem, Submenu};
use std::sync::mpsc;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{PageLoadEvent, WebViewBuilder};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let url = args.get(1).expect("missing url").clone();
    let target = args.get(2).expect("missing target").clone();

    #[cfg(target_os = "macos")]
    {
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
        .unwrap();
        menu.append(&edit).unwrap();
        menu.init_for_nsapp();
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Authenticate")
        .with_inner_size(LogicalSize::new(500.0, 700.0))
        .build(&event_loop)
        .unwrap();

    let (tx, rx) = mpsc::channel::<()>();
    let target_clone = target.clone();

    let webview = WebViewBuilder::new()
        .with_url(&url)
        .with_ipc_handler(|msg| {
            println!("{}", msg.body());
            std::process::exit(0);
        })
        .with_on_page_load_handler(move |event, nav_url| {
            if let PageLoadEvent::Finished = event {
                if nav_url.starts_with(&target_clone) {
                    tx.send(()).ok();
                }
            }
        })
        .build(&window)
        .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if rx.try_recv().is_ok() {
            webview
                .evaluate_script(
                    r#"
                (function() {
                    window.ipc.postMessage(JSON.stringify({
                        url: window.location.href,
                        body: document.body.innerText
                    }));
                })();
            "#,
                )
                .ok();
        }

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            std::process::exit(1);
        }
    });
}
