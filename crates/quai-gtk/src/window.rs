//! Main window: list of sources and a detail page per source.

use crate::backend::{args, quai};
use crate::util::*;
use adw::prelude::*;
use gtk::{gio, glib};
use quai_i18n::{tr, trc, trf, trn, trnf};
use quai_types::{ActionResult, Channel, RefreshReply, SourceStatus, StatusReport};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Ui {
    pub window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    nav: adw::NavigationView,
    stack: gtk::Stack,
    list: gtk::ListBox,
    banner: adw::Banner,
    status: RefCell<Option<StatusReport>>,
    /// Source shown on the detail page, and the box holding its content.
    detail: RefCell<Option<(String, gtk::Box)>>,
    busy: Cell<u32>,
    spinner: gtk::Spinner,
    /// QUAI_GTK_OPEN was honoured.
    opened: Cell<bool>,
}

pub fn build(app: &adw::Application) -> Rc<Ui> {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Quai")
        .default_width(760)
        .default_height(680)
        .width_request(360)
        .height_request(400)
        .build();

    let header = adw::HeaderBar::new();
    let add_btn = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text(tr("Add a Source")).build();
    add_btn.set_action_name(Some("win.add-source"));
    header.pack_start(&add_btn);

    let menu = gio::Menu::new();
    let section = gio::Menu::new();
    section.append(Some(&tr("Check All Sources Now")), Some("win.refresh-all"));
    section.append(Some(&tr("Diagnostics")), Some("win.doctor"));
    menu.append_section(None, &section);
    let section = gio::Menu::new();
    section.append(Some(&tr("Preferences")), Some("win.preferences"));
    section.append(Some(&tr("About Quai")), Some("win.about"));
    menu.append_section(None, &section);
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(tr("Main Menu"))
        .build();
    header.pack_end(&menu_btn);
    let spinner = gtk::Spinner::new();
    header.pack_end(&spinner);

    let banner = adw::Banner::new("");
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();

    let intro = gtk::Label::builder()
        .label(tr("Each source is a project whose RPM releases dnf and GNOME Software keep up to date."))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&intro);
    content.append(&list);
    let clamp = adw::Clamp::builder().maximum_size(720).child(&content).build();
    let scrolled =
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build();

    let empty_btn = gtk::Button::builder()
        .label(tr("Add a Source"))
        .halign(gtk::Align::Center)
        .css_classes(["pill", "suggested-action"])
        .action_name("win.add-source")
        .build();
    let empty = adw::StatusPage::builder()
        .icon_name("folder-download-symbolic")
        .title(tr("No Sources Yet"))
        .description(tr("Add a GitHub, GitLab or Codeberg project that publishes RPM packages.\n\
             dnf and GNOME Software will then keep it up to date."))
        .child(&empty_btn)
        .vexpand(true)
        .build();
    let loading = gtk::Spinner::builder()
        .spinning(true)
        .width_request(32)
        .height_request(32)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    let error_page = adw::StatusPage::builder()
        .icon_name("dialog-error-symbolic")
        .title(tr("Cannot Read the Sources"))
        .vexpand(true)
        .build();

    let stack = gtk::Stack::new();
    stack.add_named(&loading, Some("loading"));
    stack.add_named(&empty, Some("empty"));
    stack.add_named(&scrolled, Some("list"));
    stack.add_named(&error_page, Some("error"));
    stack.set_visible_child_name("loading");

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&banner);
    body.append(&stack);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&body));
    let root_page = adw::NavigationPage::builder().title("Quai").tag("sources").child(&toolbar).build();

    let nav = adw::NavigationView::new();
    nav.add(&root_page);
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&nav));
    window.set_content(Some(&toasts));

    let ui = Rc::new(Ui {
        window,
        toasts,
        nav,
        stack,
        list,
        banner,
        status: RefCell::new(None),
        detail: RefCell::new(None),
        busy: Cell::new(0),
        spinner,
        opened: Cell::new(false),
    });

    {
        let ui2 = ui.clone();
        ui.banner.connect_button_clicked(move |_| {
            let ui = ui2.clone();
            ui.run_action(args(&["setup"]), true, None);
        });
    }
    {
        let ui2 = ui.clone();
        ui.nav.connect_popped(move |_, page| {
            if page.tag().as_deref() == Some("detail") {
                ui2.detail.replace(None);
            }
        });
    }
    ui
}

impl Ui {
    pub fn toast(&self, text: &str) {
        let t = adw::Toast::new(text);
        t.set_timeout(4);
        self.toasts.add_toast(t);
    }

    pub fn error_toast(&self, text: &str) {
        let t = adw::Toast::builder().title(esc(text)).timeout(8).build();
        self.toasts.add_toast(t);
    }

    fn begin(&self) {
        self.busy.set(self.busy.get() + 1);
        self.spinner.set_spinning(true);
    }

    fn end(&self) {
        self.busy.set(self.busy.get().saturating_sub(1));
        if self.busy.get() == 0 {
            self.spinner.set_spinning(false);
        }
    }

    /// Re-read `quai status` and redraw.
    pub fn reload(self: &Rc<Self>) {
        let ui = self.clone();
        glib::spawn_future_local(async move {
            ui.begin();
            let res: Result<StatusReport, String> = quai(&args(&["status"]), false, None).await;
            ui.end();
            match res {
                Ok(report) => {
                    ui.status.replace(Some(report));
                    ui.populate();
                    // Tests and screenshots: QUAI_GTK_OPEN=<source id> opens that source once loaded.
                    if let Some(id) = std::env::var("QUAI_GTK_OPEN").ok().filter(|_| !ui.opened.replace(true)) {
                        ui.open_detail(&id);
                    }
                }
                Err(e) => {
                    if let Some(page) = ui.stack.child_by_name("error").and_downcast::<adw::StatusPage>() {
                        page.set_description(Some(&esc(&e)));
                    }
                    ui.stack.set_visible_child_name("error");
                }
            }
        });
    }

    fn populate(self: &Rc<Self>) {
        let status = self.status.borrow();
        let Some(report) = status.as_ref() else {
            return;
        };

        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for s in &report.sources {
            self.list.append(&self.source_row(s));
        }
        self.stack.set_visible_child_name(if report.sources.is_empty() { "empty" } else { "list" });

        let not_set_up = !report.sources.is_empty() && !report.daemon_reachable;
        self.banner.set_title(&tr("The quai service is not answering, so dnf cannot see your sources."));
        self.banner.set_button_label(Some(&tr("Set Up")));
        self.banner.set_revealed(not_set_up);

        let detail_id = self.detail.borrow().as_ref().map(|(id, _)| id.clone());
        if let Some(id) = detail_id {
            match report.sources.iter().find(|s| s.source.id == id) {
                Some(s) => self.fill_detail(s),
                None => {
                    self.nav.pop_to_tag("sources");
                }
            }
        }
    }

    fn source_row(self: &Rc<Self>, s: &SourceStatus) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(esc(&s.source.name))
            .subtitle(esc(&source_subtitle(s)))
            .activatable(true)
            .build();
        let icon = gtk::Image::from_icon_name(state_icon(s.state));
        icon.add_css_class(state_css(s.state));
        row.add_prefix(&icon);
        let label =
            gtk::Label::builder().label(state_label(s.state)).css_classes(["caption", state_css(s.state)]).build();
        row.add_suffix(&label);
        row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let ui = self.clone();
        let id = s.source.id.clone();
        row.connect_activated(move |_| ui.open_detail(&id));
        row
    }

    fn open_detail(self: &Rc<Self>, id: &str) {
        let source = {
            let status = self.status.borrow();
            status.as_ref().and_then(|r| r.sources.iter().find(|s| s.source.id == id).cloned())
        };
        let Some(s) = source else { return };
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        let clamp = adw::Clamp::builder().maximum_size(720).child(&content).build();
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let check = gtk::Button::builder().icon_name("view-refresh-symbolic").tooltip_text(tr("Check Now")).build();
        {
            let ui = self.clone();
            let id = id.to_string();
            check.connect_clicked(move |_| ui.refresh(Some(id.clone())));
        }
        header.pack_end(&check);
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scrolled));
        let page = adw::NavigationPage::builder().title(s.source.name.as_str()).tag("detail").child(&toolbar).build();
        self.detail.replace(Some((id.to_string(), content)));
        self.fill_detail(&s);
        self.nav.push(&page);
    }

    /// (Re)build the detail page content for `s`.
    fn fill_detail(self: &Rc<Self>, s: &SourceStatus) {
        let detail = self.detail.borrow();
        let Some((_, content)) = detail.as_ref() else {
            return;
        };
        while let Some(child) = content.first_child() {
            content.remove(&child);
        }
        let id = s.source.id.clone();

        // Header: name, project link, state.
        let title = gtk::Label::builder().label(&s.source.name).css_classes(["title-1"]).wrap(true).build();
        let state = gtk::Label::builder().label(state_label(s.state)).css_classes([state_css(s.state)]).build();
        let link = gtk::LinkButton::with_label(&s.source.web_url, &format!("{}/{}", s.source.host, s.source.project));
        let head = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).build();
        head.append(&title);
        head.append(&state);
        head.append(&link);
        content.append(&head);

        if let Some(err) = &s.last_error {
            let banner = adw::PreferencesGroup::new();
            let row = adw::ActionRow::builder().title(tr("Last check failed")).subtitle(esc(err)).build();
            row.add_css_class("error");
            row.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));
            banner.add(&row);
            content.append(&banner);
        }

        // Packages.
        let pkgs = adw::PreferencesGroup::builder().title(tr("Packages")).build();
        let mut any_update = false;
        for p in &s.packages {
            let subtitle = match (&p.installed, &p.available) {
                (Some(i), Some(a)) if p.update_available => {
                    trf("Installed {installed} · {available} available", &[("installed", i), ("available", a)])
                }
                (Some(i), _) => trf("Installed {version}", &[("version", i)]),
                (None, Some(a)) => trf("Not installed · {version} available", &[("version", a)]),
                (None, None) => trc("package state", "Not installed"),
            };
            let row = adw::ActionRow::builder().title(esc(&p.name)).subtitle(esc(&subtitle)).build();
            if let Some(arch) = &p.arch {
                row.add_suffix(&gtk::Label::builder().label(arch).css_classes(["caption", "dim-label"]).build());
            }
            any_update |= p.update_available;
            pkgs.add(&row);
        }
        let action_btn = if s.packages.iter().all(|p| p.installed.is_none()) {
            let b = gtk::Button::builder().label(tr("Install")).css_classes(["suggested-action"]).build();
            let ui = self.clone();
            let id2 = id.clone();
            b.connect_clicked(move |_| ui.run_action(args(&["install", &id2]), true, None));
            Some(b)
        } else if any_update {
            let b = gtk::Button::builder().label(trc("verb", "Update")).css_classes(["suggested-action"]).build();
            let ui = self.clone();
            let id2 = id.clone();
            b.connect_clicked(move |_| ui.run_action(args(&["upgrade", &id2]), true, None));
            Some(b)
        } else {
            None
        };
        if let Some(b) = action_btn {
            b.set_valign(gtk::Align::Center);
            pkgs.set_header_suffix(Some(&b));
        }
        content.append(&pkgs);

        for x in &s.excluded {
            let g = adw::PreferencesGroup::builder().title(tr("Not Served")).build();
            let row = adw::ActionRow::builder()
                .title(esc(&format!("{} ({})", x.name, x.file)))
                .subtitle(esc(&x.reason))
                .build();
            row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
            g.add(&row);
            content.append(&g);
        }

        // Settings.
        let settings = adw::PreferencesGroup::builder().title(tr("Source")).build();
        let enabled = adw::SwitchRow::builder()
            .title(tr("Enabled"))
            .subtitle(tr("When off, dnf ignores this source"))
            .active(s.source.enabled)
            .build();
        {
            let ui = self.clone();
            let id = id.clone();
            enabled.connect_active_notify(move |row| {
                let verb = if row.is_active() { "enable" } else { "disable" };
                ui.run_action(args(&[verb, &id]), true, None);
            });
        }
        settings.add(&enabled);

        let channel = adw::ComboRow::builder()
            .title(tr("Channel"))
            .subtitle(tr("Pre-releases are often less tested"))
            .model(&{
                let items = [tr("Stable releases"), tr("Pre-releases too")];
                gtk::StringList::new(&items.iter().map(String::as_str).collect::<Vec<_>>())
            })
            .selected(if s.source.channel == Channel::Prerelease { 1 } else { 0 })
            .build();
        {
            let ui = self.clone();
            let id = id.clone();
            channel.connect_selected_notify(move |row| {
                let v = if row.selected() == 1 { "prerelease" } else { "stable" };
                ui.run_action(args(&["set", &id, "channel", v]), true, None);
            });
        }
        settings.add(&channel);

        let keep = adw::SpinRow::with_range(1.0, 20.0, 1.0);
        keep.set_title(&tr("Versions Kept"));
        keep.set_subtitle(&tr("Older versions stay available for dnf downgrade"));
        keep.set_value(s.source.keep_versions as f64);
        let keep_apply = gtk::Button::builder()
            .icon_name("object-select-symbolic")
            .tooltip_text(tr("Apply"))
            .valign(gtk::Align::Center)
            .visible(false)
            .css_classes(["flat"])
            .build();
        keep.add_suffix(&keep_apply);
        {
            let btn = keep_apply.clone();
            let original = s.source.keep_versions as f64;
            keep.connect_value_notify(move |row| btn.set_visible((row.value() - original).abs() > f64::EPSILON));
        }
        {
            let ui = self.clone();
            let id = id.clone();
            let row = keep.clone();
            keep_apply.connect_clicked(move |_| {
                let v = (row.value() as u32).to_string();
                ui.run_action(args(&["set", &id, "keep-versions", &v]), true, None);
            });
        }
        settings.add(&keep);

        let pattern = adw::EntryRow::builder().title(tr("Asset pattern (regex)")).show_apply_button(true).build();
        pattern.set_text(s.source.asset_pattern.as_deref().unwrap_or(""));
        {
            let ui = self.clone();
            let id = id.clone();
            pattern.connect_apply(move |row| {
                ui.run_action(args(&["set", &id, "asset-pattern", row.text().as_str()]), true, None);
            });
        }
        settings.add(&pattern);
        content.append(&settings);

        // Status.
        let info = adw::PreferencesGroup::builder().title(tr("Status")).build();
        let row = |title: String, value: String| {
            let r = adw::ActionRow::builder().title(title).subtitle(esc(&value)).subtitle_selectable(true).build();
            r.add_css_class("property");
            r
        };
        info.add(&row(tr("Latest release"), s.latest_release.clone().unwrap_or_else(|| "—".into())));
        info.add(&row(tr("Last check"), s.last_check.as_deref().map(relative_time).unwrap_or_else(|| tr("never"))));
        info.add(&row(tr("dnf repository"), format!("quai-{id}")));
        if !s.repo_file_in_sync {
            let r = adw::ActionRow::builder()
                .title(tr("Repository file out of date"))
                .subtitle(tr("Regenerate /etc/yum.repos.d/quai-*.repo"))
                .build();
            let b = gtk::Button::builder().label(tr("Fix")).valign(gtk::Align::Center).build();
            let ui = self.clone();
            b.connect_clicked(move |_| ui.run_action(args(&["sync"]), true, None));
            r.add_suffix(&b);
            info.add(&r);
        }
        content.append(&info);

        // Danger zone.
        let remove = gtk::Button::builder()
            .label(tr("Remove Source"))
            .halign(gtk::Align::Center)
            .css_classes(["pill", "destructive-action"])
            .build();
        {
            let ui = self.clone();
            let s = s.clone();
            remove.connect_clicked(move |_| ui.confirm_remove(&s));
        }
        content.append(&remove);
    }

    fn confirm_remove(self: &Rc<Self>, s: &SourceStatus) {
        let installed: Vec<String> =
            s.packages.iter().filter(|p| p.installed.is_some()).map(|p| p.name.clone()).collect();
        let body = if installed.is_empty() {
            tr("dnf will stop seeing this source.")
        } else {
            trnf(
                "{packages} will stay installed but stop receiving updates, unless you uninstall it too.",
                "{packages} will stay installed but stop receiving updates, unless you uninstall them too.",
                installed.len() as u64,
                &[("packages", &installed.join(", "))],
            )
        };
        let dialog = adw::AlertDialog::new(Some(&trf("Remove {name}?", &[("name", &s.source.name)])), Some(&body));
        dialog.add_response("cancel", &tr("Cancel"));
        if !installed.is_empty() {
            dialog.add_response("uninstall", &tr("Remove and Uninstall"));
            dialog.set_response_appearance("uninstall", adw::ResponseAppearance::Destructive);
        }
        dialog.add_response("remove", &tr("Remove"));
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let ui = self.clone();
        let id = s.source.id.clone();
        dialog.connect_response(None, move |_, response| {
            let a = match response {
                "remove" => args(&["remove", &id]),
                "uninstall" => args(&["remove", &id, "--uninstall"]),
                _ => return,
            };
            ui.nav.pop_to_tag("sources");
            ui.run_action(a, true, None);
        });
        dialog.present(Some(&self.window));
    }

    /// Run a privileged (or not) command returning an `ActionResult`, toast it, reload.
    pub fn run_action(self: &Rc<Self>, a: Vec<String>, privileged: bool, stdin: Option<String>) {
        let ui = self.clone();
        glib::spawn_future_local(async move {
            ui.begin();
            let res: Result<ActionResult, String> = quai(&a, privileged, stdin).await;
            ui.end();
            match res {
                Ok(r) if r.ok => {
                    let mut msg = r.message.clone();
                    if r.reboot_required {
                        msg = format!("{msg} — {}", tr("restart to apply"));
                    }
                    ui.toast(&msg);
                }
                Ok(r) => ui.error_toast(&r.message),
                Err(e) => ui.error_toast(&e),
            }
            ui.reload();
        });
    }

    /// Ask quaid to check forges now, then reload.
    pub fn refresh(self: &Rc<Self>, id: Option<String>) {
        let ui = self.clone();
        glib::spawn_future_local(async move {
            ui.begin();
            let mut a = args(&["refresh"]);
            if let Some(id) = &id {
                a.push(id.clone());
            }
            let res: Result<RefreshReply, String> = quai(&a, false, None).await;
            ui.end();
            match res {
                Ok(r) if r.errors.is_empty() => {
                    let n = r.refreshed.len();
                    ui.toast(&match (n, r.skipped.len()) {
                        (0, _) => tr("Checked less than 30 seconds ago"),
                        (1, _) => tr("Checked for new releases"),
                        (n, _) => trn("Checked {n} source", "Checked {n} sources", n as u64),
                    });
                }
                Ok(r) => {
                    ui.error_toast(&r.errors.iter().map(|(id, e)| format!("{id}: {e}")).collect::<Vec<_>>().join("\n"))
                }
                Err(e) => ui.error_toast(&e),
            }
            ui.reload();
        });
    }
}
