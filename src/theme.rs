use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tracing::warn;
use tuirealm::ratatui::style::Color;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum ThemePreset {
    #[default]
    Default,
    Light,
    HighContrast,
    Mono,
    Custom,
}

impl ThemePreset {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Light,
        Self::HighContrast,
        Self::Mono,
        Self::Custom,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Light => "light",
            Self::HighContrast => "high-contrast",
            Self::Mono => "mono",
            Self::Custom => "custom",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Default => "Balanced colors for everyday use",
            Self::Light => "Bright background with dark text",
            Self::HighContrast => "Enhanced visibility, bright on dark",
            Self::Mono => "Minimal monochrome aesthetic",
            Self::Custom => "User-defined semantic palette",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Default => Self::Light,
            Self::Light => Self::HighContrast,
            Self::HighContrast => Self::Mono,
            Self::Mono => Self::Custom,
            Self::Custom => Self::Default,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Default => Self::Custom,
            Self::Light => Self::Default,
            Self::HighContrast => Self::Light,
            Self::Mono => Self::HighContrast,
            Self::Custom => Self::Mono,
        }
    }
}

impl FromStr for ThemePreset {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "light" | "day" => Ok(Self::Light),
            "high-contrast" | "high_contrast" | "contrast" => Ok(Self::HighContrast),
            "mono" | "monochrome" => Ok(Self::Mono),
            "custom" => Ok(Self::Custom),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub base: BasePalette,
    pub interactive: InteractivePalette,
    pub status: StatusPalette,
    pub tile: TilePalette,
    pub category: CategoryAccentPalette,
    pub dialog: DialogPalette,
}

#[derive(Debug, Clone, Copy)]
pub struct BasePalette {
    pub canvas: Color,
    pub surface: Color,
    pub text: Color,
    pub text_muted: Color,
    pub header: Color,
    pub accent: Color,
    pub danger: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct InteractivePalette {
    pub focus: Color,
    pub selected_bg: Color,
    pub selected_border: Color,
    pub border: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct StatusPalette {
    pub running: Color,
    pub waiting: Color,
    pub idle: Color,
    pub dead: Color,
    pub broken: Color,
    pub unavailable: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct TilePalette {
    pub repo: Color,
    pub branch: Color,
    pub todo: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct DialogPalette {
    pub surface: Color,
    pub input_bg: Color,
    pub button_bg: Color,
    pub button_fg: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct CategoryAccentPalette {
    pub primary: Color,
    pub secondary: Color,
    pub tertiary: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomThemeConfig {
    #[serde(default = "default_custom_theme_inherit")]
    pub inherit: String,
    pub base: BasePaletteOverride,
    pub interactive: InteractivePaletteOverride,
    pub status: StatusPaletteOverride,
    pub tile: TilePaletteOverride,
    pub category: CategoryAccentPaletteOverride,
    pub dialog: DialogPaletteOverride,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BasePaletteOverride {
    pub canvas: Option<String>,
    pub surface: Option<String>,
    pub text: Option<String>,
    pub text_muted: Option<String>,
    pub header: Option<String>,
    pub accent: Option<String>,
    pub danger: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct InteractivePaletteOverride {
    pub focus: Option<String>,
    pub selected_bg: Option<String>,
    pub selected_border: Option<String>,
    pub border: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StatusPaletteOverride {
    pub running: Option<String>,
    pub waiting: Option<String>,
    pub idle: Option<String>,
    pub dead: Option<String>,
    pub broken: Option<String>,
    pub unavailable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TilePaletteOverride {
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub todo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CategoryAccentPaletteOverride {
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub tertiary: Option<String>,
    pub success: Option<String>,
    pub warning: Option<String>,
    pub danger: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DialogPaletteOverride {
    pub surface: Option<String>,
    pub input_bg: Option<String>,
    pub button_bg: Option<String>,
    pub button_fg: Option<String>,
}

impl Default for CustomThemeConfig {
    fn default() -> Self {
        Self {
            inherit: default_custom_theme_inherit(),
            base: BasePaletteOverride::default(),
            interactive: InteractivePaletteOverride::default(),
            status: StatusPaletteOverride::default(),
            tile: TilePaletteOverride::default(),
            category: CategoryAccentPaletteOverride::default(),
            dialog: DialogPaletteOverride::default(),
        }
    }
}

fn default_custom_theme_inherit() -> String {
    ThemePreset::Default.as_str().to_string()
}

#[derive(Debug, Clone, Copy)]
pub struct TileStateColors {
    pub background: Color,
    pub border: Color,
}

impl Theme {
    pub fn resolve(preset: ThemePreset, custom_theme: &CustomThemeConfig) -> Self {
        if preset == ThemePreset::Custom {
            Self::from_custom(custom_theme)
        } else {
            Self::from_preset(preset)
        }
    }

    fn from_custom(custom_theme: &CustomThemeConfig) -> Self {
        let inherit = custom_theme.inherit_preset();
        let mut theme = Self::from_preset(inherit);
        custom_theme.apply_overrides(&mut theme);
        theme
    }

    pub fn from_preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::Default => Self {
                base: BasePalette {
                    canvas: Color::Rgb(36, 40, 56),
                    surface: Color::Rgb(36, 40, 56),
                    text: Color::White,
                    text_muted: Color::DarkGray,
                    header: Color::Cyan,
                    accent: Color::Magenta,
                    danger: Color::Red,
                },
                interactive: InteractivePalette {
                    focus: Color::Cyan,
                    selected_bg: Color::Rgb(54, 48, 72),
                    selected_border: Color::Rgb(255, 187, 120),
                    border: Color::DarkGray,
                },
                status: StatusPalette {
                    running: Color::LightGreen,
                    waiting: Color::Yellow,
                    idle: Color::Gray,
                    dead: Color::Red,
                    broken: Color::LightRed,
                    unavailable: Color::Red,
                },
                tile: TilePalette {
                    repo: Color::LightCyan,
                    branch: Color::LightYellow,
                    todo: Color::DarkGray,
                },
                category: CategoryAccentPalette {
                    primary: Color::Cyan,
                    secondary: Color::Magenta,
                    tertiary: Color::Blue,
                    success: Color::Green,
                    warning: Color::Yellow,
                    danger: Color::Red,
                },
                dialog: DialogPalette {
                    surface: Color::Rgb(36, 40, 56),
                    input_bg: Color::Rgb(36, 40, 56),
                    button_bg: Color::Black,
                    button_fg: Color::Black,
                },
            },
            ThemePreset::Light => Self {
                base: BasePalette {
                    canvas: Color::Rgb(226, 231, 238),
                    surface: Color::Rgb(236, 241, 247),
                    text: Color::Rgb(34, 42, 58),
                    text_muted: Color::Rgb(78, 89, 109),
                    header: Color::Rgb(47, 102, 191),
                    accent: Color::Rgb(14, 116, 144),
                    danger: Color::Rgb(176, 46, 36),
                },
                interactive: InteractivePalette {
                    focus: Color::Rgb(47, 102, 191),
                    selected_bg: Color::Rgb(214, 223, 237),
                    selected_border: Color::Rgb(71, 122, 205),
                    border: Color::Rgb(165, 178, 198),
                },
                status: StatusPalette {
                    running: Color::Rgb(39, 132, 73),
                    waiting: Color::Rgb(171, 120, 26),
                    idle: Color::Rgb(93, 104, 122),
                    dead: Color::Rgb(176, 46, 36),
                    broken: Color::Rgb(176, 46, 36),
                    unavailable: Color::Rgb(176, 46, 36),
                },
                tile: TilePalette {
                    repo: Color::Rgb(8, 102, 120),
                    branch: Color::Rgb(146, 102, 20),
                    todo: Color::Rgb(78, 89, 109),
                },
                category: CategoryAccentPalette {
                    primary: Color::Rgb(47, 102, 191),
                    secondary: Color::Rgb(171, 80, 31),
                    tertiary: Color::Rgb(105, 73, 171),
                    success: Color::Rgb(39, 132, 73),
                    warning: Color::Rgb(171, 120, 26),
                    danger: Color::Rgb(176, 46, 36),
                },
                dialog: DialogPalette {
                    surface: Color::Rgb(236, 241, 247),
                    input_bg: Color::Rgb(224, 230, 239),
                    button_bg: Color::Rgb(205, 216, 231),
                    button_fg: Color::White,
                },
            },
            ThemePreset::HighContrast => Self {
                base: BasePalette {
                    canvas: Color::Rgb(20, 20, 20),
                    surface: Color::Rgb(20, 20, 20),
                    text: Color::White,
                    text_muted: Color::Gray,
                    header: Color::LightCyan,
                    accent: Color::LightBlue,
                    danger: Color::LightRed,
                },
                interactive: InteractivePalette {
                    focus: Color::LightCyan,
                    selected_bg: Color::Rgb(36, 36, 36),
                    selected_border: Color::LightYellow,
                    border: Color::Gray,
                },
                status: StatusPalette {
                    running: Color::LightGreen,
                    waiting: Color::LightYellow,
                    idle: Color::White,
                    dead: Color::LightRed,
                    broken: Color::LightRed,
                    unavailable: Color::LightRed,
                },
                tile: TilePalette {
                    repo: Color::LightCyan,
                    branch: Color::LightYellow,
                    todo: Color::Gray,
                },
                category: CategoryAccentPalette {
                    primary: Color::LightCyan,
                    secondary: Color::LightMagenta,
                    tertiary: Color::LightBlue,
                    success: Color::LightGreen,
                    warning: Color::LightYellow,
                    danger: Color::LightRed,
                },
                dialog: DialogPalette {
                    surface: Color::Rgb(20, 20, 20),
                    input_bg: Color::Rgb(20, 20, 20),
                    button_bg: Color::Black,
                    button_fg: Color::Black,
                },
            },
            ThemePreset::Mono => Self {
                base: BasePalette {
                    canvas: Color::Rgb(26, 26, 26),
                    surface: Color::Rgb(26, 26, 26),
                    text: Color::White,
                    text_muted: Color::Gray,
                    header: Color::White,
                    accent: Color::Gray,
                    danger: Color::White,
                },
                interactive: InteractivePalette {
                    focus: Color::White,
                    selected_bg: Color::Rgb(35, 35, 35),
                    selected_border: Color::White,
                    border: Color::Gray,
                },
                status: StatusPalette {
                    running: Color::White,
                    waiting: Color::Gray,
                    idle: Color::Gray,
                    dead: Color::White,
                    broken: Color::White,
                    unavailable: Color::White,
                },
                tile: TilePalette {
                    repo: Color::White,
                    branch: Color::Gray,
                    todo: Color::Gray,
                },
                category: CategoryAccentPalette {
                    primary: Color::White,
                    secondary: Color::Gray,
                    tertiary: Color::White,
                    success: Color::White,
                    warning: Color::Gray,
                    danger: Color::White,
                },
                dialog: DialogPalette {
                    surface: Color::Rgb(26, 26, 26),
                    input_bg: Color::Rgb(26, 26, 26),
                    button_bg: Color::Black,
                    button_fg: Color::Black,
                },
            },
            ThemePreset::Custom => Self::from_custom(&CustomThemeConfig::default()),
        }
    }

    pub fn category_accent(&self, category_color: Option<&str>) -> Color {
        let Some(key) = category_color.map(str::trim) else {
            return self.base.accent;
        };

        match key.to_ascii_lowercase().as_str() {
            "primary" | "cyan" => self.category.primary,
            "secondary" | "magenta" => self.category.secondary,
            "tertiary" | "blue" => self.category.tertiary,
            "success" | "green" => self.category.success,
            "warning" | "yellow" => self.category.warning,
            "danger" | "red" => self.category.danger,
            _ => self.base.accent,
        }
    }

    pub fn status_color(&self, status: &str) -> Color {
        match status {
            "running" => self.status.running,
            "waiting" => self.status.waiting,
            "idle" => self.status.idle,
            "dead" => self.status.dead,
            "broken" => self.status.broken,
            "repo_unavailable" => self.status.unavailable,
            _ => self.base.text,
        }
    }

    pub fn tile_colors(&self, selected: bool) -> TileStateColors {
        if selected {
            TileStateColors {
                background: self.interactive.selected_bg,
                border: self.interactive.selected_border,
            }
        } else {
            TileStateColors {
                background: Color::Reset,
                border: self.interactive.border,
            }
        }
    }

    pub fn dialog_surface(&self) -> Color {
        self.dialog.surface
    }
}

impl CustomThemeConfig {
    fn inherit_preset(&self) -> ThemePreset {
        match ThemePreset::from_str(&self.inherit) {
            Ok(ThemePreset::Custom) => {
                warn!(
                    "custom_theme.inherit cannot be 'custom'; falling back to '{}'",
                    ThemePreset::Default.as_str()
                );
                ThemePreset::Default
            }
            Ok(preset) => preset,
            Err(()) => {
                warn!(
                    "invalid custom_theme.inherit '{}'; falling back to '{}'",
                    self.inherit,
                    ThemePreset::Default.as_str()
                );
                ThemePreset::Default
            }
        }
    }

    fn apply_overrides(&self, theme: &mut Theme) {
        macro_rules! apply {
            ($target:expr, $value:expr, $path:literal) => {
                apply_hex_override(&mut $target, $value, $path)
            };
        }

        apply!(
            theme.base.canvas,
            self.base.canvas.as_deref(),
            "custom_theme.base.canvas"
        );
        apply!(
            theme.base.surface,
            self.base.surface.as_deref(),
            "custom_theme.base.surface"
        );
        apply!(
            theme.base.text,
            self.base.text.as_deref(),
            "custom_theme.base.text"
        );
        apply!(
            theme.base.text_muted,
            self.base.text_muted.as_deref(),
            "custom_theme.base.text_muted"
        );
        apply!(
            theme.base.header,
            self.base.header.as_deref(),
            "custom_theme.base.header"
        );
        apply!(
            theme.base.accent,
            self.base.accent.as_deref(),
            "custom_theme.base.accent"
        );
        apply!(
            theme.base.danger,
            self.base.danger.as_deref(),
            "custom_theme.base.danger"
        );

        apply!(
            theme.interactive.focus,
            self.interactive.focus.as_deref(),
            "custom_theme.interactive.focus"
        );
        apply!(
            theme.interactive.selected_bg,
            self.interactive.selected_bg.as_deref(),
            "custom_theme.interactive.selected_bg"
        );
        apply!(
            theme.interactive.selected_border,
            self.interactive.selected_border.as_deref(),
            "custom_theme.interactive.selected_border"
        );
        apply!(
            theme.interactive.border,
            self.interactive.border.as_deref(),
            "custom_theme.interactive.border"
        );

        apply!(
            theme.status.running,
            self.status.running.as_deref(),
            "custom_theme.status.running"
        );
        apply!(
            theme.status.waiting,
            self.status.waiting.as_deref(),
            "custom_theme.status.waiting"
        );
        apply!(
            theme.status.idle,
            self.status.idle.as_deref(),
            "custom_theme.status.idle"
        );
        apply!(
            theme.status.dead,
            self.status.dead.as_deref(),
            "custom_theme.status.dead"
        );
        apply!(
            theme.status.broken,
            self.status.broken.as_deref(),
            "custom_theme.status.broken"
        );
        apply!(
            theme.status.unavailable,
            self.status.unavailable.as_deref(),
            "custom_theme.status.unavailable"
        );

        apply!(
            theme.tile.repo,
            self.tile.repo.as_deref(),
            "custom_theme.tile.repo"
        );
        apply!(
            theme.tile.branch,
            self.tile.branch.as_deref(),
            "custom_theme.tile.branch"
        );
        apply!(
            theme.tile.todo,
            self.tile.todo.as_deref(),
            "custom_theme.tile.todo"
        );

        apply!(
            theme.category.primary,
            self.category.primary.as_deref(),
            "custom_theme.category.primary"
        );
        apply!(
            theme.category.secondary,
            self.category.secondary.as_deref(),
            "custom_theme.category.secondary"
        );
        apply!(
            theme.category.tertiary,
            self.category.tertiary.as_deref(),
            "custom_theme.category.tertiary"
        );
        apply!(
            theme.category.success,
            self.category.success.as_deref(),
            "custom_theme.category.success"
        );
        apply!(
            theme.category.warning,
            self.category.warning.as_deref(),
            "custom_theme.category.warning"
        );
        apply!(
            theme.category.danger,
            self.category.danger.as_deref(),
            "custom_theme.category.danger"
        );

        apply!(
            theme.dialog.surface,
            self.dialog.surface.as_deref(),
            "custom_theme.dialog.surface"
        );
        apply!(
            theme.dialog.input_bg,
            self.dialog.input_bg.as_deref(),
            "custom_theme.dialog.input_bg"
        );
        apply!(
            theme.dialog.button_bg,
            self.dialog.button_bg.as_deref(),
            "custom_theme.dialog.button_bg"
        );
        apply!(
            theme.dialog.button_fg,
            self.dialog.button_fg.as_deref(),
            "custom_theme.dialog.button_fg"
        );
    }
}

fn apply_hex_override(target: &mut Color, raw: Option<&str>, key: &str) {
    let Some(raw) = raw else {
        return;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }

    match parse_hex_color(trimmed) {
        Some(color) => *target = color,
        None => warn!(
            "invalid custom theme color '{}' for {}; expected #RRGGBB",
            raw, key
        ),
    }
}

fn parse_hex_color(raw: &str) -> Option<Color> {
    let hex = raw.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_preset(ThemePreset::Default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default_preset() {
        let theme = Theme::default();
        assert_eq!(theme.base.header, Color::Cyan);
        assert_eq!(theme.base.accent, Color::Magenta);
        assert_eq!(theme.interactive.focus, Color::Cyan);
        assert_eq!(theme.base.text, Color::White);
        assert_eq!(theme.base.text_muted, Color::DarkGray);
    }

    #[test]
    fn test_theme_light_preset() {
        let theme = Theme::from_preset(ThemePreset::Light);
        assert_eq!(theme.base.canvas, Color::Rgb(226, 231, 238));
        assert_eq!(theme.base.text, Color::Rgb(34, 42, 58));
        assert_eq!(theme.interactive.focus, Color::Rgb(47, 102, 191));
        assert_eq!(theme.dialog.button_fg, Color::White);
    }

    #[test]
    fn test_category_accent_supports_legacy_and_semantic_keys() {
        let theme = Theme::from_preset(ThemePreset::Light);
        assert_eq!(
            theme.category_accent(Some("primary")),
            theme.category.primary
        );
        assert_eq!(theme.category_accent(Some("cyan")), theme.category.primary);
        assert_eq!(
            theme.category_accent(Some("warning")),
            theme.category.warning
        );
        assert_eq!(
            theme.category_accent(Some("yellow")),
            theme.category.warning
        );
    }

    #[test]
    fn test_theme_preset_parse() {
        assert_eq!(ThemePreset::from_str("default"), Ok(ThemePreset::Default));
        assert_eq!(ThemePreset::from_str("light"), Ok(ThemePreset::Light));
        assert_eq!(
            ThemePreset::from_str("high-contrast"),
            Ok(ThemePreset::HighContrast)
        );
        assert_eq!(ThemePreset::from_str("mono"), Ok(ThemePreset::Mono));
        assert_eq!(ThemePreset::from_str("custom"), Ok(ThemePreset::Custom));
        assert!(ThemePreset::from_str("unknown").is_err());
    }

    #[test]
    fn test_theme_preset_cycle() {
        assert_eq!(ThemePreset::Default.next(), ThemePreset::Light);
        assert_eq!(ThemePreset::Light.next(), ThemePreset::HighContrast);
        assert_eq!(ThemePreset::Mono.next(), ThemePreset::Custom);
        assert_eq!(ThemePreset::Custom.next(), ThemePreset::Default);
        assert_eq!(ThemePreset::Default.previous(), ThemePreset::Custom);
        assert_eq!(ThemePreset::Light.previous(), ThemePreset::Default);
        assert_eq!(ThemePreset::Custom.previous(), ThemePreset::Mono);
    }

    #[test]
    fn test_custom_theme_overrides_apply_hex_colors() {
        let custom = CustomThemeConfig {
            inherit: "light".to_string(),
            base: BasePaletteOverride {
                canvas: Some("#AABBCC".to_string()),
                ..BasePaletteOverride::default()
            },
            interactive: InteractivePaletteOverride {
                focus: Some("#123456".to_string()),
                ..InteractivePaletteOverride::default()
            },
            ..CustomThemeConfig::default()
        };

        let theme = Theme::resolve(ThemePreset::Custom, &custom);
        assert_eq!(theme.base.canvas, Color::Rgb(170, 187, 204));
        assert_eq!(theme.interactive.focus, Color::Rgb(18, 52, 86));
        assert_eq!(
            theme.base.text,
            Theme::from_preset(ThemePreset::Light).base.text
        );
    }

    #[test]
    fn test_custom_theme_invalid_hex_falls_back_to_inherited_value() {
        let custom = CustomThemeConfig {
            inherit: "default".to_string(),
            base: BasePaletteOverride {
                canvas: Some("cyan".to_string()),
                ..BasePaletteOverride::default()
            },
            ..CustomThemeConfig::default()
        };

        let theme = Theme::resolve(ThemePreset::Custom, &custom);
        assert_eq!(
            theme.base.canvas,
            Theme::from_preset(ThemePreset::Default).base.canvas
        );
    }

    #[test]
    fn test_custom_theme_inherit_custom_falls_back_to_default() {
        let custom = CustomThemeConfig {
            inherit: "custom".to_string(),
            ..CustomThemeConfig::default()
        };

        let theme = Theme::resolve(ThemePreset::Custom, &custom);
        assert_eq!(
            theme.base.canvas,
            Theme::from_preset(ThemePreset::Default).base.canvas
        );
    }
}
