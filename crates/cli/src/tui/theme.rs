use ratatui::style::{Color, Modifier, Style};

use cleanprojects_core::{Category, Risk};

pub const ACCENT: Color = Color::Rgb(249, 115, 22);
pub const MUTED: Color = Color::Rgb(140, 132, 126);
pub const GOOD: Color = Color::Rgb(34, 197, 94);
pub const WARN: Color = Color::Rgb(234, 179, 8);
pub const BAD: Color = Color::Rgb(239, 68, 68);

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}
pub fn accent_bold() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
pub fn muted() -> Style {
    Style::default().fg(MUTED)
}
pub fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}
pub fn cursor() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}
pub fn header() -> Style {
    Style::default().fg(MUTED).add_modifier(Modifier::BOLD)
}
pub fn risk(r: Risk) -> Style {
    match r {
        Risk::Low => Style::default().fg(GOOD),
        Risk::Medium => Style::default().fg(WARN),
        Risk::High => Style::default().fg(BAD).add_modifier(Modifier::BOLD),
    }
}
pub fn category(c: Category) -> Color {
    use Category::*;
    match c {
        Project => ACCENT,
        PackageCache => Color::Rgb(251, 191, 36),
        BuildCache => Color::Rgb(252, 211, 77),
        Ide => Color::Rgb(96, 165, 250),
        Browser => Color::Rgb(167, 139, 250),
        Container => Color::Rgb(45, 212, 191),
        Mobile => Color::Rgb(244, 114, 182),
        Git => Color::Rgb(248, 113, 113),
        AppData => Color::Rgb(192, 132, 252),
        Temp => MUTED,
    }
}
