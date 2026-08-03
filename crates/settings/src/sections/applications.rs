//! The launcher and the application list behind it.
//!
//! What is left here is the forms this area cannot say in `.rsx`: the ones whose rows are a list the machine
//! decides the length of. The static-shape forms are `.rsx` components beside this file.

use telar::{
    AlignItems, Container, Input, LayoutError, LayoutItem, LayoutStyle, ReactiveList, RectStyle,
    RwSignal, SizeDimension, StyledContainer, Text, box_item, signal,
};

use crate::form::*;
use crate::table::*;
use config::LauncherConfig;
use config::theme::{FontRole, NordTheme};
use services::apps::{self, App};
use ui::icon::icon_view;

/// How many applications the database page draws at once. `ReactiveList` builds a widget per row up front, so
/// a machine with two thousand entries would spend the UI thread before the page appeared — the same bound,
/// and the same reason, as the launcher's wallpaper grid. The search box is what reaches past it.
const APP_ROWS: usize = 200;

/// K7: the installed applications, as the launcher sees them.
///
/// `[launcher] favourites` and `hidden` are lists of desktop-entry ids, and a user does not know their
/// software by desktop-entry id — the CSV fields these replace asked them to type `org.gnome.Nautilus` from
/// memory. Here the list is the control: every application it found, each with the two switches and the icon
/// override that are the only per-app settings there are.
pub(crate) fn apps_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let query = signal(String::new());
    let favourites = signal(config.launcher.favourites.clone());
    let hidden = signal(config.launcher.hidden.clone());
    let icons = signal(config.launcher.icons.clone());

    let installed = signal(apps::all());
    let sink = installed.clone();
    platform_layershell::watch(apps::subscribe, move |apps| sink.set(apps));

    let search = text_field(
        || telar::t!("settings.field.search"),
        query.clone(),
        "firefox",
        theme,
    )?;

    let source_apps = installed.read_only();
    let source_query = query.read_only();
    let source_favourites = favourites.read_only();
    let source_hidden = hidden.read_only();
    let list = ReactiveList::with_gap(
        move || {
            // Every signal read out before any of them is mapped: `matching` does no reactive work, but a
            // `with` over one while reading the next is the borrow panic this file keeps documenting.
            let apps = source_apps.get();
            let query = source_query.get();
            let favourites = source_favourites.get();
            let hidden = source_hidden.get();
            matching(&apps, &query)
                .into_iter()
                .map(|app| AppRow {
                    favourite: favourites.contains(&app.id),
                    hidden: hidden.contains(&app.id),
                    app,
                })
                .collect()
        },
        |row: &AppRow| format!("{}|{}|{}", row.app.id, row.favourite, row.hidden),
        {
            let (favourites, hidden, icons) = (favourites.clone(), hidden.clone(), icons.clone());
            move |row: AppRow| {
                app_row(
                    row,
                    favourites.clone(),
                    hidden.clone(),
                    icons.clone(),
                    theme,
                )
            }
        },
        6.0,
    )?;

    let count_apps = installed.read_only();
    let count_query = query.read_only();
    let count = Text::auto(
        move || {
            let apps = count_apps.get();
            let query = count_query.get();
            let shown = matching(&apps, &query).len();
            telar::t!(
                "settings.apps.count",
                shown = shown.to_string(),
                total = apps.len().to_string()
            )
        },
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, theme.muted),
    )?;

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.apps"),
        move || {
            persist_with(&path, "launcher", |current| LauncherConfig {
                favourites: favourites.peek(),
                hidden: hidden.peek(),
                icons: icons.peek(),
                ..current.launcher.clone()
            });
        },
    )?;

    section(
        || telar::t!("settings.section.apps"),
        vec![search, box_item(count), Box::new(list)],
        save,
        theme,
    )
}

/// One application in the database list, and the two switches it carries.
#[derive(Clone, Debug, PartialEq)]
struct AppRow {
    app: App,
    favourite: bool,
    hidden: bool,
}

/// The applications a query narrows to, capped. Sorted by name rather than left in scan order, because the
/// directories are read most-specific-first and a user browsing a list expects an alphabet.
fn matching(apps: &[App], query: &str) -> Vec<App> {
    let needle = query.trim().to_lowercase();
    let mut found: Vec<App> = apps
        .iter()
        .filter(|app| {
            needle.is_empty()
                || app.haystack().to_lowercase().contains(&needle)
                || app.id.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
    found.sort_by_key(|app| app.name.to_lowercase());
    found.truncate(APP_ROWS);
    found
}

fn app_row(
    row: AppRow,
    favourites: RwSignal<Vec<String>>,
    hidden: RwSignal<Vec<String>>,
    icons: RwSignal<std::collections::HashMap<String, String>>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id = row.app.id.clone();
    let declared = row.app.icon.clone();
    let override_now = icons.peek().get(&id).cloned().unwrap_or_default();
    let reference = if override_now.trim().is_empty() {
        declared.clone()
    } else {
        override_now.clone()
    };

    let icon = ui::icon::app_icon_view(&reference, 24.0)?.unwrap_or(icon_view(
        || "app-window".to_string(),
        move || theme.muted,
        24.0,
    )?);

    let name = row.app.name.clone();
    let name_text = Text::auto(
        move || name.clone(),
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Body, theme.text),
    )?;
    let subtitle = id.clone();
    let id_text = Text::auto(
        move || subtitle.clone(),
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, theme.subtle),
    )?;
    let labels = Container::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_width(0.0)
            .gap(1.0),
        vec![box_item(name_text), box_item(id_text)],
    )?;

    // An `RwSignal<String>` per row rather than one field the Save button reads back: a save that walked two
    // hundred rows would have to hold two hundred signals it almost never uses. The field feeds the shared map
    // through an effect instead, and `keeping` ties that effect to this row's lifetime — a bare `effect(…)`
    // statement runs once and stops, which is the trap `shared::reactive` exists to document.
    let icon_field = signal(override_now);
    let watched = icon_field.read_only();
    let key = id.clone();
    let sync = telar::effect(move || {
        let text = watched.get();
        let mut map = icons.peek();
        let changed = match text.trim() {
            "" => map.remove(&key).is_some(),
            icon => map.insert(key.clone(), icon.to_string()).as_deref() != Some(icon),
        };
        if changed {
            icons.set(map);
        }
    });
    let placeholder = if declared.trim().is_empty() {
        telar::t!("settings.apps.no_icon")
    } else {
        declared.clone()
    };
    let boxed_icon_field = StyledContainer::new(
        LayoutStyle::new()
            .width(150.0)
            .flex_shrink(0.0)
            .padding_horizontal(8.0)
            .padding_vertical(4.0),
        move |_r| RectStyle::filled(theme.base, 8.0),
        vec![box_item(
            Input::new(
                icon_field,
                LayoutStyle::new()
                    .flex_grow(1.0)
                    .height(theme.font(FontRole::Body) * 1.6),
                move || theme.text_style(FontRole::Caption, theme.text),
            )?
            .placeholder(placeholder),
        )],
    )?;

    let star = toggle_pill(
        "star",
        row.favourite,
        theme.yellow,
        theme,
        toggle_membership(favourites, id.clone()),
    )?;
    let eye = toggle_pill(
        "eye-off",
        row.hidden,
        theme.red,
        theme,
        toggle_membership(hidden, id),
    )?;

    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(10.0)
            .padding_horizontal(10.0)
            .padding_vertical(6.0)
            .width(SizeDimension::Percent(1.0)),
        move |_r| RectStyle::filled(theme.base, 8.0),
        vec![
            icon,
            Box::new(labels),
            Box::new(boxed_icon_field),
            star,
            eye,
        ],
    )?;
    // The wrapper is what holds the field's effect for exactly this row's lifetime; it paints nothing and is
    // full-width, which is what the row already is.
    util::reactive::keeping(Box::new(row), sync)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_application_list_narrows_alphabetically_and_stops_at_the_cap() {
        let app = |id: &str, name: &str, keyword: &str| apps::App {
            id: id.to_string(),
            name: name.to_string(),
            keywords: vec![keyword.to_string()],
            ..apps::App::default()
        };
        let installed = vec![
            app("zed", "Zed", "editor"),
            app("firefox", "Firefox", "www"),
            app("org.gnome.Nautilus", "Files", "browser"),
        ];

        let names = |query: &str| {
            matching(&installed, query)
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(""),
            vec!["Files", "Firefox", "Zed"],
            "an alphabet, not the order the directories were scanned in"
        );
        assert_eq!(names("www"), vec!["Firefox"], "keywords are searchable");
        assert_eq!(
            names("nautilus"),
            vec!["Files"],
            "so is the desktop id, which is what the config actually stores"
        );
        assert!(names("zzz-no-such-app").is_empty());

        let many: Vec<apps::App> = (0..APP_ROWS + 50)
            .map(|i| app(&format!("app{i}"), &format!("App {i:04}"), ""))
            .collect();
        assert_eq!(matching(&many, "").len(), APP_ROWS);
    }
}
