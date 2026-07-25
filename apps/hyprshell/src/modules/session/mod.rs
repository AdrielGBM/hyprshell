//! The power chip and the menu it opens.

use rsx::{
    AlignItems, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    SizeDimension, StyledContainer, Text, TextStyle, box_item, signal, use_theme,
};

use crate::shared::services::session::{self, Action};
use crate::shared::theme::{FontRole, NordTheme};

/// The bar chip: a power symbol that opens the session menu.
pub fn power_chip() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let fg = crate::module_fg();
    crate::icon_view(|| "power".to_string(), move || fg.get(), crate::icon_px())
}

/// The session menu: one tile per action this machine can actually perform.
///
/// Destructive actions confirm first — the tile arms, and only a second press on the armed tile goes through.
/// A single mis-click on a panel that opens next to the clock should not end the session, and arming in place
/// costs no extra surface and no extra keystroke for someone who meant it.
pub fn session_panel() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let armed = signal(String::new());

    let title = Text::auto(
        || rsx::t!("session.title"),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Title), theme.text).with_weight(700),
    )?;

    let mut tiles: Vec<Box<dyn LayoutItem>> = Vec::new();
    for action in session::available() {
        tiles.push(tile(action, armed.clone(), theme)?);
    }

    let grid = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(8.0)
            .justify_content(JustifyContent::CENTER)
            .width(SizeDimension::Percent(1.0)),
        tiles,
    )?;

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(12.0)
            .width(SizeDimension::Percent(1.0)),
        vec![box_item(title), box_item(grid)],
    )?))
}

/// Whether pressing `action` ends work the user might not have saved. Suspend and lock are recoverable in a
/// keystroke; the rest are not, so only the rest arm before they fire.
fn is_destructive(action: Action) -> bool {
    !matches!(action, Action::Lock | Action::Suspend)
}

fn label_for(action: Action) -> String {
    match action {
        Action::Lock => rsx::t!("session.lock"),
        Action::Logout => rsx::t!("session.logout"),
        Action::Suspend => rsx::t!("session.suspend"),
        Action::Hibernate => rsx::t!("session.hibernate"),
        Action::Reboot => rsx::t!("session.reboot"),
        Action::Shutdown => rsx::t!("session.shutdown"),
    }
}

fn tile(
    action: Action,
    armed: rsx::RwSignal<String>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id = action.id();
    // One handle per closure: a signal is not `Copy`, and each of the four readers below outlives the others.
    let armed_icon = armed.read_only();
    let armed_caption = armed.read_only();
    let armed_fill = armed.read_only();
    let armed_hover = armed.read_only();

    let icon = crate::icon_view(
        move || action.icon().to_string(),
        move || {
            if armed_icon.get() == id {
                theme.red
            } else {
                theme.text
            }
        },
        24.0,
    )?;

    let caption = Text::auto(
        move || {
            if armed_caption.get() == id {
                rsx::t!("session.confirm")
            } else {
                label_for(action)
            }
        },
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Caption), theme.text),
    )?;

    let tile = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .gap(6.0)
            .width(88.0)
            .padding_vertical(12.0),
        move |_| {
            let fill = if armed_fill.get() == id {
                theme.red
            } else {
                theme.base
            };
            RectStyle::filled(fill, 10.0)
        },
        vec![icon, box_item(caption)],
    )?
    .on_hover_style(move |_| {
        let fill = if armed_hover.get() == id {
            theme.red
        } else {
            theme.overlay
        };
        RectStyle::filled(fill, 10.0)
    })
    .on_press(move || {
        if !is_destructive(action) || armed.peek() == id {
            session::perform(action);
            crate::close_panel("session");
            return;
        }
        // Arming one tile disarms any other, so two half-pressed tiles can never both be live.
        armed.set(id.to_string());
    });
    Ok(Box::new(tile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unrecoverable_actions_ask_twice() {
        assert!(!is_destructive(Action::Lock), "a lock is undone by typing");
        assert!(!is_destructive(Action::Suspend), "a suspend is undone by a key");
        for action in [Action::Logout, Action::Reboot, Action::Shutdown, Action::Hibernate] {
            assert!(
                is_destructive(action),
                "'{}' can lose unsaved work, so it must confirm",
                action.id()
            );
        }
    }
}
