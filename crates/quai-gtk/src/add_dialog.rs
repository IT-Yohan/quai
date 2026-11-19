//! "Add a Source": type a project, see what it ships, confirm.

use crate::backend::{args, quai};
use crate::util::*;
use crate::window::Ui;
use adw::prelude::*;
use gtk::glib;
use quai_i18n::{tr, trc, trf};
use quai_types::{ActionResult, Preview};
use std::cell::RefCell;
use std::rc::Rc;

struct AddDialog {
    ui: Rc<Ui>,
    dialog: adw::Dialog,
    stack: gtk::Stack,
    project: adw::EntryRow,
    forge: adw::ComboRow,
    channel: adw::ComboRow,
    input_error: gtk::Label,
    preview_box: gtk::Box,
    primary: gtk::Button,
    back: gtk::Button,
    spinner: gtk::Spinner,
    preview: RefCell<Option<Preview>>,
    pattern: RefCell<Option<String>>,
    id_row: RefCell<Option<adw::EntryRow>>,
    name_row: RefCell<Option<adw::EntryRow>>,
    install_row: RefCell<Option<adw::SwitchRow>>,
    preview_error: gtk::Label,
}

const FORGES: [&str; 4] = ["", "github", "gitlab", "forgejo"];

pub fn show(ui: &Rc<Ui>) {
    let project = adw::EntryRow::builder().title(tr("owner/repo or project URL")).build();
    let forge = adw::ComboRow::builder()
        .title(tr("Forge"))
        .subtitle(tr("Only needed for self-hosted forges"))
        .model(&string_list(&[tr("Detect automatically"), "GitHub".into(), "GitLab".into(), "Forgejo / Gitea".into()]))
        .build();
    let channel = adw::ComboRow::builder()
        .title(tr("Channel"))
        .model(&string_list(&[tr("Stable releases"), tr("Pre-releases too")]))
        .build();
    let group = adw::PreferencesGroup::builder()
        .title(tr("Project"))
        .description(tr(
            "A GitHub, GitLab or Codeberg project that attaches .rpm files to its releases, e.g. stablyai/orca.",
        ))
        .build();
    group.add(&project);
    group.add(&forge);
    group.add(&channel);
    let input_error = gtk::Label::builder().wrap(true).xalign(0.0).visible(false).css_classes(["error"]).build();
    let input = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
    input.append(&group);
    input.append(&input_error);

    let preview_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).build();
    let preview_error = gtk::Label::builder().wrap(true).xalign(0.0).visible(false).css_classes(["error"]).build();
    let preview_outer = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
    preview_outer.append(&preview_box);
    preview_outer.append(&preview_error);

    let stack =
        gtk::Stack::builder().transition_type(gtk::StackTransitionType::SlideLeftRight).vhomogeneous(false).build();
    stack.add_named(&page(&input), Some("input"));
    stack.add_named(&page(&preview_outer), Some("preview"));

    let back = gtk::Button::builder().label(tr("Back")).visible(false).build();
    let primary = gtk::Button::builder().label("Analyze").css_classes(["suggested-action"]).sensitive(false).build();
    let spinner = gtk::Spinner::new();
    let bottom =
        gtk::Box::builder().spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    bottom.append(&back);
    let filler = gtk::Box::builder().hexpand(true).build();
    bottom.append(&filler);
    bottom.append(&spinner);
    bottom.append(&primary);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&stack));
    toolbar.add_bottom_bar(&bottom);

    let dialog =
        adw::Dialog::builder().title(tr("Add a Source")).content_width(560).content_height(620).child(&toolbar).build();

    let st = Rc::new(AddDialog {
        ui: ui.clone(),
        dialog,
        stack,
        project,
        forge,
        channel,
        input_error,
        preview_box,
        primary,
        back,
        spinner,
        preview: RefCell::new(None),
        pattern: RefCell::new(None),
        id_row: RefCell::new(None),
        name_row: RefCell::new(None),
        install_row: RefCell::new(None),
        preview_error,
    });

    {
        let s = st.clone();
        st.project.connect_changed(move |e| {
            s.primary.set_sensitive(!e.text().trim().is_empty());
            s.input_error.set_visible(false);
        });
    }
    {
        let s = st.clone();
        st.project.connect_entry_activated(move |_| {
            if s.stack.visible_child_name().as_deref() == Some("input") && s.primary.is_sensitive() {
                s.analyze();
            }
        });
    }
    {
        let s = st.clone();
        st.primary.connect_clicked(move |_| {
            if s.stack.visible_child_name().as_deref() == Some("input") {
                s.analyze();
            } else {
                s.add();
            }
        });
    }
    {
        let s = st.clone();
        st.back.connect_clicked(move |_| {
            s.stack.set_visible_child_name("input");
            s.back.set_visible(false);
            s.primary.set_label(&tr("Analyze"));
            s.primary.set_sensitive(true);
            s.pattern.replace(None);
        });
    }
    st.dialog.present(Some(&ui.window));
    st.project.grab_focus();
}

fn page(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder()
        .maximum_size(560)
        .child(child)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
}

impl AddDialog {
    fn selection_args(&self) -> Vec<String> {
        let mut a = vec![self.project.text().trim().to_string()];
        let forge = FORGES[self.forge.selected().min(3) as usize];
        if !forge.is_empty() {
            a.push("--forge".into());
            a.push(forge.into());
        }
        a.push("--channel".into());
        a.push(if self.channel.selected() == 1 { "prerelease" } else { "stable" }.into());
        if let Some(p) = self.pattern.borrow().as_ref() {
            a.push("--asset-pattern".into());
            a.push(p.clone());
        }
        a
    }

    fn set_busy(&self, busy: bool) {
        self.spinner.set_spinning(busy);
        self.primary.set_sensitive(!busy);
        self.back.set_sensitive(!busy);
    }

    fn analyze(self: &Rc<Self>) {
        let s = self.clone();
        glib::spawn_future_local(async move {
            s.set_busy(true);
            s.input_error.set_visible(false);
            s.preview_error.set_visible(false);
            let mut a = args(&["preview"]);
            a.extend(s.selection_args());
            let res: Result<Preview, String> = quai(&a, false, None).await;
            s.set_busy(false);
            match res {
                Ok(p) => {
                    s.preview.replace(Some(p.clone()));
                    s.show_preview(&p);
                    s.stack.set_visible_child_name("preview");
                    s.back.set_visible(true);
                    s.primary.set_label(&trc("button", "Add"));
                }
                Err(e) => {
                    let on_preview = s.stack.visible_child_name().as_deref() == Some("preview");
                    let label = if on_preview { &s.preview_error } else { &s.input_error };
                    label.set_text(&e);
                    label.set_visible(true);
                }
            }
        });
    }

    fn show_preview(self: &Rc<Self>, p: &Preview) {
        let b = &self.preview_box;
        while let Some(c) = b.first_child() {
            b.remove(&c);
        }

        // The release name, unless it only repeats the tag ("1.2.0" for v1.2.0).
        let bare = |s: &str| s.trim().trim_start_matches(['v', 'V']).to_string();
        let mut title = p.release.name.clone().filter(|n| bare(n) != bare(&p.release.tag)).unwrap_or_default();
        if !title.is_empty() {
            title = format!(" — {title}");
        }
        let pkgs = adw::PreferencesGroup::builder()
            .title(esc(&format!("{}{title}", trf("Release {tag}", &[("tag", &p.release.tag)]))))
            .description(esc(&format!("{}/{}", p.host, p.project)))
            .build();
        // With variants, default to a Fedora build when there is one.
        let no_pattern = self.pattern.borrow().is_none();
        if let (Some(v), true) = (p.variants.first(), no_pattern) {
            let fedora = v.assets.iter().position(|a| {
                let l = a.to_ascii_lowercase();
                l.contains("fedora")
                    || l.match_indices(".fc").any(|(i, _)| l[i + 3..].starts_with(|c: char| c.is_ascii_digit()))
            });
            if let Some(pat) = v.patterns.get(fedora.unwrap_or(0)) {
                self.pattern.replace(Some(pat.clone()));
            }
        }
        let selected_asset = p.variants.first().and_then(|v| {
            let pat = self.pattern.borrow().clone()?;
            v.patterns.iter().position(|x| *x == pat).and_then(|i| v.assets.get(i).cloned())
        });
        let hidden: Vec<String> = p
            .variants
            .iter()
            .flat_map(|v| v.assets.iter().cloned())
            .filter(|a| Some(a) != selected_asset.as_ref())
            .collect();
        let mut list: Vec<_> = p.packages.iter().filter(|x| !hidden.contains(&x.asset_name)).cloned().collect();
        list.sort_by_key(|x| (x.arch != p.host_arch && x.arch != "noarch", x.name.clone()));
        for pkg in &list {
            let mut subtitle = pkg.summary.clone();
            if let Some(i) = &pkg.installed {
                subtitle =
                    format!("{} · {subtitle}", trf("Installed {version}, will be kept up to date", &[("version", i)]));
            }
            let row = adw::ActionRow::builder()
                .title(esc(&format!("{} {}", pkg.name, pkg.evr)))
                .subtitle(esc(&subtitle))
                .build();
            let compatible = pkg.arch == p.host_arch || pkg.arch == "noarch";
            let mut meta = pkg.arch.clone();
            if let Some(sz) = pkg.size {
                meta = format!("{meta} · {}", human_size(sz));
            }
            let label = gtk::Label::builder().label(&meta).css_classes(["caption", "dim-label"]).build();
            row.add_suffix(&label);
            if !compatible {
                row.add_css_class("dim-label");
            }
            pkgs.add(&row);
        }
        b.append(&pkgs);

        if let Some(v) = p.variants.first() {
            let g = adw::PreferencesGroup::builder()
                .title(tr("Choose a Variant"))
                .description(esc(&trf(
                    "Several files provide {package} for {arch}. quai follows the one you pick in every future release.",
                    &[("package", &v.name), ("arch", &v.arch)],
                )))
                .build();
            let model = gtk::StringList::new(&v.assets.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            let combo = adw::ComboRow::builder().title(tr("File")).model(&model).build();
            let current = self.pattern.borrow().clone();
            if let Some(i) = current.and_then(|cur| v.patterns.iter().position(|x| *x == cur)) {
                combo.set_selected(i as u32);
            }
            let s = self.clone();
            let patterns = v.patterns.clone();
            combo.connect_selected_notify(move |row| {
                if let Some(pat) = patterns.get(row.selected() as usize) {
                    s.pattern.replace(Some(pat.clone()));
                    // Re-render from the stored preview: no new network round-trip.
                    let p = s.preview.borrow().clone();
                    if let Some(p) = p {
                        let s2 = s.clone();
                        glib::idle_add_local_once(move || s2.show_preview(&p));
                    }
                }
            });
            g.add(&combo);
            b.append(&g);
        }

        let warnings: Vec<_> = p.warnings.iter().filter(|w| w.code != "multiple_variants").collect();
        if !warnings.is_empty() {
            let g = adw::PreferencesGroup::builder().title(tr("Good to Know")).build();
            for w in warnings {
                let row = adw::ActionRow::builder().title(esc(&sentence(&w.message))).title_lines(0).build();
                let icon = gtk::Image::from_icon_name(severity_icon(w.severity));
                icon.add_css_class(severity_css(w.severity));
                row.add_prefix(&icon);
                g.add(&row);
            }
            b.append(&g);
        }

        let opts = adw::PreferencesGroup::builder().title(tr("Options")).build();
        let id_row = adw::EntryRow::builder().title(tr("Source id")).text(&p.suggested_id).build();
        let name_row = adw::EntryRow::builder().title(tr("Display name")).text(&p.suggested_name).build();
        let all_installed =
            p.packages.iter().filter(|x| x.arch == p.host_arch || x.arch == "noarch").all(|x| x.installed.is_some());
        let install_row = adw::SwitchRow::builder()
            .title(tr("Install now"))
            .subtitle(if all_installed {
                tr("Already installed")
            } else {
                tr("Otherwise install later from the source page")
            })
            .active(!all_installed)
            .sensitive(!all_installed)
            .build();
        opts.add(&name_row);
        opts.add(&id_row);
        opts.add(&install_row);
        b.append(&opts);
        self.id_row.replace(Some(id_row));
        self.name_row.replace(Some(name_row));
        self.install_row.replace(Some(install_row));
        self.primary.set_sensitive(true);
    }

    fn add(self: &Rc<Self>) {
        let s = self.clone();
        glib::spawn_future_local(async move {
            let mut a = args(&["add"]);
            a.extend(s.selection_args());
            if let Some(r) = s.id_row.borrow().as_ref() {
                a.push("--id".into());
                a.push(r.text().trim().to_string());
            }
            if let Some(r) = s.name_row.borrow().as_ref() {
                a.push("--name".into());
                a.push(r.text().trim().to_string());
            }
            if s.install_row.borrow().as_ref().map(|r| r.is_active() && r.is_sensitive()).unwrap_or(false) {
                a.push("--install".into());
            }
            s.set_busy(true);
            s.preview_error.set_visible(false);
            let res: Result<ActionResult, String> = quai(&a, true, None).await;
            s.set_busy(false);
            match res {
                Ok(r) if r.ok => {
                    let mut msg = sentence(&r.message);
                    if r.reboot_required {
                        msg = format!("{msg} — {}", tr("restart to finish installing"));
                    }
                    s.ui.toast(&msg);
                    s.dialog.close();
                    s.ui.reload();
                }
                Ok(r) => {
                    s.preview_error.set_text(&sentence(&r.message));
                    s.preview_error.set_visible(true);
                }
                Err(e) => {
                    s.preview_error.set_text(&e);
                    s.preview_error.set_visible(true);
                }
            }
        });
    }
}

fn string_list(items: &[String]) -> gtk::StringList {
    gtk::StringList::new(&items.iter().map(String::as_str).collect::<Vec<_>>())
}
