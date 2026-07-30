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
    /// One row down in a grid — a whole column count, not one tile. Same as [`Move::Next`] in a single-column
    /// list, which is what lets one `apply` serve both.
    NextRow,
    PreviousRow,
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
    /// The list wraps into rows, so it uses *both* pairs of arrows: Left/Right for one tile and Up/Down for a
    /// whole row. Only a grid can, which is why it is a mode rather than the default.
    pub grid: bool,
}

impl KeyNav {
    /// A vertical list reading the shell's configured bindings.
    pub fn from_config(config: &KeyNavConfig) -> Self {
        Self {
            vim: config.vim,
            horizontal: false,
            grid: false,
        }
    }

    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    /// A grid: tiles run along a row, so Left/Right step one and Up/Down step a row.
    pub fn grid(mut self) -> Self {
        self.horizontal = true;
        self.grid = true;
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
            if self.grid {
                if *named == NamedKey::ArrowDown {
                    return Some(Move::NextRow);
                }
                if *named == NamedKey::ArrowUp {
                    return Some(Move::PreviousRow);
                }
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
        let (down, up) = if self.grid {
            (Move::NextRow, Move::PreviousRow)
        } else {
            (Move::Next, Move::Previous)
        };
        match key {
            Key::Char(c) => match c {
                'j' => Some(down),
                'k' => Some(up),
                'h' if self.grid => Some(Move::Previous),
                'l' if self.grid => Some(Move::Next),
                'g' => Some(Move::First),
                'G' => Some(Move::Last),
                '\u{e}' => Some(down), // Ctrl-N
                '\u{10}' => Some(up),  // Ctrl-P
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
    apply_grid(current, count, 1, movement)
}

/// Where a move lands in a grid `columns` tiles wide. A single column is a list, which is why [`apply`] is this
/// with `columns = 1` rather than a second implementation.
///
/// A row move off the bottom lands on the nearest tile below where there is one — a partial last row is still a
/// row — and wraps to the same column otherwise, matching the horizontal rule.
pub fn apply_grid(current: usize, count: usize, columns: usize, movement: Move) -> usize {
    if count == 0 {
        return 0;
    }
    let columns = columns.max(1);
    let current = current.min(count - 1);
    let last = count - 1;
    match movement {
        Move::Next => (current + 1) % count,
        Move::Previous => (current + count - 1) % count,
        Move::First => 0,
        Move::Last => last,
        Move::NextRow => {
            let below = current + columns;
            if below <= last {
                below
            } else if current / columns < last / columns {
                // A shorter last row: down from the end of a full row still goes down, to its final tile.
                last
            } else {
                current % columns
            }
        }
        Move::PreviousRow => {
            if current >= columns {
                current - columns
            } else {
                let bottom = (last / columns) * columns + current % columns;
                if bottom > last { bottom - columns } else { bottom }
            }
        }
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
            grid: false,
        }
    }

    fn vim() -> KeyNav {
        KeyNav {
            vim: true,
            horizontal: false,
            grid: false,
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

        // A list is a one-column grid, so a row move is a step: the launcher's rows and its wallpaper grid share
        // one `apply` and must not need to know which they are.
        assert_eq!(apply(0, 3, Move::NextRow), 1);
        assert_eq!(apply(2, 3, Move::PreviousRow), 1);
        assert_eq!(apply(2, 3, Move::NextRow), 0, "still wraps");
    }

    #[test]
    fn a_grid_uses_both_pairs_of_arrows() {
        let grid = arrows().grid();
        assert_eq!(grid.interpret(&named(NamedKey::ArrowRight)), Some(Move::Next));
        assert_eq!(grid.interpret(&named(NamedKey::ArrowLeft)), Some(Move::Previous));
        assert_eq!(grid.interpret(&named(NamedKey::ArrowDown)), Some(Move::NextRow));
        assert_eq!(grid.interpret(&named(NamedKey::ArrowUp)), Some(Move::PreviousRow));
        assert_eq!(grid.interpret(&named(NamedKey::Enter)), Some(Move::Activate));

        // With vim off — the launcher's default, since the grid sits under a search field — a letter is typing.
        assert_eq!(grid.interpret(&character('j')), None);
        assert_eq!(grid.interpret(&character('h')), None);

        let keys = vim().grid();
        assert_eq!(keys.interpret(&character('j')), Some(Move::NextRow));
        assert_eq!(keys.interpret(&character('k')), Some(Move::PreviousRow));
        assert_eq!(keys.interpret(&character('l')), Some(Move::Next));
        assert_eq!(keys.interpret(&character('h')), Some(Move::Previous));
    }

    /// Nine tiles in three columns, plus the awkward case a wallpaper folder always ends in: a partial last row.
    #[test]
    fn a_row_move_crosses_a_whole_row_and_a_partial_one_is_still_a_row() {
        // 0 1 2
        // 3 4 5
        // 6 7 8
        assert_eq!(apply_grid(0, 9, 3, Move::NextRow), 3);
        assert_eq!(apply_grid(4, 9, 3, Move::PreviousRow), 1);
        assert_eq!(apply_grid(1, 9, 3, Move::Next), 2, "along the row, not down it");
        assert_eq!(apply_grid(7, 9, 3, Move::NextRow), 1, "wraps to the same column");
        assert_eq!(apply_grid(1, 9, 3, Move::PreviousRow), 7, "and back to the bottom of it");

        // 0 1 2
        // 3 4
        // Down from 2 has no tile of its own below, but there *is* a row: landing on 4 beats not moving.
        assert_eq!(apply_grid(2, 5, 3, Move::NextRow), 4);
        assert_eq!(apply_grid(1, 5, 3, Move::NextRow), 4);
        assert_eq!(apply_grid(0, 5, 3, Move::NextRow), 3);
        // Up from the top column 2 finds the bottom-most tile in that column, which is on the row above the gap.
        assert_eq!(apply_grid(2, 5, 3, Move::PreviousRow), 2);
        assert_eq!(apply_grid(1, 5, 3, Move::PreviousRow), 4);
        assert_eq!(apply_grid(4, 5, 3, Move::NextRow), 1, "the last row wraps to the first");

        // One row: a row move has nowhere to go and must stay put rather than run off the end.
        assert_eq!(apply_grid(1, 3, 3, Move::NextRow), 1);
        assert_eq!(apply_grid(1, 3, 3, Move::PreviousRow), 1);
        assert_eq!(apply_grid(0, 0, 4, Move::NextRow), 0);
        assert_eq!(apply_grid(9, 5, 3, Move::NextRow), 1, "a stale index is clamped first");
    }
}
