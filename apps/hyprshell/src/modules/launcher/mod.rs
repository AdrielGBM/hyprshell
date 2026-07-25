//! The application launcher: a modal that owns the keyboard while it is up.

use std::rc::Rc;

use rsx::{
    AlignItems, Container, Input, Key, KeyboardMode, LayoutError, LayoutItem, LayoutStyle, NamedKey,
    RectStyle, SizeDimension, StyledContainer, SurfacePlacement, SurfaceToken, Text, TextStyle,
    box_item, memo, open_surface, set_theme, signal,
};

use crate::core::config::LauncherConfig;
use crate::core::shell;
use crate::shared::search::{self, Mode};
use crate::shared::services::apps::{self, App};
use crate::shared::services::state;
use crate::shared::theme::{FontRole, NordTheme};

/// The id the surface registry keys the launcher on.
pub const ID: &str = "launcher";

/// The results for `query`, ranked and capped.
///
/// Familiarity is folded into the score rather than sorted on separately, so a much better text match still
/// wins over a slightly more familiar app — the user typing `fir` means Firefox even if they open Files more.
pub fn results(apps: Vec<App>, query: &str, config: &LauncherConfig) -> Vec<App> {
    let counts = state::get().launch_counts;
    let mode = if config.fuzzy {
        Mode::Fuzzy
    } else {
        Mode::Substring
    };
    let hidden = config.hidden.clone();
    let visible: Vec<App> = apps.into_iter().filter(|a| !hidden.contains(&a.id)).collect();

    let mut ranked = search::rank(
        visible,
        query,
        mode,
        |app| app.haystack(),
        move |app| {
            // Diminishing: the 50th launch should not outweigh a better name match, but the difference between
            // never-used and used-daily should be visible.
            let launches = counts.get(&app.id).copied().unwrap_or(0);
            (launches as f32).sqrt().round() as i32 * 4
        },
    );
    ranked.truncate(config.max_results.max(1) as usize);
    ranked
}

/// Opens the launcher, or closes it if it is already up.
pub fn toggle() {
    shell::toggle_window(ID, open);
}

fn open() -> SurfaceToken {
    let config = shell::config();
    let theme = config
        .as_ref()
        .map(|c| c.resolve_theme())
        .unwrap_or_else(NordTheme::new);
    let launcher = config.map(|c| c.launcher.clone()).unwrap_or_default();
    let output = shell::focused_output();

    // No `.size(...)`: an overlay carries a scrim, so its *surface* is full-screen and the `SurfaceScaffold`
    // centres the panel inside it. The panel's own size is a layout property (see `panel`), not a surface one —
    // asking the surface to be 640×420 would shrink the scrim to that box and leave the rest of the screen live.
    let placement = SurfacePlacement::overlay().output(output);
    open_surface(
        placement,
        Box::new(move || {
            set_theme(theme);
            panel(theme, &launcher).expect("launcher build failed")
        }),
    )
}

/// Where the arrow keys move the selection, given the current index and how many results there are.
///
/// Wraps at both ends, so holding Down cycles rather than sticking at the bottom, and Up from the first result
/// jumps to the last — which is how every launcher behaves and what the hand expects.
fn move_selection(current: usize, count: usize, down: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if down {
        (current + 1) % count
    } else {
        (current + count - 1) % count
    }
}

fn panel(theme: NordTheme, config: &LauncherConfig) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let query = signal(String::new());
    let query_read = query.read_only();
    let config = config.clone();

    // The app list is read once per open, not per keystroke: it only changes when software is installed.
    let installed = apps::all();
    let for_results = config.clone();
    let shown = memo(move || query_read.with(|q| results(installed.clone(), q, &for_results)));

    let selected = signal(0usize);
    // Typing changes the result set, so the old index would point at a different app — or past the end. Resetting
    // to the top on every query change keeps "type a few letters, press Enter" landing on the best match.
    let reset_on_query = selected.clone();
    let query_watch = query.read_only();
    rsx::effect(move || {
        query_watch.get();
        reset_on_query.set(0);
    });

    let field = search_field(query, theme)?;
    // What is left for the list once the search field and the panel's own padding have taken their share; the
    // panel then sizes to its content, so a short result list gives a short panel rather than dead space.
    let field_height = theme.font(FontRole::Title) * 1.8 + 12.0;
    let list_height = (config.height as f32 - field_height - 38.0).max(80.0);
    let list = result_list(shown.clone(), selected.read_only(), list_height, theme)?;
    let keys_shown = shown.clone();
    let keys_selected = selected;

    // A fixed-width box the scaffold centres, rather than `100%` — which inside a full-screen scrim scaffold
    // would be the whole screen. Height is left to the content so a short result list gives a short panel; the
    // list itself carries the bound (see `result_list`).
    let panel = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .gap(10.0)
            .padding_all(14.0)
            .width(config.width as f32),
        move |_| RectStyle::filled(theme.surface, config.radius),
        vec![field, list],
    )?
    // `on_key` fires before the event reaches the children, which is what lets the arrows drive the list while
    // the search field holds focus and keeps every other keystroke going to the field as typing.
    .on_key(move |key| match key {
        Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp) => {
            let down = matches!(key, Key::Named(NamedKey::ArrowDown));
            let count = keys_shown.with(|list| list.len());
            keys_selected.set(move_selection(keys_selected.peek(), count, down));
        }
        Key::Named(NamedKey::Enter) => {
            let chosen = keys_shown.with(|list| list.get(keys_selected.peek()).cloned());
            if let Some(app) = chosen {
                apps::launch(&app);
                shell::close(ID);
            }
        }
        _ => {}
    });
    Ok(Box::new(panel))
}

fn search_field(
    query: rsx::RwSignal<String>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let input = Input::new(
        query,
        LayoutStyle::new()
            .flex_grow(1.0)
            .height(theme.font(FontRole::Title) * 1.8),
        move || TextStyle::new(theme.font(FontRole::Title), theme.text),
    )?
    .placeholder(rsx::t!("launcher.placeholder"));

    let boxed = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .padding_horizontal(12.0)
            .padding_vertical(6.0)
            .width(SizeDimension::Percent(1.0)),
        move |_| RectStyle::filled(theme.base, 10.0),
        vec![box_item(input)],
    )?;
    Ok(Box::new(boxed))
}

fn result_list(
    matches: rsx::Memo<Vec<App>>,
    selected: rsx::ReadSignal<usize>,
    max_height: f32,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Built through `new_with` so the rows can reach the viewport: moving the selection has to scroll the list
    // to follow it, and only the viewport can do that.
    let scroll = rsx::LayoutScrollArea::new_with(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            // A definite bound, not `flex_grow`: inside a content-sized column there is no free space to grow
            // into, which collapsed the list to less than one row.
            .max_height(max_height),
        move |viewport| {
            let for_source = matches.clone();
            let source = move || for_source.get();
            let key = |app: &App| app.id.clone();
            // Cloned per row rather than moved: `ReactiveList` needs an `Fn`, so the builder may run many times.
            let build = move |app: App| -> Result<Box<dyn LayoutItem>, LayoutError> {
                // A row highlights when it *is* the selection, resolved by id rather than by position, so the
                // reactive list can reorder rows without the highlight following the wrong one.
                let id = app.id.clone();
                let list = matches.clone();
                let at = selected.clone();
                let is_selected = move || list.with(|l| l.get(at.get()).is_some_and(|a| a.id == id));
                let row = row(app, theme, is_selected.clone())?;

                // Follow the selection: when this row becomes the selected one, ask the viewport to bring it
                // into view. Already-visible rows are left alone, so arrowing within the visible window doesn't
                // yank the list.
                let node = row.layout_node();
                let viewport = viewport.clone();
                rsx::effect(move || {
                    if is_selected() {
                        viewport.reveal(node, 4.0);
                    }
                });
                Ok(row)
            };
            Ok(Box::new(rsx::ReactiveList::new(source, key, build)?) as Box<dyn LayoutItem>)
        },
    )?;
    Ok(Box::new(scroll))
}

fn row(
    app: App,
    theme: NordTheme,
    is_selected: impl Fn() -> bool + Clone + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon = crate::shared::icon::app_icon_view(&app.icon, 28.0)?;
    let name = app.name.clone();
    let description = app.description.clone();

    let title = Text::auto(
        move || name.clone(),
        LayoutStyle::new(),
        move || {
            TextStyle::new(theme.font(FontRole::Body), theme.text)
                .with_max_lines(1)
                .with_ellipsis(true)
        },
    )?;

    let mut lines: Vec<Box<dyn LayoutItem>> = vec![box_item(title)];
    if !description.is_empty() {
        let subtitle = Text::auto(
            move || description.clone(),
            LayoutStyle::new(),
            move || {
                TextStyle::new(theme.font(FontRole::Caption), theme.muted)
                    .with_max_lines(1)
                    .with_ellipsis(true)
            },
        )?;
        lines.push(box_item(subtitle));
    }
    let text_column = Container::new(
        LayoutStyle::new().flex_column().flex_grow(1.0).gap(1.0),
        lines,
    )?;

    let mut children: Vec<Box<dyn LayoutItem>> = Vec::new();
    if let Some(icon) = icon {
        children.push(icon);
    }
    children.push(box_item(text_column));

    let launch = Rc::new(app);
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(10.0)
            .padding_horizontal(10.0)
            .padding_vertical(7.0)
            .width(SizeDimension::Percent(1.0)),
        move |_| {
            let fill = if is_selected() {
                theme.overlay
            } else {
                rsx::Color::TRANSPARENT
            };
            RectStyle::filled(fill, 8.0)
        },
        children,
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 8.0))
    .on_press(move || {
        apps::launch(&launch);
        shell::close(ID);
    });
    Ok(Box::new(row))
}

/// The keyboard mode the launcher asks for, kept next to the surface it belongs to so the reason is visible:
/// it opens on a keybind, and the next keystroke is already its first search character.
pub const KEYBOARD: KeyboardMode = KeyboardMode::Exclusive;

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, keywords: &[&str]) -> App {
        App {
            id: id.to_string(),
            name: name.to_string(),
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
            exec: id.to_string(),
            ..App::default()
        }
    }

    fn catalog() -> Vec<App> {
        vec![
            app("firefox", "Firefox", &["www", "browser"]),
            app("files", "Files", &[]),
            app("code", "Visual Studio Code", &["editor"]),
        ]
    }

    #[test]
    fn an_empty_query_lists_everything_up_to_the_cap() {
        let config = LauncherConfig {
            max_results: 2,
            ..LauncherConfig::default()
        };
        assert_eq!(results(catalog(), "", &config).len(), 2);
        assert_eq!(
            results(catalog(), "", &LauncherConfig::default()).len(),
            3,
            "the default cap is above this catalog's size"
        );
    }

    #[test]
    fn a_query_narrows_to_matches_and_ranks_them() {
        let found = results(catalog(), "fire", &LauncherConfig::default());
        assert_eq!(found.first().map(|a| a.id.as_str()), Some("firefox"));
        assert_eq!(found.len(), 1, "nothing else contains those letters in order");
    }

    #[test]
    fn keywords_are_searchable_not_just_names() {
        // Nothing is called "www", but Firefox lists it — which is exactly what keywords are for.
        let found = results(catalog(), "www", &LauncherConfig::default());
        assert_eq!(found.first().map(|a| a.id.as_str()), Some("firefox"));
    }

    #[test]
    fn an_acronym_finds_the_editor() {
        let found = results(catalog(), "vsc", &LauncherConfig::default());
        assert_eq!(found.first().map(|a| a.id.as_str()), Some("code"));
    }

    #[test]
    fn hidden_apps_never_appear() {
        let config = LauncherConfig {
            hidden: vec!["firefox".to_string()],
            ..LauncherConfig::default()
        };
        let found = results(catalog(), "f", &config);
        assert!(
            !found.iter().any(|a| a.id == "firefox"),
            "a hidden app is hidden even when it matches"
        );
        assert!(found.iter().any(|a| a.id == "files"));
    }

    #[test]
    fn arrow_keys_wrap_at_both_ends() {
        assert_eq!(move_selection(0, 3, true), 1);
        assert_eq!(
            move_selection(2, 3, true),
            0,
            "holding Down cycles rather than sticking at the bottom"
        );
        assert_eq!(
            move_selection(0, 3, false),
            2,
            "Up from the first result reaches the last"
        );
    }

    #[test]
    fn moving_the_selection_with_no_results_stays_put() {
        // An empty list must not produce an index the result lookup would then miss on.
        assert_eq!(move_selection(0, 0, true), 0);
        assert_eq!(move_selection(5, 0, false), 0);
        assert_eq!(move_selection(0, 1, true), 0, "one result has nowhere to go");
    }

    #[test]
    fn substring_mode_is_stricter_than_fuzzy() {
        let fuzzy = LauncherConfig::default();
        let strict = LauncherConfig {
            fuzzy: false,
            ..LauncherConfig::default()
        };
        assert_eq!(results(catalog(), "ff", &fuzzy).len(), 1);
        assert_eq!(
            results(catalog(), "ff", &strict).len(),
            0,
            "substring mode refuses the gap between the two f's"
        );
    }
}
