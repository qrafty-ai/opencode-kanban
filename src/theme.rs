use std::str::FromStr;

use tuirealm::ratatui::style::Color;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum ThemePreset {
    #[default]
    Default,
    HighContrast,
    Mono,
}

impl FromStr for ThemePreset {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" => Ok(Self::Default),
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
pub struct TileStateColors {
    pub background: Color,
    pub border: Color,
}

impl Theme {
    pub fn from_preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::Default => Self {
                base: BasePalette {
                    canvas: Color::Black,
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
                dialog: DialogPalette {
                    surface: Color::Rgb(36, 40, 56),
                    input_bg: Color::Rgb(36, 40, 56),
                    button_bg: Color::Black,
                    button_fg: Color::Black,
                },
            },
            ThemePreset::HighContrast => Self {
                base: BasePalette {
                    canvas: Color::Black,
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
                dialog: DialogPalette {
                    surface: Color::Rgb(20, 20, 20),
                    input_bg: Color::Rgb(20, 20, 20),
                    button_bg: Color::Black,
                    button_fg: Color::Black,
                },
            },
            ThemePreset::Mono => Self {
                base: BasePalette {
                    canvas: Color::Black,
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
        category_color
            .and_then(parse_color)
            .unwrap_or(self.base.accent)
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

pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();

    if s.starts_with('#') && s.len() == 7 {
        let hex = &s[1..];
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Some(Color::Rgb(r, g, b));
        }
        return None;
    }

    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_named_cyan() {
        assert_eq!(parse_color("cyan"), Some(Color::Cyan));
    }

    #[test]
    fn test_parse_color_named_magenta() {
        assert_eq!(parse_color("magenta"), Some(Color::Magenta));
    }

    #[test]
    fn test_parse_color_hex_red() {
        assert_eq!(parse_color("#FF0000"), Some(Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn test_parse_color_hex_green() {
        assert_eq!(parse_color("#00FF00"), Some(Color::Rgb(0, 255, 0)));
    }

    #[test]
    fn test_parse_color_hex_blue() {
        assert_eq!(parse_color("#0000FF"), Some(Color::Rgb(0, 0, 255)));
    }

    #[test]
    fn test_parse_color_invalid() {
        assert_eq!(parse_color("invalid"), None);
    }

    #[test]
    fn test_parse_color_invalid_hex() {
        assert_eq!(parse_color("#GGG"), None);
    }

    #[test]
    fn test_parse_color_case_insensitive() {
        assert_eq!(parse_color("CYAN"), Some(Color::Cyan));
        assert_eq!(parse_color("Cyan"), Some(Color::Cyan));
    }

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
    fn test_theme_preset_parse() {
        assert_eq!(ThemePreset::from_str("default"), Ok(ThemePreset::Default));
        assert_eq!(
            ThemePreset::from_str("high-contrast"),
            Ok(ThemePreset::HighContrast)
        );
        assert_eq!(ThemePreset::from_str("mono"), Ok(ThemePreset::Mono));
        assert!(ThemePreset::from_str("unknown").is_err());
    }
}
