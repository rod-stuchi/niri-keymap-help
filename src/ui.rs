use std::{cell::RefCell, collections::HashMap, path::Path, rc::Rc};

use gtk::{gdk, glib, prelude::*};
use niri_keymap_help::{
    config::{Binding, DescriptionSource, LoadReport},
    search,
    sort::{self, SortMode},
    timing,
    watch::Watcher,
};

struct Row {
    binding: Binding,
    widget: gtk::ListBoxRow,
    description: gtk::Label,
}

/// Read by the ListBox header function. Kept outside `View` because GTK
/// calls that function synchronously while `View` is mutably borrowed.
#[derive(Default)]
struct Headers {
    grouped: bool,
    visible: HashMap<String, usize>,
}

struct View {
    rows: Vec<Row>,
    loaded: bool,
    mode: SortMode,
    headers: Rc<RefCell<Headers>>,
    /// Row indices in display order, to skip re-appending when unchanged.
    order: RefCell<Vec<usize>>,
    list: gtk::ListBox,
    search: gtk::SearchEntry,
    count: gtk::Label,
    missing: gtk::Label,
    message: gtk::Label,
    empty: gtk::Label,
    /// Shown until the first report arrives, so the window can paint first.
    loading: gtk::Box,
    spinner: gtk::Spinner,
    sort_mode: gtk::DropDown,
    scroll: gtk::ScrolledWindow,
}

impl View {
    fn filter(&self, reset_scroll: bool) {
        let query = self.search.text();
        self.apply_order(&query);
        let mut visible = 0;
        let mut missing = 0;
        let mut groups = HashMap::new();
        for row in &self.rows {
            let matches = search::matches(&row.binding, &query);
            row.widget.set_visible(matches);
            if matches {
                visible += 1;
                missing += usize::from(row.binding.description.is_none());
                *groups
                    .entry(sort::group_of(&row.binding).to_owned())
                    .or_insert(0) += 1;
            }
        }
        *self.headers.borrow_mut() = Headers {
            grouped: self.mode == SortMode::Grouped,
            visible: groups,
        };
        self.list.invalidate_headers();
        let waiting = self.spinner.is_spinning();
        self.loading.set_visible(waiting);
        self.sort_mode.set_sensitive(self.loaded);
        if self.loaded {
            self.count
                .set_text(&format!("{visible} of {} bindings", self.rows.len()));
        } else {
            self.count.set_text("Loading bindings…");
        }
        self.missing
            .set_text(&format!("{missing} without descriptions"));
        self.missing.set_visible(missing > 0);
        self.empty.set_text(if !self.loaded {
            "Waiting for a readable config…"
        } else if self.rows.is_empty() {
            "No active bindings in this config."
        } else {
            "No matching bindings. Try a key, description, or command."
        });
        self.empty.set_visible(visible == 0 && !waiting);
        self.scroll.set_visible(visible > 0);
        if reset_scroll {
            self.scroll.vadjustment().set_value(0.0);
        }
    }

    /// Re-append rows in the current sort order. In grouped mode the header
    /// shows the group, so descriptions drop their `Group:` prefix.
    fn apply_order(&self, query: &str) {
        let bindings: Vec<_> = self.rows.iter().map(|row| row.binding.clone()).collect();
        let order = sort::order(&bindings, self.mode, query);
        if *self.order.borrow() == order {
            return;
        }
        for row in &self.rows {
            if row.widget.parent().is_some() {
                self.list.remove(&row.widget);
            }
        }
        for &index in &order {
            let row = &self.rows[index];
            if let Some(description) = &row.binding.description {
                let text = match self.mode {
                    SortMode::Grouped => sort::split_group(description)
                        .map_or(description.as_str(), |(_, rest)| rest),
                    _ => description.as_str(),
                };
                row.description.set_text(text);
            }
            self.list.append(&row.widget);
        }
        *self.order.borrow_mut() = order;
    }

    fn set_mode(&mut self, mode: SortMode) {
        self.mode = mode;
        self.order.borrow_mut().clear();
        self.filter(true);
    }

    fn update(&mut self, report: LoadReport) {
        self.spinner.stop();
        match report.result {
            Ok(snapshot) => {
                let scroll = self.scroll.vadjustment().value();
                for row in self.rows.drain(..) {
                    self.list.remove(&row.widget);
                }
                for binding in snapshot.bindings {
                    let (widget, description) = binding_row(&binding);
                    widget.set_widget_name(sort::group_of(&binding));
                    self.rows.push(Row {
                        binding,
                        widget,
                        description,
                    });
                }
                self.order.borrow_mut().clear();
                self.loaded = true;
                self.message.remove_css_class("error-message");
                self.message.set_text(&snapshot.warnings.join("\n"));
                self.message.set_visible(!snapshot.warnings.is_empty());
                self.filter(false);
                let adjustment = self.scroll.vadjustment();
                glib::idle_add_local_once(move || adjustment.set_value(scroll));
            }
            Err(error) => {
                let suffix = if self.loaded {
                    "Showing the last valid bindings. Updates resume when the config is fixed."
                } else {
                    "Updates resume automatically when the config is fixed."
                };
                self.message
                    .set_text(&format!("Could not load config\n{error}\n{suffix}"));
                self.message.add_css_class("error-message");
                self.message.set_visible(true);
                self.filter(false);
            }
        }
    }
}

pub fn build(app: &gtk::Application, path: &Path, mode: SortMode) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Niri Keymap Help")
        .default_width(920)
        .default_height(680)
        .build();
    window.add_css_class("keymap-window");

    let header = gtk::HeaderBar::new();
    let title = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title.append(&gtk::Image::from_icon_name("input-keyboard-symbolic"));
    let heading = gtk::Label::new(Some("Niri Keymap Help"));
    heading.add_css_class("heading");
    title.append(&heading);
    header.set_title_widget(Some(&title));
    window.set_titlebar(Some(&header));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let top = gtk::Box::new(gtk::Orientation::Vertical, 12);
    top.add_css_class("search-panel");
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search keymaps, descriptions, or commands…")
        .hexpand(true)
        .build();
    search.set_key_capture_widget(Some(&window));
    search.set_tooltip_text(Some(
        "Search keys, descriptions, and commands (including arguments). Each word must match; case is ignored.",
    ));
    top.append(&search);

    let summary = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let count = gtk::Label::new(Some("Loading bindings…"));
    count.set_xalign(0.0);
    count.set_hexpand(true);
    count.add_css_class("dim-label");
    let missing = gtk::Label::new(None);
    missing.add_css_class("missing-count");
    missing.set_visible(false);
    let labels = SortMode::ALL.map(SortMode::label);
    let sort_mode = gtk::DropDown::from_strings(&labels);
    sort_mode.set_focusable(false);
    sort_mode.set_tooltip_text(Some("Sort order (Ctrl+S cycles)"));
    sort_mode.set_selected(SortMode::ALL.iter().position(|&m| m == mode).unwrap_or(0) as u32);
    summary.append(&count);
    summary.append(&sort_mode);
    summary.append(&missing);
    top.append(&summary);
    content.append(&top);

    let message = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    message.add_css_class("config-message");
    content.append(&message);

    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    columns.add_css_class("column-headings");
    let key_heading = gtk::Label::new(Some("KEYMAP"));
    key_heading.set_xalign(0.0);
    key_heading.set_size_request(250, -1);
    columns.append(&key_heading);
    columns.append(&gtk::Label::new(Some("DESCRIPTION / ACTION")));
    content.append(&columns);

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .focusable(false)
        .build();
    list.add_css_class("bindings");
    let headers = Rc::new(RefCell::new(Headers::default()));
    list.set_header_func(glib::clone!(
        #[strong]
        headers,
        move |row, before| {
            let headers = headers.borrow();
            let group = row.widget_name();
            let starts_group = before.is_none_or(|before| before.widget_name() != group);
            if !headers.grouped || !starts_group {
                row.set_header(None::<&gtk::Widget>);
                return;
            }
            let count = headers.visible.get(group.as_str()).copied().unwrap_or(0);
            let label = gtk::Label::builder()
                .label(format!("{}  ·  {count}", group.to_uppercase()))
                .xalign(0.0)
                .build();
            label.add_css_class("group-header");
            row.set_header(Some(&label));
        }
    ));
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    scroll.set_can_focus(false);
    content.append(&scroll);
    let empty = gtk::Label::builder().wrap(true).vexpand(true).build();
    empty.add_css_class("empty-state");
    content.append(&empty);
    let spinner = gtk::Spinner::builder()
        .spinning(true)
        .width_request(32)
        .height_request(32)
        .build();
    let loading = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .vexpand(true)
        .build();
    loading.add_css_class("loading-state");
    loading.append(&spinner);
    loading.append(&gtk::Label::new(Some("Loading keymaps…")));
    content.append(&loading);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    footer.add_css_class("footer");
    let source = gtk::Label::builder()
        .label(path.to_string_lossy())
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .hexpand(true)
        .xalign(0.0)
        .build();
    source.set_tooltip_text(Some(&format!(
        "{}\nAutomatically reloads when files change.",
        path.display()
    )));
    footer.append(&source);
    footer.append(&gtk::Label::new(Some(
        "Esc  clear / close   ·   Ctrl+S  sort",
    )));
    content.append(&footer);
    window.set_child(Some(&content));

    let view = Rc::new(RefCell::new(View {
        rows: Vec::new(),
        loaded: false,
        mode,
        headers,
        order: RefCell::default(),
        list,
        search: search.clone(),
        count,
        missing,
        message,
        empty,
        loading,
        spinner,
        sort_mode: sort_mode.clone(),
        scroll: scroll.clone(),
    }));
    view.borrow().filter(false);
    sort_mode.connect_selected_notify(glib::clone!(
        #[weak]
        view,
        move |sort_mode| {
            let mode = SortMode::ALL[sort_mode.selected() as usize % SortMode::ALL.len()];
            view.borrow_mut().set_mode(mode);
        }
    ));
    search.connect_changed(glib::clone!(
        #[weak]
        view,
        move |_| {
            view.borrow().filter(true);
        }
    ));

    // Capture Escape before SearchEntry's default handler clears the query,
    // otherwise the same Escape could both clear it and close the window.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        window,
        #[weak]
        search,
        #[weak]
        scroll,
        #[weak]
        sort_mode,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, modifiers| {
            if key == gdk::Key::Escape {
                if search.text().is_empty() {
                    window.close();
                } else {
                    search.set_text("");
                }
                return glib::Propagation::Stop;
            }
            if modifiers.contains(gdk::ModifierType::CONTROL_MASK)
                && matches!(key, gdk::Key::s | gdk::Key::S)
            {
                let next = (sort_mode.selected() + 1) % SortMode::ALL.len() as u32;
                sort_mode.set_selected(next);
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Tab || key == gdk::Key::ISO_Left_Tab {
                search.grab_focus();
                return glib::Propagation::Stop;
            }
            if !modifiers.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK)
            {
                let adjustment = scroll.vadjustment();
                let delta = match key {
                    gdk::Key::Down => 64.0,
                    gdk::Key::Up => -64.0,
                    gdk::Key::Page_Down => adjustment.page_size() * 0.9,
                    gdk::Key::Page_Up => -adjustment.page_size() * 0.9,
                    _ => return glib::Propagation::Proceed,
                };
                let max = (adjustment.upper() - adjustment.page_size()).max(0.0);
                adjustment.set_value((adjustment.value() + delta).clamp(0.0, max));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    ));
    window.add_controller(keys);
    window.connect_is_active_notify(glib::clone!(
        #[weak]
        search,
        move |window| {
            if window.is_active() {
                search.grab_focus();
            }
        }
    ));
    window.connect_focus_widget_notify(glib::clone!(
        #[weak]
        search,
        move |window| {
            if window.is_active()
                && gtk::prelude::GtkWindowExt::focus(window).is_none_or(|focus| {
                    focus != search
                        && !focus.is_ancestor(&search)
                        && focus.ancestor(gtk::Popover::static_type()).is_none()
                })
            {
                search.grab_focus();
            }
        }
    ));

    let watcher = Watcher::start(path.to_path_buf());
    // Handle reports as soon as the worker sends them. The future owns the
    // watcher, so it (and its thread) stops once the window is gone.
    let window_ref = window.downgrade();
    glib::spawn_future_local(async move {
        while let Ok(mut report) = watcher.updates.recv().await {
            // Only the newest of several queued reports matters.
            while let Ok(newer) = watcher.updates.try_recv() {
                report = newer;
            }
            let Some(window) = window_ref.upgrade().filter(|window| window.is_visible()) else {
                break;
            };
            timing::mark("report received (ui)");
            view.borrow_mut().update(report);
            timing::mark("rows built");
            mark_next_frame(&window, "list painted");
        }
    });
    window.present();
    timing::mark("build: window presented");
    mark_next_frame(&window, "first frame clock tick");
    search.grab_focus();
}

/// Log once when the next frame clock tick starts (startup timing only). This
/// is before GTK renders and presents the frame, so GPU setup cost is not
/// included; use `WAYLAND_DEBUG=client` and look for `wl_surface.attach` to
/// see when pixels actually reach the compositor.
fn mark_next_frame(window: &impl IsA<gtk::Widget>, label: &'static str) {
    window.add_tick_callback(move |_, _| {
        timing::mark(label);
        glib::ControlFlow::Break
    });
}

fn binding_row(binding: &Binding) -> (gtk::ListBoxRow, gtk::Label) {
    let row = gtk::ListBoxRow::builder()
        .activatable(false)
        .selectable(false)
        .focusable(false)
        .build();
    row.add_css_class("binding-row");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    let keys = gtk::Label::builder()
        .label(&binding.keys)
        .xalign(0.0)
        .yalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .width_chars(1)
        .build();
    keys.set_size_request(250, -1);
    keys.add_css_class("key-combo");
    content.append(&keys);

    let details = gtk::Box::new(gtk::Orientation::Vertical, 5);
    details.set_hexpand(true);
    let description = gtk::Label::builder()
        .label(binding.description.as_deref().unwrap_or("⚠ No description"))
        .xalign(0.0)
        .wrap(true)
        .width_chars(1)
        .build();
    description.add_css_class("description");
    details.append(&description);
    let action = gtk::Label::builder()
        .label(&binding.action)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .width_chars(1)
        .build();
    action.add_css_class("action");
    details.append(&action);
    content.append(&details);
    if binding.description.is_none() {
        keys.add_css_class("missing-description");
        description.add_css_class("missing-description");
        row.add_css_class("undocumented");
    }
    let origin = match binding.description_source {
        Some(DescriptionSource::Title) => "Description from hotkey-overlay-title",
        Some(DescriptionSource::Comment) => "Description from an adjacent comment",
        None => "Add hotkey-overlay-title or an adjacent comment to describe this binding",
    };
    row.set_tooltip_text(Some(&format!(
        "{}:{}\n{origin}\n\n{}",
        binding.file.display(),
        binding.line,
        binding.action
    )));
    row.set_child(Some(&content));
    (row, description)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
        let mut result = Vec::new();
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            result.extend(descendants(&widget));
            result.push(widget);
        }
        result
    }

    fn wait_for(mut predicate: impl FnMut() -> bool) {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            while context.pending() {
                context.iteration(false);
            }
            if predicate() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "UI did not reach the expected state"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn has_label(window: &gtk::Window, text: &str) -> bool {
        descendants(window).iter().any(|widget| {
            widget
                .downcast_ref::<gtk::Label>()
                .is_some_and(|label| label.text().contains(text))
        })
    }

    #[test]
    #[ignore = "requires a graphical display; run under xvfb-run"]
    fn search_focus_and_reload_keep_the_ui_consistent() {
        gtk::init().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.kdl");
        let mut config = String::from(
            "binds {\nMod+T hotkey-overlay-title=\"Open a Terminal\" { spawn \"kitty\"; }\n// Close window.\nMod+Q { close-window; }\n",
        );
        for key in 1..=24 {
            config.push_str(&format!("Mod+F{key} {{ focus-workspace {key}; }}\n"));
        }
        config.push_str("}\n");
        fs::write(&path, &config).unwrap();
        let app = gtk::Application::builder()
            .application_id("io.github.niri_keymap_help.test")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        build(&app, &path, SortMode::Config);
        let window = app.windows().into_iter().next().unwrap();
        // The window is built before any config is read: a spinner shows first.
        let spinner = descendants(&window)
            .into_iter()
            .find_map(|widget| widget.downcast::<gtk::Spinner>().ok())
            .unwrap();
        let loading = spinner.parent().unwrap();
        assert!(spinner.is_spinning() && loading.is_visible());
        assert!(has_label(&window, "Loading bindings…"));
        wait_for(|| has_label(&window, "26 of 26 bindings"));
        assert!(!spinner.is_spinning() && !loading.is_visible());
        let widgets = descendants(&window);
        let search = widgets
            .iter()
            .find_map(|widget| widget.downcast_ref::<gtk::SearchEntry>())
            .unwrap();
        let scroll = widgets
            .iter()
            .find_map(|widget| widget.downcast_ref::<gtk::ScrolledWindow>())
            .unwrap();
        assert!(has_label(&window, "24 without descriptions"));
        let focus = gtk::prelude::GtkWindowExt::focus(&window).unwrap();
        assert!(focus == *search || focus.is_ancestor(search));
        scroll.vadjustment().set_value(200.0);
        search.set_text("MOD terminal");
        wait_for(|| has_label(&window, "1 of 26 bindings"));
        assert_eq!(scroll.vadjustment().value(), 0.0);
        assert_eq!(
            descendants(&window)
                .iter()
                .filter(|widget| { widget.is::<gtk::ListBoxRow>() && widget.is_visible() })
                .count(),
            1
        );

        fs::write(&path, "binds {\n").unwrap();
        wait_for(|| has_label(&window, "Showing the last valid bindings"));
        assert_eq!(search.text(), "MOD terminal");
        assert!(has_label(&window, "1 of 26 bindings"));

        fs::write(&path, config.replace("Open a Terminal", "Terminal updated")).unwrap();
        wait_for(|| has_label(&window, "Terminal updated"));
        assert_eq!(search.text(), "MOD terminal");
        assert!(has_label(&window, "1 of 26 bindings"));
        search.set_text("no-match-at-all");
        assert!(has_label(&window, "0 of 26 bindings"));
        assert!(has_label(&window, "No matching bindings"));

        search.set_text("");
        let sort_mode = descendants(&window)
            .into_iter()
            .find_map(|widget| widget.downcast::<gtk::DropDown>().ok())
            .unwrap();
        sort_mode.set_selected(0);
        wait_for(|| has_label(&window, "NO DESCRIPTION  ·  24"));
        assert!(has_label(&window, "OTHER  ·  2"));
        window.close();
        wait_for(|| !window.is_visible());
    }
}
