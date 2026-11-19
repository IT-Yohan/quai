//! Preferences, diagnostics and about.

use crate::backend::{args, quai};
use crate::util::esc;
use crate::window::Ui;
use adw::prelude::*;
use gtk::glib;
use quai_i18n::{tr, trf};
use quai_types::{DoctorReport, Settings};
use std::rc::Rc;

pub fn about(ui: &Rc<Ui>) {
    let about = adw::AboutDialog::builder()
        .application_name("Quai")
        .application_icon("io.github.it_yohan.Quai")
        .developer_name("The quai contributors")
        .version(env!("CARGO_PKG_VERSION"))
        .comments(tr("Keeps RPM packages published as GitHub, GitLab or Forgejo releases up to date through dnf."))
        .website("https://github.com/IT-Yohan/quai")
        .issue_url("https://github.com/IT-Yohan/quai/issues")
        .license_type(gtk::License::Apache20)
        .build();
    // Translators: your names, one "Name <email>" per line, credited in the
    // About window. Leave untranslated to show no translator.
    let credits = tr("translator-credits");
    if credits != "translator-credits" {
        about.set_translator_credits(&credits);
    }
    about.present(Some(&ui.window));
}

pub fn preferences(ui: &Rc<Ui>) {
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let settings: Settings = match quai(&args(&["config"]), false, None).await {
            Ok(s) => s,
            Err(e) => {
                ui.error_toast(&e);
                return;
            }
        };
        let dialog = adw::PreferencesDialog::new();
        dialog.set_title(&tr("Preferences"));
        let page =
            adw::PreferencesPage::builder().title(tr("General")).icon_name("preferences-system-symbolic").build();

        let checks = adw::PreferencesGroup::builder()
            .title(tr("Checking for Releases"))
            .description(tr(
                "quaid asks the forges for new releases at most this often, whenever dnf or GNOME Software refreshes.",
            ))
            .build();
        let interval = adw::SpinRow::with_range(1.0, 1440.0, 5.0);
        interval.set_title(&tr("Minimum interval (minutes)"));
        interval.set_value(settings.check_interval_mins as f64);
        let keep = adw::SpinRow::with_range(1.0, 20.0, 1.0);
        keep.set_title(&tr("Versions kept for new sources"));
        keep.set_value(settings.keep_versions as f64);
        let apply = gtk::Button::builder()
            .label(tr("Apply"))
            .valign(gtk::Align::Center)
            .css_classes(["suggested-action"])
            .sensitive(false)
            .build();
        checks.set_header_suffix(Some(&apply));
        checks.add(&interval);
        checks.add(&keep);
        {
            let apply = apply.clone();
            interval.connect_value_notify(move |_| apply.set_sensitive(true));
        }
        {
            let apply = apply.clone();
            keep.connect_value_notify(move |_| apply.set_sensitive(true));
        }
        {
            let ui = ui.clone();
            let (interval, keep) = (interval.clone(), keep.clone());
            let orig = settings.clone();
            apply.connect_clicked(move |btn| {
                btn.set_sensitive(false);
                let iv = interval.value() as u32;
                let kv = keep.value() as u32;
                if iv != orig.check_interval_mins {
                    ui.run_action(args(&["config", "check-interval-mins", &iv.to_string()]), true, None);
                }
                if kv != orig.keep_versions {
                    ui.run_action(args(&["config", "keep-versions", &kv.to_string()]), true, None);
                }
            });
        }
        page.add(&checks);

        let tokens = adw::PreferencesGroup::builder()
            .title(tr("Access Tokens"))
            .description(tr("Without a token GitHub allows 60 requests per hour. A token raises that to 5,000; \
                 for public projects it needs no permissions. Tokens are readable by root and quaid only."))
            .build();
        for t in &settings.tokens {
            let row = adw::ActionRow::builder()
                .title(esc(&t.host))
                .subtitle(if t.configured { tr("Token set") } else { tr("Optional") })
                .build();
            let set = gtk::Button::builder().label(tr("Set…")).valign(gtk::Align::Center).build();
            let host = t.host.clone();
            let ui2 = ui.clone();
            let dlg = dialog.clone();
            set.connect_clicked(move |_| ask_token(&ui2, &dlg, &host));
            row.add_suffix(&set);
            let clear = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(tr("Remove Token"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            let host = t.host.clone();
            let ui3 = ui.clone();
            clear.connect_clicked(move |_| ui3.run_action(args(&["token", "clear", "--host", &host]), true, None));
            row.add_suffix(&clear);
            tokens.add(&row);
        }
        page.add(&tokens);

        let about = adw::PreferencesGroup::builder().title(tr("Service")).build();
        let listen = adw::ActionRow::builder()
            .title(tr("quaid address"))
            .subtitle(esc(&settings.listen))
            .subtitle_selectable(true)
            .build();
        listen.add_css_class("property");
        about.add(&listen);
        let signing = adw::ActionRow::builder()
            .title(tr("Signed metadata"))
            .subtitle(if settings.sign_metadata { tr("On: dnf verifies every repomd.xml") } else { tr("Off") })
            .build();
        signing.add_css_class("property");
        about.add(&signing);
        page.add(&about);

        dialog.add(&page);
        dialog.present(Some(&ui.window));
    });
}

fn ask_token(ui: &Rc<Ui>, parent: &adw::PreferencesDialog, host: &str) {
    let entry = gtk::PasswordEntry::builder().show_peek_icon(true).activates_default(true).build();
    let dialog = adw::AlertDialog::new(
        Some(&trf("Token for {host}", &[("host", &host)])),
        Some(&tr("Paste a personal access token. It is stored in /etc/quai/private/tokens.toml.")),
    );
    dialog.set_extra_child(Some(&entry));
    dialog.add_response("cancel", &tr("Cancel"));
    dialog.add_response("save", &tr("Save"));
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");
    let ui = ui.clone();
    let host = host.to_string();
    dialog.connect_response(None, move |_, r| {
        let token = entry.text().trim().to_string();
        if r == "save" && !token.is_empty() {
            ui.run_action(args(&["token", "set", "--host", &host]), true, Some(format!("{token}\n")));
        }
    });
    dialog.present(Some(parent));
}

pub fn doctor(ui: &Rc<Ui>) {
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    let spinner = gtk::Spinner::builder().spinning(true).halign(gtk::Align::Center).build();
    let actions = gtk::Box::builder().spacing(12).halign(gtk::Align::Center).build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&spinner);
    content.append(&list);
    content.append(&actions);
    let scrolled =
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&content).build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&scrolled));
    let dialog =
        adw::Dialog::builder().title(tr("Diagnostics")).content_width(560).content_height(560).child(&toolbar).build();
    dialog.present(Some(&ui.window));

    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let res: Result<DoctorReport, String> = quai(&args(&["doctor"]), false, None).await;
        spinner.set_visible(false);
        let report = match res {
            Ok(r) => r,
            Err(e) => {
                list.append(&adw::ActionRow::builder().title(tr("Diagnostics failed")).subtitle(esc(&e)).build());
                return;
            }
        };
        for c in &report.checks {
            let mut subtitle = c.detail.clone();
            if let (false, Some(fix)) = (c.ok, &c.fix) {
                subtitle = format!("{subtitle}\n{}", trf("Fix: {command}", &[("command", fix)]));
            }
            let row = adw::ActionRow::builder().title(esc(&c.title)).subtitle(esc(&subtitle)).subtitle_lines(0).build();
            let icon = gtk::Image::from_icon_name(if c.ok { "emblem-ok-symbolic" } else { "dialog-warning-symbolic" });
            icon.add_css_class(if c.ok { "success" } else { "warning" });
            row.add_prefix(&icon);
            list.append(&row);
        }
        let failed = |ids: &[&str]| report.checks.iter().any(|c| !c.ok && ids.contains(&c.id.as_str()));
        if failed(&["config", "signing-key", "socket", "daemon"]) {
            let b = gtk::Button::builder().label(tr("Run Setup")).css_classes(["pill", "suggested-action"]).build();
            let ui2 = ui.clone();
            let d = dialog.clone();
            b.connect_clicked(move |_| {
                d.close();
                ui2.run_action(args(&["setup"]), true, None);
            });
            actions.append(&b);
        }
        if failed(&["repo-files"]) {
            let b = gtk::Button::builder().label(tr("Regenerate Repository Files")).css_classes(["pill"]).build();
            let ui2 = ui.clone();
            let d = dialog.clone();
            b.connect_clicked(move |_| {
                d.close();
                ui2.run_action(args(&["sync"]), true, None);
            });
            actions.append(&b);
        }
    });
}
