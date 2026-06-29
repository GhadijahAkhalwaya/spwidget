use std::sync::Arc;

use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

mod cache;
mod commands;
mod config;
mod error;
mod jira;
mod scheduler;
mod secrets;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // Quit the whole app when the user clicks the × button.
        // Without this, ActivationPolicy::Accessory keeps the process alive
        // invisibly after the only window is closed. We use std::process::exit
        // to forcefully terminate (app_handle().exit() can race with the
        // close flow and let the process linger).
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                std::process::exit(0);
            }
        })
        .setup(|app| {
            // Regular policy so the app shows a Dock icon — the user can
            // click it to wake the widget via RunEvent::Reopen below.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            // macOS: by default WKWebView draws its own opaque backing
            // rectangle. With our transparent NSWindow + CSS-rounded #app,
            // that rectangle bleeds into the four corner "ears" outside
            // the rounded mask, making bottom corners read as square.
            // Clear WKWebView's background so the desktop shows through
            // cleanly outside the rounded shape.
            //
            // Use the public `setValue:forKey:` API with an NSNumber-boxed
            // BOOL (KVC requires an object, not a primitive — passing a raw
            // BOOL here crashes the WK content process). Also call
            // `setUnderPageBackgroundColor:` to clear any tint behind the
            // page that might compose into the corner regions.
            // Explicitly disable the native NSWindow drop shadow.
            // tauri.conf.json sets `shadow: false` but macOS can restore it
            // after the window is first shown; calling setHasShadow:NO here
            // ensures it is off for the lifetime of the window.
            #[cfg(target_os = "macos")]
            if let Some(win) = app.get_webview_window("main") {
                use objc::{msg_send, sel, sel_impl};
                use objc::runtime::Object;
                type Id = *mut Object;
                if let Ok(ns_win_ptr) = win.ns_window() {
                    unsafe {
                        let ns_win: Id = ns_win_ptr as Id;
                        let _: () = msg_send![ns_win, setHasShadow: false];
                    }
                }
            }

            #[cfg(target_os = "macos")]
            if let Some(webview_window) = app.get_webview_window("main") {
                let _ = webview_window.with_webview(|webview| unsafe {
                    use objc::{class, msg_send, sel, sel_impl};
                    use objc::runtime::Object;
                    type Id = *mut Object;

                    let wk: Id = webview.inner() as _;
                    if wk.is_null() {
                        return;
                    }

                    // NSNumber numberWithBool:NO — boxes the primitive so KVC accepts it.
                    let ns_number = class!(NSNumber);
                    let boxed_no: Id = msg_send![ns_number, numberWithBool: false];

                    // NSString stringWithUTF8String:"drawsBackground"
                    let ns_string = class!(NSString);
                    let key_cstr = b"drawsBackground\0".as_ptr() as *const i8;
                    let key: Id = msg_send![ns_string, stringWithUTF8String: key_cstr];

                    let _: () = msg_send![wk, setValue: boxed_no forKey: key];

                    // Also paint the WKWebView's background colour as clear,
                    // belt-and-suspenders for the corner region.
                    let ns_color = class!(NSColor);
                    let clear_color: Id = msg_send![ns_color, clearColor];
                    let _: () = msg_send![wk, setBackgroundColor: clear_color];
                });
            }

            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("app_data_dir resolvable on desktop");
            let ctx = state::AppContext::load(app_data_dir)
                .expect("load app context");
            let arc: commands::Ctx = Arc::new(Mutex::new(ctx));

            // Schedule daily refresh; first tick fires immediately.
            scheduler::spawn_daily_refresh(app.handle().clone(), arc.clone());

            app.manage(arc);

            // Menu-bar (system tray) icon — single click wakes the widget.
            // This is the macOS-idiomatic way to bring a background widget
            // back to front without touching the Dock.
            {
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
                let icon = app.default_window_icon()
                    .expect("app icon must be set")
                    .clone();
                TrayIconBuilder::new()
                    .tooltip("Spwidget")
                    .icon(icon)
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event {
                            // Tell the frontend to wake the widget.
                            tray.app_handle().emit("tray-wake", ()).ok();
                        }
                    })
                    .build(app)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::is_configured,
            commands::get_config,
            commands::get_points,
            commands::refresh_now,
            commands::save_credentials,
            commands::list_point_candidates,
            commands::clear_credentials,
            commands::get_idle_seconds,
            commands::get_mode,
            commands::set_mode,
            commands::get_project_key,
            commands::set_project_key,
            commands::quit_app,
            commands::send_to_back,
            commands::send_to_front,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                // macOS shutdown / logout — exit immediately so we don't block.
                tauri::RunEvent::ExitRequested { .. } => std::process::exit(0),
                // Dock icon clicked while app is running → wake the widget.
                tauri::RunEvent::Reopen { .. } => { app.emit("tray-wake", ()).ok(); }
                _ => {}
            }
        });
}
