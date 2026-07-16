use std::any::Any;

use rsx::{Color, Theme, ThemeTokens};

#[derive(Clone, Copy)]
pub struct NordTheme {
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

impl NordTheme {
    pub fn new() -> Self {
        Self {
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
