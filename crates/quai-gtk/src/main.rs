//! quai-gtk: GNOME frontend for quai.
//!
//! It never touches the system itself: it runs `quai ... --json`, through
//! `pkexec` for anything that needs root, and shows the result.

mod add_dialog;
mod backend;
mod dialogs;
mod util;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.it_yohan.Quai";

fn main() -> glib::ExitCode {
    quai_i18n::init();
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        let ui = window::build(app);

        let add = gio::ActionEntry::builder("add-source")
            .activate({
                let ui = ui.clone();
                move |_: &adw::ApplicationWindow, _, _| add_dialog::show(&ui)
            })
            .build();
        let refresh = gio::ActionEntry::builder("refresh-all")
            .activate({
                let ui = ui.clone();
                move |_: &adw::ApplicationWindow, _, _| ui.refresh(None)
            })
            .build();
        let prefs = gio::ActionEntry::builder("preferences")
            .activate({
                let ui = ui.clone();
                move |_: &adw::ApplicationWindow, _, _| dialogs::preferences(&ui)
            })
            .build();
        let doctor = gio::ActionEntry::builder("doctor")
            .activate({
                let ui = ui.clone();
                move |_: &adw::ApplicationWindow, _, _| dialogs::doctor(&ui)
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate({
                let ui = ui.clone();
                move |_: &adw::ApplicationWindow, _, _| dialogs::about(&ui)
            })
            .build();
        ui.window.add_action_entries([add, refresh, prefs, doctor, about]);
        app.set_accels_for_action("win.add-source", &["<Control>n"]);
        app.set_accels_for_action("win.refresh-all", &["<Control>r", "F5"]);
        app.set_accels_for_action("win.preferences", &["<Control>comma"]);
        app.set_accels_for_action("window.close", &["<Control>w"]);

        ui.window.present();
        ui.reload();
    });
    app.run()
}
