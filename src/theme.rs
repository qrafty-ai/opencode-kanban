use std::str::FromStr;

use tuirealm::ratatui::style::Color;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum ThemePreset {
    #[default]
    Default,
    Light,
    HighContrast,
    Mono,
}

impl ThemePreset {
    pub const ALL: [Self; 4] = [Self::Default, Self::Light, Self::HighContrast, Self::Mono];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Light => "light",
            Self::HighContrast => "high-contrast",
            Self::Mono => "mono",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Default => "Balanced colors for everyday use",
            Self::Light => "Bright background with dark text",
            Self::HighContrast => "Enhanced visibility, bright on dark",
            Self::Mono => "Minimal monochrome aesthetic",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Default => Self::Light,
            Self::Light => Self::HighContrast,
            Self::HighContrast => Self::Mono,
            Self::Mono => Self::Default,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Default => Self::Mono,
            Self::Light => Self::Default,
            Self::HighContrast => Self::Light,
            Self::Mono => Self::HighContrast,
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

#[derive(Debug, Clone, Copy)]
pub struct TileStateColors {
    pub background: Color,
    pub border: Color,
}

impl Theme {
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
                    canvas: Color::Rgb(246, 248, 252),
                    surface: Color::Rgb(255, 255, 255),
                    text: Color::Rgb(32, 38, 51),
                    text_muted: Color::Rgb(95, 105, 122),
                    header: Color::Rgb(37, 99, 235),
                    accent: Color::Rgb(2, 132, 199),
                    danger: Color::Rgb(185, 28, 28),
                },
                interactive: InteractivePalette {
                    focus: Color::Rgb(37, 99, 235),
                    selected_bg: Color::Rgb(227, 237, 255),
                    selected_border: Color::Rgb(59, 130, 246),
                    border: Color::Rgb(196, 208, 224),
                },
                status: StatusPalette {
                    running: Color::Rgb(22, 163, 74),
                    waiting: Color::Rgb(202, 138, 4),
                    idle: Color::Rgb(71, 85, 105),
                    dead: Color::Rgb(185, 28, 28),
                    broken: Color::Rgb(185, 28, 28),
                    unavailable: Color::Rgb(185, 28, 28),
                },
                tile: TilePalette {
                    repo: Color::Rgb(14, 116, 144),
                    branch: Color::Rgb(161, 98, 7),
                    todo: Color::Rgb(95, 105, 122),
                },
                category: CategoryAccentPalette {
                    primary: Color::Rgb(37, 99, 235),
                    secondary: Color::Rgb(194, 65, 12),
                    tertiary: Color::Rgb(124, 58, 237),
                    success: Color::Rgb(22, 163, 74),
                    warning: Color::Rgb(202, 138, 4),
                    danger: Color::Rgb(185, 28, 28),
                },
                dialog: DialogPalette {
                    surface: Color::Rgb(255, 255, 255),
                    input_bg: Color::Rgb(241, 245, 249),
                    button_bg: Color::Rgb(226, 232, 240),
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
        assert_eq!(theme.base.canvas, Color::Rgb(246, 248, 252));
        assert_eq!(theme.base.text, Color::Rgb(32, 38, 51));
        assert_eq!(theme.interactive.focus, Color::Rgb(37, 99, 235));
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
        assert!(ThemePreset::from_str("unknown").is_err());
    }

    #[test]
    fn test_theme_preset_cycle() {
        assert_eq!(ThemePreset::Default.next(), ThemePreset::Light);
        assert_eq!(ThemePreset::Light.next(), ThemePreset::HighContrast);
        assert_eq!(ThemePreset::Default.previous(), ThemePreset::Mono);
        assert_eq!(ThemePreset::Light.previous(), ThemePreset::Default);
    }
}
