//! Moving a selection through a list with the keyboard, the same way on every surface that has one.
//!
//! The launcher had this logic inline and nothing else had it at all, so the network AP list, the bluetooth device
//! list and the session tiles were all pointer-only. One primitive rather than five copies: a list is a list, and a
//! user who learns that `j` moves down in the launcher is owed the same answer everywhere.
//!
//! It is deliberately *not* rsx's focus system. That answers "which widget receives keys" — one focusable per
//! widget, driven by Tab. This answers "which row of a list is selected", which is one focusable holding a cursor
//! over N rows, and the two compose: the launcher's search field keeps focus while these keys drive the list
//! underneath it.

use rsx::{Key, NamedKey};

use crate::core::config::KeyNavConfig;

/// What a key press means to a list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    Next,
    Previous,
    First,
    Last,
    /// Run the selected row.
    Activate,
    /// Back out: dismiss the surface, or undo an armed confirmation.
    Cancel,
}

/// How a list reads keys: the arrows always, and optionally the vim bindings on top.
///
/// `vim` is off by default because a list that swallows `j` cannot also be typed into, and hyprshell's biggest
/// list — the launcher — is a search field. A surface with no text input can turn it on freely; one with a
/// field should only do so if its user asked for it.
#[derive(Clone, Copy)]
pub struct KeyNav {
    pub vim: bool,
    /// The list runs along the screen's horizontal, so Left/Right move it rather than Up/Down.
    pub horizontal: bool,
}

impl KeyNav {
    /// A vertical list reading the shell's configured bindings.
    pub fn from_config(config: &KeyNavConfig) -> Self {
        Self {
            vim: config.vim,
            horizontal: false,
        }
    }

    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    /// What `key` asks the list to do, or `None` when it is not a navigation key — which is the important half
    /// of the contract: everything this returns `None` for must still reach a focused text field as typing.
    pub fn interpret(self, key: &Key) -> Option<Move> {
        let (forward, back) = if self.horizontal {
            (NamedKey::ArrowRight, NamedKey::ArrowLeft)
        } else {
            (NamedKey::ArrowDown, NamedKey::ArrowUp)
        };
        if let Key::Named(named) = key {
            if *named == forward {
                return Some(Move::Next);
            }
            if *named == back {
                return Some(Move::Previous);
            }
            return match named {
                NamedKey::Enter => Some(Move::Activate),
                NamedKey::Escape => Some(Move::Cancel),
                NamedKey::Home => Some(Move::First),
                NamedKey::End => Some(Move::Last),
                _ => None,
            };
        }
        if !self.vim {
            return None;
        }
        // Vim's own pairs, and the readline pair every terminal user already has in their fingers. `G` before `g` because the shift-key distinction is the whole difference between them.
        match key {
            Key::Char(c) => match c {
                'j' => Some(Move::Next),
                'k' => Some(Move::Previous),
                'g' => Some(Move::First),
                'G' => Some(Move::Last),
                '\u{e}' => Some(Move::Next),      // Ctrl-N
                '\u{10}' => Some(Move::Previous), // Ctrl-P
                _ => None,
            },
            _ => None,
        }
    }
}

/// Where a move lands, given the current index and how many rows there are.
///
/// Wraps at both ends: a list short enough to see all of is faster to reach the bottom of by pressing up once,
/// and a list too long to see wraps rather than sticking silently, which reads as the key not working.
pub fn apply(current: usize, count: usize, movement: Move) -> usize {
    if count == 0 {
        return 0;
    }
    let current = current.min(count - 1);
    match movement {
        Move::Next => (current + 1) % count,
        Move::Previous => (current + count - 1) % count,
        Move::First => 0,
        Move::Last => count - 1,
        Move::Activate | Move::Cancel => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrows() -> KeyNav {
        KeyNav {
            vim: false,
            horizontal: false,
        }
    }

    fn vim() -> KeyNav {
        KeyNav {
            vim: true,
            horizontal: false,
        }
    }

    fn named(key: NamedKey) -> Key {
        Key::Named(key)
    }

    fn character(c: char) -> Key {
        Key::Char(c)
    }

    #[test]
    fn the_arrows_always_navigate_and_the_letters_only_do_in_vim_mode() {
        assert_eq!(arrows().interpret(&named(NamedKey::ArrowDown)), Some(Move::Next));
        assert_eq!(arrows().interpret(&named(NamedKey::ArrowUp)), Some(Move::Previous));
        assert_eq!(arrows().interpret(&named(NamedKey::Enter)), Some(Move::Activate));
        assert_eq!(arrows().interpret(&named(NamedKey::Escape)), Some(Move::Cancel));

        // The half that matters: with vim off, a letter is typing and must reach the search field.
        assert_eq!(arrows().interpret(&character('j')), None);
        assert_eq!(arrows().interpret(&character('k')), None);
        assert_eq!(arrows().interpret(&character('G')), None);

        assert_eq!(vim().interpret(&character('j')), Some(Move::Next));
        assert_eq!(vim().interpret(&character('k')), Some(Move::Previous));
        assert_eq!(vim().interpret(&character('g')), Some(Move::First));
        assert_eq!(vim().interpret(&character('G')), Some(Move::Last));
        assert_eq!(vim().interpret(&character('\u{e}')), Some(Move::Next), "Ctrl-N");
        assert_eq!(vim().interpret(&character('\u{10}')), Some(Move::Previous), "Ctrl-P");
        assert_eq!(vim().interpret(&character('q')), None, "an unbound letter is still typing");
    }

    #[test]
    fn a_horizontal_list_reads_the_other_pair_of_arrows() {
        let row = arrows().horizontal();
        assert_eq!(row.interpret(&named(NamedKey::ArrowRight)), Some(Move::Next));
        assert_eq!(row.interpret(&named(NamedKey::ArrowLeft)), Some(Move::Previous));
        assert_eq!(
            row.interpret(&named(NamedKey::ArrowDown)),
            None,
            "down is not along a row, so it stays available to whatever else wants it"
        );
    }

    #[test]
    fn the_selection_wraps_at_both_ends_and_survives_a_list_that_shrank() {
        assert_eq!(apply(0, 3, Move::Next), 1);
        assert_eq!(apply(2, 3, Move::Next), 0, "wraps past the end");
        assert_eq!(apply(0, 3, Move::Previous), 2, "and back past the start");
        assert_eq!(apply(1, 3, Move::First), 0);
        assert_eq!(apply(1, 3, Move::Last), 2);
        assert_eq!(apply(1, 3, Move::Activate), 1, "activating moves nothing");

        assert_eq!(apply(0, 0, Move::Next), 0, "an empty list has nowhere to go");
        // A selection left over from a longer list is clamped rather than wrapping off a stale index — the launcher's results shrink on every keystroke.
        assert_eq!(apply(9, 3, Move::Next), 0);
        assert_eq!(apply(9, 3, Move::Previous), 1);
    }
}
