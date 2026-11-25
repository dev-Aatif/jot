use crate::config::ThemeConfig;
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct ResolvedTheme {
    pub preset_name: String,
    pub no_color: bool,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub border_muted: Color,
    pub border_active: Color,
    pub accent_primary: Color,
    pub accent_secondary: Color,
    pub text_primary: Color,
    pub text_muted: Color,
    pub danger: Color,
    pub success: Color,
}

impl ResolvedTheme {
    pub fn from_config(config: &ThemeConfig, no_color: bool) -> Self {
        let palette = config.resolve();
        if no_color {
            return Self {
                preset_name: palette.preset_name,
                no_color: true,
                bg_surface: Color::Reset,
                bg_elevated: Color::Reset,
                border_muted: Color::Reset,
                border_active: Color::Reset,
                accent_primary: Color::Reset,
                accent_secondary: Color::Reset,
                text_primary: Color::Reset,
                text_muted: Color::Reset,
                danger: Color::Reset,
                success: Color::Reset,
            };
        }

        Self {
            preset_name: palette.preset_name,
            no_color: false,
            bg_surface: parse_color(&palette.bg_surface),
            bg_elevated: parse_color(&palette.bg_elevated),
            border_muted: parse_color(&palette.border_muted),
            border_active: parse_color(&palette.border_active),
            accent_primary: parse_color(&palette.accent_primary),
            accent_secondary: parse_color(&palette.accent_secondary),
            text_primary: parse_color(&palette.text_primary),
            text_muted: parse_color(&palette.text_muted),
            danger: parse_color(&palette.danger),
            success: parse_color(&palette.success),
        }
    }

    pub fn surface_style(&self) -> Style {
        if self.no_color {
            Style::default()
        } else {
            Style::default().bg(self.bg_surface).fg(self.text_primary)
        }
    }

    pub fn border_style(&self, active: bool) -> Style {
        if self.no_color {
            if active {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            }
        } else if active {
            Style::default().fg(self.border_active)
        } else {
            Style::default().fg(self.border_muted)
        }
    }

    pub fn title_style(&self, active: bool) -> Style {
        if self.no_color {
            if active {
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            }
        } else if active {
            Style::default()
                .fg(self.accent_primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.text_muted)
        }
    }

    pub fn selected_item_style(&self) -> Style {
        if self.no_color {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
                .bg(self.bg_elevated)
                .fg(self.text_primary)
                .add_modifier(Modifier::BOLD)
        }
    }

    pub fn muted_style(&self) -> Style {
        if self.no_color {
            Style::default().add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(self.text_muted)
        }
    }

    pub fn chip_style(&self) -> Style {
        if self.no_color {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(self.accent_secondary)
                .bg(self.bg_elevated)
        }
    }

    pub fn primary_accent(&self) -> Style {
        if self.no_color {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(self.accent_primary)
                .add_modifier(Modifier::BOLD)
        }
    }

    pub fn danger_style(&self) -> Style {
        if self.no_color {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(self.danger)
                .add_modifier(Modifier::BOLD)
        }
    }

    pub fn success_style(&self) -> Style {
        if self.no_color {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(self.success)
                .add_modifier(Modifier::BOLD)
        }
    }
}

pub fn parse_color(color: &str) -> Color {
    let color = color.trim();
    if color.starts_with('#')
        && color.len() == 7
        && color.is_ascii()
        && let Ok(r) = u8::from_str_radix(&color[1..3], 16)
        && let Ok(g) = u8::from_str_radix(&color[3..5], 16)
        && let Ok(b) = u8::from_str_radix(&color[5..7], 16)
    {
        return Color::Rgb(r, g, b);
    }
    match color.to_lowercase().as_str() {
        "reset" | "default" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark_gray" | "darkgray" => Color::DarkGray,
        "light_red" => Color::LightRed,
        "light_green" => Color::LightGreen,
        "light_yellow" => Color::LightYellow,
        "light_blue" => Color::LightBlue,
        "light_magenta" => Color::LightMagenta,
        "light_cyan" => Color::LightCyan,
        "white" => Color::White,
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_utf8_and_fallbacks() {
        assert_eq!(parse_color("#bd93f9"), Color::Rgb(189, 147, 249));
        assert_eq!(parse_color("cyan"), Color::Cyan);
        assert_eq!(parse_color("reset"), Color::Reset);
        assert_eq!(parse_color("#ééé"), Color::Reset);
        assert_eq!(parse_color("invalid"), Color::Reset);
    }

    #[test]
    fn test_no_color_resets_palette() {
        let cfg = ThemeConfig::default();
        let resolved = ResolvedTheme::from_config(&cfg, true);
        assert_eq!(resolved.border_active, Color::Reset);
        assert_eq!(resolved.bg_surface, Color::Reset);
        assert_eq!(resolved.danger, Color::Reset);
    }
}
