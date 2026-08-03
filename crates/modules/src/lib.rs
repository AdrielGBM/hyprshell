telar::rsx_modules!(::config::theme::NordTheme);

// Components borrowed from `ui`: `[telar] components` in telar.toml gives the transpiler their signatures, this gives the generated code their symbols, which it reaches through its own `use super::*`.
pub use ::ui::{ChipLabelProps, IconGlyphProps, chip_label, icon_glyph};
