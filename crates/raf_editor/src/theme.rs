//! Renderer-neutral editor theme helpers.
//!
//! Theme values are data consumed by RafUI/AGB.  This module intentionally
//! does not expose widget-framework colors or painting types.

use raf_core::config::Theme;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiTokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub background: [u8; 4],
    pub surface: [u8; 4],
    pub surface_alt: [u8; 4],
    pub text: [u8; 4],
    pub text_muted: [u8; 4],
    pub accent: [u8; 4],
    pub border: [u8; 4],
}

impl ThemePalette {
    pub fn industrial_dark() -> Self {
        Self::from_tokens(StudioUiPalette::IndustrialDark.tokens())
    }

    pub fn for_theme(theme: Theme) -> Self {
        let palette = match theme {
            Theme::Light => StudioUiPalette::PaperLight,
            // System follows the existing dark-first native shell until the
            // platform color-mode bridge is available.
            Theme::Dark | Theme::System => StudioUiPalette::IndustrialDark,
        };
        Self::from_tokens(palette.tokens())
    }

    pub fn from_tokens(tokens: UiTokens) -> Self {
        Self {
            background: tokens.background,
            surface: tokens.surface,
            surface_alt: tokens.surface_alt,
            text: tokens.text,
            text_muted: tokens.text_muted,
            accent: tokens.accent,
            border: tokens.border,
        }
    }
}
