//! The application launcher: a modal that owns the keyboard while it is up.

use std::rc::Rc;

use rsx::{
    AlignItems, Container, Input, Key, KeyboardMode, LayoutError, LayoutItem, LayoutStyle, NamedKey,
    RectStyle, SizeDimension, StyledContainer, SurfacePlacement, SurfaceToken, Text, TextStyle,
    box_item, memo, open_surface, set_theme, signal,
};

use crate::core::config::{LauncherAction, LauncherConfig};
use crate::core::shell;
use crate::shared::calc;
use crate::shared::search::{self, Mode};
use crate::shared::services::apps::{self, App};
use crate::shared::services::state;
use crate::shared::theme::{FontRole, NordTheme};

/// The id the surface registry keys the launcher on.
pub const ID: &str = "launcher";

/// Typing this first switches to the action mode, listing `[[launcher.actions]]` instead of applications.
const ACTION_PREFIX: char = '>';

/// Typing this first forces the calculator, for the cases auto-detection deliberately skips — `=2` echoes 2,
/// where a bare `2` is far more likely the start of an app name.
const CALC_PREFIX: char = '=';

/// One row of the launcher. The launcher lists *things you can do*, not only applications, so each mode
/// contributes the same shape and one `row` renders any of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    App(App),
    Action(LauncherAction),
    /// The calculator's answer. Selecting it copies the result rather than the whole sum, which is what you
    /// want to paste.
    Calculation { expression: String, result: String },
}

impl Entry {
    /// The identity the reactive list reconciles on and the selection is resolved by. Prefixed per kind so an
    /// app and an action that share a name can't collide into one row.
    pub fn key(&self) -> String {
        match self {
            Entry::App(app) => format!("app:{}", app.id),
            Entry::Action(action) => format!("action:{}", action.name),
            Entry::Calculation { expression, .. } => format!("calc:{expression}"),
        }
    }

    /// Whether choosing this needs a second, confirming Enter.
    pub fn is_dangerous(&self) -> bool {
        matches!(self, Entry::Action(action) if action.dangerous)
    }
}

/// Which mode a query is in, and the query with its prefix stripped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryMode {
    Apps,
    Actions,
    Calculator,
}

/// Reads the mode off the query's first character. An explicit prefix always wins; without one the query is an
/// app search, which may still *also* show a calculation (see [`entries`]).
pub fn mode_of(query: &str) -> (QueryMode, &str) {
    let trimmed = query.trim_start();
    if let Some(rest) = trimmed.strip_prefix(ACTION_PREFIX) {
        return (QueryMode::Actions, rest.trim_start());
    }
    if let Some(rest) = trimmed.strip_prefix(CALC_PREFIX) {
        return (QueryMode::Calculator, rest.trim_start());
    }
    (QueryMode::Apps, query.trim())
}

/// Every row to show for `query`, in order.
///
/// The calculator is additive rather than a mode you fall into: an unambiguous sum puts its answer at the top
/// and the app matches still follow underneath, so typing something that happens to parse as arithmetic never
/// hides the app you were reaching for.
pub fn entries(apps: Vec<App>, query: &str, config: &LauncherConfig) -> Vec<Entry> {
    let (mode, rest) = mode_of(query);
    let cap = config.max_results.max(1) as usize;
    match mode {
        QueryMode::Actions => actions(rest, config).into_iter().take(cap).collect(),
        QueryMode::Calculator => calculation(rest, config).into_iter().collect(),
        QueryMode::Apps => {
            let mut rows: Vec<Entry> = Vec::new();
            if config.calculator && calc::looks_like_math(rest) {
                rows.extend(calculation(rest, config));
            }
            rows.extend(results(apps, rest, config).into_iter().map(Entry::App));
            rows.truncate(cap);
            rows
        }
    }
}

fn calculation(expression: &str, config: &LauncherConfig) -> Option<Entry> {
    if !config.calculator {
        return None;
    }
    calc::evaluate(expression).map(|value| Entry::Calculation {
        expression: expression.trim().to_string(),
        result: calc::format(value),
    })
}

/// The declared actions matching `query`, ranked by the same matcher the app list uses so the two modes behave
/// identically under the same `fuzzy` setting.
pub fn actions(query: &str, config: &LauncherConfig) -> Vec<Entry> {
    let listed: Vec<LauncherAction> = config
        .actions
        .iter()
        .filter(|a| a.is_listed(config.enable_dangerous_actions))
        .cloned()
        .collect();
    search::rank(
        listed,
        query,
        match_mode(config),
        |action| format!("{} {}", action.name, action.description),
        |_| 0,
    )
    .into_iter()
    .map(Entry::Action)
    .collect()
}

fn match_mode(config: &LauncherConfig) -> Mode {
    if config.fuzzy {
        Mode::Fuzzy
    } else {
        Mode::Substring
    }
}

/// The applications matching `query`, ranked and capped.
///
/// Familiarity is folded into the score rather than sorted on separately, so a much better text match still
/// wins over a slightly more familiar app — the user typing `fir` means Firefox even if they open Files more.
/// Favourites are the exception: they are pinned above the ranking, in the order the user listed them, because
/// naming an app there *is* the statement that it outranks the shell's idea of relevance. A favourite that does
/// not match the query is still filtered out, so pinning never puts an irrelevant entry at the top.
pub fn results(apps: Vec<App>, query: &str, config: &LauncherConfig) -> Vec<App> {
    let counts = state::get().launch_counts;
    let hidden = config.hidden.clone();
    let visible: Vec<App> = apps.into_iter().filter(|a| !hidden.contains(&a.id)).collect();

    let mut ranked = search::rank(
        visible,
        query,
        match_mode(config),
        |app| app.haystack(),
        move |app| {
            // Diminishing: the 50th launch should not outweigh a better name match, but the difference between
            // never-used and used-daily should be visible.
            let launches = counts.get(&app.id).copied().unwrap_or(0);
            (launches as f32).sqrt().round() as i32 * 4
        },
    );
    // Pinned before the cap, so a favourite the ranking put 20th still makes a 12-row list. `sort_by_key` is
    // stable, so everything else keeps the order the ranking gave it.
    let favourites = &config.favourites;
    ranked.sort_by_key(|app| {
        favourites
            .iter()
            .position(|id| *id == app.id)
            .unwrap_or(usize::MAX)
    });
    ranked.truncate(config.max_results.max(1) as usize);
    ranked
}

/// Carries out `entry`. Returns whether the launcher should close: an armed dangerous action does not, since
/// the user still has to confirm it.
fn choose(entry: &Entry) {
    match entry {
        Entry::App(app) => apps::launch(app),
        Entry::Action(action) => apps::run_detached(action.command.clone()),
        Entry::Calculation { result, .. } => crate::shared::clipboard::copy(result),
    }
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
    let shown = memo(move || query_read.with(|q| entries(installed.clone(), q, &for_results)));

    let selected = signal(0usize);
    // Which row is armed, by key. A dangerous action needs a second Enter, and arming in place costs no extra
    // surface — the same rule the session menu's destructive tiles follow.
    let armed = signal(String::new());
    // Typing changes the result set, so the old index would point at a different app — or past the end. Resetting
    // to the top on every query change keeps "type a few letters, press Enter" landing on the best match. It also
    // disarms: a row you have navigated away from must not still be one keystroke from running.
    let reset_on_query = selected.clone();
    let disarm_on_query = armed.clone();
    let query_watch = query.read_only();
    rsx::effect(move || {
        query_watch.get();
        reset_on_query.set(0);
        disarm_on_query.set(String::new());
    });

    let field = search_field(query, theme)?;
    // What is left for the list once the search field and the panel's own padding have taken their share; the
    // panel then sizes to its content, so a short result list gives a short panel rather than dead space.
    let field_height = theme.font(FontRole::Title) * 1.8 + 12.0;
    let list_height = (config.height as f32 - field_height - 38.0).max(80.0);
    let list = result_list(
        shown.clone(),
        selected.read_only(),
        armed.read_only(),
        list_height,
        theme,
    )?;
    let keys_shown = shown.clone();
    let keys_selected = selected;
    let keys_armed = armed;

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
            let Some(entry) = chosen else { return };
            let key = entry.key();
            // A dangerous action arms on the first Enter and runs on the second. Arming leaves the launcher up
            // — closing it would be indistinguishable from having run the thing.
            if entry.is_dangerous() && keys_armed.peek() != key {
                keys_armed.set(key);
                return;
            }
            choose(&entry);
            shell::close(ID);
        }
        Key::Named(NamedKey::Escape) if !keys_armed.peek().is_empty() => {
            // Escape disarms first, so backing out of a confirmation doesn't also dismiss the launcher.
            keys_armed.set(String::new());
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
    matches: rsx::Memo<Vec<Entry>>,
    selected: rsx::ReadSignal<usize>,
    armed: rsx::ReadSignal<String>,
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
            let key = |entry: &Entry| entry.key();
            // Cloned per row rather than moved: `ReactiveList` needs an `Fn`, so the builder may run many times.
            let build = move |entry: Entry| -> Result<Box<dyn LayoutItem>, LayoutError> {
                // A row highlights when it *is* the selection, resolved by key rather than by position, so the
                // reactive list can reorder rows without the highlight following the wrong one.
                let key = entry.key();
                let armed_key = key.clone();
                let list = matches.clone();
                let at = selected.clone();
                let armed = armed.clone();
                let is_selected =
                    move || list.with(|l| l.get(at.get()).is_some_and(|e| e.key() == key));
                let is_armed = move || armed.get() == armed_key;
                let row = row(entry, theme, is_selected.clone(), is_armed)?;

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

/// The title, subtitle and icon a row shows. Resolving them per kind here keeps `row` itself one layout.
fn row_text(entry: &Entry) -> (String, String) {
    match entry {
        Entry::App(app) => (app.name.clone(), app.description.clone()),
        Entry::Action(action) => (action.name.clone(), action.description.clone()),
        // The answer is the headline and the sum the caption: what you are reading for is the number.
        Entry::Calculation { expression, result } => {
            (result.clone(), format!("{expression} ="))
        }
    }
}

/// A row's leading graphic: an application's own artwork, or an Iconify glyph for the other kinds.
fn row_icon(entry: &Entry, theme: NordTheme) -> Result<Option<Box<dyn LayoutItem>>, LayoutError> {
    const SIZE: f32 = 28.0;
    match entry {
        Entry::App(app) => crate::shared::icon::app_icon_view(&app.icon, SIZE),
        Entry::Action(action) => {
            let glyph = action.icon.clone();
            let tint = if action.dangerous { theme.red } else { theme.text };
            crate::icon_view(move || glyph.clone(), move || tint, SIZE).map(Some)
        }
        Entry::Calculation { .. } => {
            crate::icon_view(|| "equal".to_string(), move || theme.accent, SIZE).map(Some)
        }
    }
}

fn row(
    entry: Entry,
    theme: NordTheme,
    is_selected: impl Fn() -> bool + Clone + 'static,
    is_armed: impl Fn() -> bool + Clone + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon = row_icon(&entry, theme)?;
    let (name, description) = row_text(&entry);
    let dangerous = entry.is_dangerous();

    let armed_title = is_armed.clone();
    let title = Text::auto(
        move || name.clone(),
        LayoutStyle::new(),
        move || {
            // An armed row reads in the warning colour, so the state is visible and not only implied by the
            // caption underneath it.
            let colour = if armed_title() { theme.red } else { theme.text };
            TextStyle::new(theme.font(FontRole::Body), colour)
                .with_max_lines(1)
                .with_ellipsis(true)
        },
    )?;

    let armed_caption = is_armed.clone();
    let armed_caption_style = is_armed.clone();
    let mut lines: Vec<Box<dyn LayoutItem>> = vec![box_item(title)];
    if !description.is_empty() || dangerous {
        let subtitle = Text::auto(
            move || {
                if armed_caption() {
                    rsx::t!("launcher.confirm")
                } else {
                    description.clone()
                }
            },
            LayoutStyle::new(),
            move || {
                let colour = if armed_caption_style() {
                    theme.red
                } else {
                    theme.muted
                };
                TextStyle::new(theme.font(FontRole::Caption), colour)
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

    // A press is the pointer's Enter, so it follows the same arm-then-run rule; the keyboard path owns the
    // armed signal, so a dangerous row simply refuses the click and leaves arming to the keyboard.
    let chosen = Rc::new(entry);
    let armed_press = is_armed.clone();
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(10.0)
            .padding_horizontal(10.0)
            .padding_vertical(7.0)
            .width(SizeDimension::Percent(1.0)),
        move |_| {
            let fill = if is_armed() {
                theme.red.with_alpha(0.18)
            } else if is_selected() {
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
        if dangerous && !armed_press() {
            return;
        }
        choose(&chosen);
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
    fn favourites_are_pinned_above_the_ranking_in_the_order_listed() {
        let config = LauncherConfig {
            favourites: vec!["files".to_string(), "code".to_string()],
            ..LauncherConfig::default()
        };
        let ids: Vec<String> = results(catalog(), "", &config)
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(&ids[..2], &["files", "code"], "config order, not rank order");
        assert!(ids.len() > 2, "the rest still follow");
    }

    #[test]
    fn a_favourite_that_does_not_match_stays_out_of_the_results() {
        let config = LauncherConfig {
            favourites: vec!["code".to_string()],
            ..LauncherConfig::default()
        };
        let found = results(catalog(), "firefox", &config);
        assert!(
            !found.iter().any(|a| a.id == "code"),
            "pinning reorders the matches; it does not add one"
        );
        assert_eq!(found.first().map(|a| a.id.as_str()), Some("firefox"));
    }

    #[test]
    fn a_favourite_survives_the_result_cap() {
        // Ranked last but pinned first: the cap must not be what decides whether a favourite is shown.
        let config = LauncherConfig {
            favourites: vec!["files".to_string()],
            max_results: 1,
            ..LauncherConfig::default()
        };
        let found = results(catalog(), "", &config);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "files");
    }

    fn action(name: &str, dangerous: bool) -> LauncherAction {
        LauncherAction {
            name: name.to_string(),
            command: format!("run-{name}"),
            dangerous,
            ..LauncherAction::default()
        }
    }

    fn names(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::App(app) => app.id.clone(),
                Entry::Action(a) => a.name.clone(),
                Entry::Calculation { result, .. } => result.clone(),
            })
            .collect()
    }

    #[test]
    fn a_prefix_selects_the_mode_and_is_stripped() {
        assert_eq!(mode_of("firefox"), (QueryMode::Apps, "firefox"));
        assert_eq!(mode_of("> reboot"), (QueryMode::Actions, "reboot"));
        assert_eq!(mode_of(">reboot"), (QueryMode::Actions, "reboot"));
        assert_eq!(mode_of("=2+2"), (QueryMode::Calculator, "2+2"));
        assert_eq!(
            mode_of(">"),
            (QueryMode::Actions, ""),
            "a bare prefix lists everything in that mode"
        );
    }

    #[test]
    fn the_calculator_answers_above_the_apps_without_hiding_them() {
        let found = entries(catalog(), "2+2", &LauncherConfig::default());
        assert!(
            matches!(found.first(), Some(Entry::Calculation { result, .. }) if result == "4"),
            "the answer leads: {:?}",
            names(&found)
        );

        // A query that happens to parse as arithmetic must not hide the app search underneath it.
        let mixed = entries(catalog(), "2+2", &LauncherConfig::default());
        assert!(
            mixed.len() > 1 || mixed.iter().all(|e| matches!(e, Entry::Calculation { .. })),
            "app matches still follow when there are any"
        );

        // And a plain name never grows a calculation row.
        let plain = entries(catalog(), "firefox", &LauncherConfig::default());
        assert!(!plain.iter().any(|e| matches!(e, Entry::Calculation { .. })));
    }

    #[test]
    fn the_calc_prefix_forces_a_result_where_auto_detection_declines() {
        let config = LauncherConfig::default();
        // A bare number is treated as the start of a name, not a sum — unless asked explicitly.
        assert!(!entries(catalog(), "2", &config).iter().any(|e| matches!(e, Entry::Calculation { .. })));
        assert_eq!(names(&entries(catalog(), "=2", &config)), vec!["2".to_string()]);
    }

    #[test]
    fn the_calculator_can_be_switched_off_entirely() {
        let config = LauncherConfig {
            calculator: false,
            ..LauncherConfig::default()
        };
        assert!(!entries(catalog(), "2+2", &config).iter().any(|e| matches!(e, Entry::Calculation { .. })));
        assert!(entries(catalog(), "=2+2", &config).is_empty());
    }

    #[test]
    fn action_mode_lists_and_ranks_the_declared_actions() {
        let config = LauncherConfig {
            actions: vec![action("lock", false), action("logout", false)],
            ..LauncherConfig::default()
        };
        assert_eq!(names(&entries(catalog(), ">", &config)).len(), 2);
        assert_eq!(names(&entries(catalog(), ">lo", &config)).len(), 2);
        assert_eq!(
            names(&entries(catalog(), ">lock", &config)),
            vec!["lock".to_string()]
        );
        // Action mode is exclusive: apps do not leak into it.
        assert!(
            entries(catalog(), ">", &config)
                .iter()
                .all(|e| matches!(e, Entry::Action(_)))
        );
    }

    #[test]
    fn a_dangerous_action_is_hidden_until_it_is_allowed() {
        let mut config = LauncherConfig {
            actions: vec![action("lock", false), action("wipe", true)],
            ..LauncherConfig::default()
        };
        assert_eq!(
            names(&entries(catalog(), ">", &config)),
            vec!["lock".to_string()],
            "a dangerous action is not even listed without the opt-in"
        );

        config.enable_dangerous_actions = true;
        let listed = entries(catalog(), ">", &config);
        assert_eq!(names(&listed).len(), 2);
        assert!(
            listed.iter().any(|e| e.is_dangerous()),
            "and once allowed it still asks for a confirming Enter"
        );
    }

    #[test]
    fn an_incomplete_or_disabled_action_never_appears() {
        let config = LauncherConfig {
            actions: vec![
                LauncherAction {
                    name: "no command".to_string(),
                    ..LauncherAction::default()
                },
                LauncherAction {
                    command: "run".to_string(),
                    ..LauncherAction::default()
                },
                LauncherAction {
                    enabled: false,
                    ..action("parked", false)
                },
                action("good", false),
            ],
            ..LauncherConfig::default()
        };
        assert_eq!(names(&entries(catalog(), ">", &config)), vec!["good".to_string()]);
    }

    #[test]
    fn entry_keys_separate_the_kinds() {
        // The reactive list reconciles on these, so an app and an action sharing a name must not collide.
        let app = Entry::App(app("lock", "lock", &[]));
        let act = Entry::Action(action("lock", false));
        assert_ne!(app.key(), act.key());
        assert!(!app.is_dangerous());
        assert!(Entry::Action(action("wipe", true)).is_dangerous());
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
