use std::any::Any;

use rsx::{Color, Theme, ThemeTokens};

/// Semantic text sizes, each a step off the theme's base [`font_size`](NordTheme::font_size).
#[derive(Clone, Copy)]
pub enum FontRole {
    /// Small secondary text: badges, pills, chip labels, notification bodies.
    Caption,
    /// Default body text.
    Body,
    /// Section/panel headers.
    Title,
    /// Large display text, e.g. the clock face.
    Display,
}

#[derive(Clone, Copy)]
pub struct NordTheme {
    /// Base corner radius the theme rounds panels and bars to (the design default; `[shape]`/per-bar can override).
    pub radius: f32,
    /// Base gap between adjacent modules inside a bar/section (the design default; `[shape]`/per-bar can override).
    pub spacing: f32,
    /// Base (body) font size in px. Every other text size steps off this via [`font`](Self::font), so scaling it scales all text.
    pub font_size: f32,
    /// Default size for a standalone icon (px), e.g. the OSD glyph. Bar chips derive their icon size from the bar thickness instead, so they scale with the bar.
    pub icon_size: f32,
    pub base: Color,
    pub surface: Color,
    pub overlay: Color,
    pub muted: Color,
    pub subtle: Color,
    pub text: Color,
    pub accent: Color,
    pub blue: Color,
    pub cyan: Color,
    pub teal: Color,
    pub red: Color,
    pub orange: Color,
    pub yellow: Color,
    pub green: Color,
    pub purple: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub highlight_low: Color,
    pub highlight_med: Color,
    pub highlight_high: Color,
}

/// A theme's descriptive metadata (§9), so a theme can present itself (in a picker, logs, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeMeta {
    pub name: &'static str,
    pub author: &'static str,
    pub description: &'static str,
    pub version: &'static str,
}

impl NordTheme {
    /// The built-in palette for `name` (`nord`, `rose-pine`); `custom` starts from nord for config to override, and an unknown name falls back to nord with a warning.
    pub fn named(name: &str) -> Self {
        match name {
            "nord" | "custom" => Self::nord(),
            "rose-pine" | "rose_pine" | "rosepine" => Self::rose_pine(),
            other => {
                tracing::warn!("unknown theme '{other}', falling back to nord");
                Self::nord()
            }
        }
    }

    /// Metadata for a built-in theme name (falls back to nord's).
    pub fn meta(name: &str) -> ThemeMeta {
        match name {
            "rose-pine" | "rose_pine" | "rosepine" => ThemeMeta {
                name: "Rosé Pine",
                author: "Rosé Pine",
                description: "Soho vibes — a warm, low-contrast dark theme.",
                version: "1.0.0",
            },
            _ => ThemeMeta {
                name: "Nord",
                author: "Arctic Ice Studio",
                description: "An arctic, north-bluish palette.",
                version: "1.0.0",
            },
        }
    }

    /// The Nord palette (the default built-in). [`new`](Self::new) is kept as its alias.
    pub fn nord() -> Self {
        Self::new()
    }

    /// The Rosé Pine palette — its own colours and a slightly rounder, airier metric set, to show a theme carries all its design tokens.
    pub fn rose_pine() -> Self {
        Self {
            radius: 10.0,
            spacing: 8.0,
            font_size: 14.0,
            icon_size: 24.0,
            base: Color::from_rgb_u8(25, 23, 36),
            surface: Color::from_rgb_u8(31, 29, 46),
            overlay: Color::from_rgb_u8(38, 35, 58),
            muted: Color::from_rgb_u8(110, 106, 134),
            subtle: Color::from_rgb_u8(144, 140, 170),
            text: Color::from_rgb_u8(224, 222, 244),
            accent: Color::from_rgb_u8(156, 207, 216),
            blue: Color::from_rgb_u8(49, 116, 143),
            cyan: Color::from_rgb_u8(156, 207, 216),
            teal: Color::from_rgb_u8(49, 116, 143),
            red: Color::from_rgb_u8(235, 111, 146),
            orange: Color::from_rgb_u8(246, 193, 119),
            yellow: Color::from_rgb_u8(246, 193, 119),
            green: Color::from_rgb_u8(156, 207, 216),
            purple: Color::from_rgb_u8(196, 167, 231),
            success: Color::from_rgb_u8(49, 116, 143),
            warning: Color::from_rgb_u8(246, 193, 119),
            error: Color::from_rgb_u8(235, 111, 146),
            info: Color::from_rgb_u8(49, 116, 143),
            highlight_low: Color::from_rgb_u8(33, 32, 46),
            highlight_med: Color::from_rgb_u8(64, 61, 82),
            highlight_high: Color::from_rgb_u8(82, 79, 103),
        }
    }

    pub fn new() -> Self {
        Self {
            // Match today's defaults so no config changes look; a theme is free to round more / space wider.
            radius: 0.0,
            spacing: 6.0,
            font_size: 14.0,
            icon_size: 24.0,
            base: Color::from_rgb_u8(46, 52, 64),
            surface: Color::from_rgb_u8(59, 66, 82),
            overlay: Color::from_rgb_u8(67, 76, 94),
            muted: Color::from_rgb_u8(76, 86, 106),
            subtle: Color::from_rgb_u8(216, 222, 233),
            text: Color::from_rgb_u8(236, 239, 244),
            accent: Color::from_rgb_u8(136, 192, 208),
            blue: Color::from_rgb_u8(94, 129, 172),
            cyan: Color::from_rgb_u8(136, 192, 208),
            teal: Color::from_rgb_u8(143, 188, 187),
            red: Color::from_rgb_u8(191, 97, 106),
            orange: Color::from_rgb_u8(208, 135, 112),
            yellow: Color::from_rgb_u8(235, 203, 139),
            green: Color::from_rgb_u8(163, 190, 140),
            purple: Color::from_rgb_u8(180, 142, 173),
            success: Color::from_rgb_u8(163, 190, 140),
            warning: Color::from_rgb_u8(235, 203, 139),
            error: Color::from_rgb_u8(191, 97, 106),
            info: Color::from_rgb_u8(94, 129, 172),
            highlight_low: Color::from_rgb_u8(67, 76, 94),
            highlight_med: Color::from_rgb_u8(76, 86, 106),
            highlight_high: Color::from_rgb_u8(94, 105, 128),
        }
    }

    /// Applies the configured accent to the theme's `accent` field, so everything reading `use_theme().accent` follows `[theme] accent` uniformly, not just the modules the bar resolves per-id.
    pub fn with_accent(mut self, name: &str) -> Self {
        self.accent = self.accent_by_name(name);
        self
    }

    /// A text size by semantic role, stepping off [`font_size`](Self::font_size) so a theme scales its whole type ramp from one number.
    pub fn font(&self, role: FontRole) -> f32 {
        match role {
            FontRole::Caption => self.font_size - 2.0,
            FontRole::Body => self.font_size,
            FontRole::Title => self.font_size + 1.0,
            FontRole::Display => (self.font_size * 2.4).round(),
        }
    }

    /// Overrides one palette token by name (as used in `[theme.colors]`), for config-defined custom colors; an unknown name is ignored with a warning.
    pub fn with_color(mut self, name: &str, color: Color) -> Self {
        match name {
            "base" => self.base = color,
            "surface" => self.surface = color,
            "overlay" => self.overlay = color,
            "muted" => self.muted = color,
            "subtle" => self.subtle = color,
            "text" => self.text = color,
            "accent" => self.accent = color,
            "blue" => self.blue = color,
            "cyan" => self.cyan = color,
            "teal" => self.teal = color,
            "red" => self.red = color,
            "orange" => self.orange = color,
            "yellow" => self.yellow = color,
            "green" => self.green = color,
            "purple" => self.purple = color,
            "success" => self.success = color,
            "warning" => self.warning = color,
            "error" => self.error = color,
            "info" => self.info = color,
            "highlight_low" => self.highlight_low = color,
            "highlight_med" => self.highlight_med = color,
            "highlight_high" => self.highlight_high = color,
            other => tracing::warn!("unknown theme color token '{other}'"),
        }
        self
    }

    pub fn accent_by_name(&self, name: &str) -> Color {
        match name {
            "blue" => self.blue,
            "cyan" => self.cyan,
            "teal" => self.teal,
            "red" => self.red,
            "orange" => self.orange,
            "yellow" => self.yellow,
            "green" => self.green,
            "purple" => self.purple,
            _ => self.accent,
        }
    }
}

impl Default for NordTheme {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_selects_a_palette_and_falls_back_to_nord() {
        assert_eq!(NordTheme::named("nord").base, NordTheme::nord().base);
        assert_eq!(NordTheme::named("rose-pine").base, NordTheme::rose_pine().base);
        // Rosé Pine is a different palette with its own metrics.
        assert_ne!(NordTheme::rose_pine().base, NordTheme::nord().base);
        assert_eq!(NordTheme::rose_pine().radius, 10.0);
        // "custom" and unknown names both fall back to nord (custom for config to override).
        assert_eq!(NordTheme::named("custom").base, NordTheme::nord().base);
        assert_eq!(NordTheme::named("does-not-exist").base, NordTheme::nord().base);
        assert_eq!(NordTheme::meta("rose-pine").name, "Rosé Pine");
        assert_eq!(NordTheme::meta("whatever").name, "Nord");
    }

    #[test]
    fn font_scale_steps_off_the_base_size() {
        let t = NordTheme::new();
        assert_eq!(t.font(FontRole::Body), t.font_size);
        assert_eq!(t.font(FontRole::Caption), t.font_size - 2.0);
        assert_eq!(t.font(FontRole::Title), t.font_size + 1.0);
        assert_eq!(t.font(FontRole::Display), (t.font_size * 2.4).round());
        // Scaling the base scales the whole ramp.
        let big = NordTheme {
            font_size: 20.0,
            ..NordTheme::new()
        };
        assert_eq!(big.font(FontRole::Body), 20.0);
        assert_eq!(big.font(FontRole::Display), 48.0);
    }

    #[test]
    fn semantic_tokens_map_to_nord_palette() {
        let t = NordTheme::new();
        assert_eq!(t.success, t.green);
        assert_eq!(t.warning, t.yellow);
        assert_eq!(t.error, t.red);
        assert_eq!(t.info, t.blue);
        // The catalogue reads them through the ThemeTokens contract.
        assert_eq!(ThemeTokens::success(&t), t.green);
        assert_eq!(ThemeTokens::error(&t), t.red);
        assert_eq!(ThemeTokens::info(&t), t.blue);
        assert_ne!(t.highlight_low, t.highlight_med);
        assert_ne!(t.highlight_med, t.highlight_high);
    }
}

impl Theme for NordTheme {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ThemeTokens for NordTheme {
    fn primary(&self) -> Color {
        self.accent
    }
    fn on_primary(&self) -> Color {
        self.base
    }
    fn muted(&self) -> Color {
        self.muted
    }
    fn scrollbar(&self) -> Color {
        self.muted
    }
    fn ink(&self) -> Color {
        self.text
    }
    fn surface_alt(&self) -> Color {
        self.surface
    }
    fn border(&self) -> Color {
        self.muted
    }
    fn success(&self) -> Color {
        self.success
    }
    fn warning(&self) -> Color {
        self.warning
    }
    fn error(&self) -> Color {
        self.error
    }
    fn info(&self) -> Color {
        self.info
    }
    fn highlight_low(&self) -> Color {
        self.highlight_low
    }
    fn highlight_med(&self) -> Color {
        self.highlight_med
    }
    fn highlight_high(&self) -> Color {
        self.highlight_high
    }
}
