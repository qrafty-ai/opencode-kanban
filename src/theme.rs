use tuirealm::ratatui::style::Color;

/// Theme configuration for the kanban board UI.
/// Provides color settings for different UI elements.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Color for headers and titles
    pub header: Color,
    /// Color for column backgrounds/borders
    pub column: Color,
    /// Color for focused elements
    pub focus: Color,
    /// Color for task text
    pub task: Color,
    /// Color for secondary text
    pub secondary: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            header: Color::Cyan,
            column: Color::Magenta,
            focus: Color::Cyan,
            task: Color::White,
            secondary: Color::DarkGray,
        }
    }
}

/// Parse a color string into a ratatui Color.
///
/// Supports:
/// - Named colors: "cyan", "magenta", "red", "green", "blue", "white", "black", etc.
/// - Hex colors: "#RRGGBB" format (e.g., "#FF0000" for red)
///
/// Returns None for invalid color strings.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();

    // Try hex format first: #RRGGBB
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

    // Try named colors (case-insensitive)
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
    fn test_theme_default() {
        let theme = Theme::default();
        assert_eq!(theme.header, Color::Cyan);
        assert_eq!(theme.column, Color::Magenta);
        assert_eq!(theme.focus, Color::Cyan);
        assert_eq!(theme.task, Color::White);
        assert_eq!(theme.secondary, Color::DarkGray);
    }
}
