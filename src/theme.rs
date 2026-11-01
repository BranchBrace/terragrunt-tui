//! Thème unique de l'interface : palette ELIA36 (lospec.com/palette-list/elia36) sur un
//! fond bleu nuit, plus deux couleurs hors palette là où elle manque (un turquoise lisible
//! pour les ajouts et un vert sombre pour le fond des lignes ajoutées).
//!
//! Aucune autre partie de l'interface ne doit écrire une couleur en dur.

use crate::model::ChangeKind;
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};

// Fonds et cadres.
pub const BG: Color = Color::Rgb(36, 43, 74);
pub const SELECTION: Color = Color::Rgb(61, 57, 87);
pub const BORDER_FOCUS: Color = Color::Rgb(93, 139, 179);
pub const BORDER: Color = Color::Rgb(64, 85, 120);
pub const BAND_BG: Color = Color::Rgb(61, 57, 87);

// Textes.
pub const TEXT: Color = Color::Rgb(223, 237, 237);
pub const TEXT_SECONDARY: Color = Color::Rgb(167, 184, 194);
pub const TEXT_DIM: Color = Color::Rgb(114, 135, 148);
pub const TITLE: Color = Color::Rgb(198, 219, 222);
pub const KEY: Color = Color::Rgb(255, 211, 163);

// Sémantique des changements et des statuts.
pub const ADD: Color = Color::Rgb(95, 211, 200);
pub const CHANGE: Color = Color::Rgb(255, 172, 127);
pub const DELETE: Color = Color::Rgb(245, 79, 79);
pub const REPLACE: Color = Color::Rgb(230, 204, 138);
pub const READ: Color = Color::Rgb(93, 139, 179);
pub const UNKNOWN: Color = Color::Rgb(230, 163, 163);
pub const PLANNED: Color = ADD;
pub const NO_CHANGES: Color = TEXT_DIM;
pub const FAILED: Color = DELETE;

// Diff unifié.
pub const DIFF_REMOVED_BG: Color = Color::Rgb(102, 51, 61);
pub const DIFF_ADDED_BG: Color = Color::Rgb(43, 77, 63);
pub const DIFF_REMOVED_HIGHLIGHT: Color = Color::Rgb(191, 38, 81);
pub const DIFF_ADDED_HIGHLIGHT: Color = Color::Rgb(76, 143, 130);
pub const DIFF_HIGHLIGHT_FG: Color = Color::Rgb(250, 250, 195);
pub const DIFF_CONTEXT: Color = TEXT_DIM;
pub const DIFF_FOLD: Color = Color::Rgb(93, 139, 179);

// Vagues : un accent par vague, en cycle ; la vague inconnue a sa propre couleur.
const WAVES: [Color; 6] = [
    Color::Rgb(153, 194, 219),
    Color::Rgb(120, 179, 146),
    Color::Rgb(230, 204, 138),
    Color::Rgb(255, 172, 127),
    Color::Rgb(230, 163, 163),
    Color::Rgb(181, 224, 186),
];
pub const WAVE_UNKNOWN: Color = Color::Rgb(153, 92, 149);

/// Style de base de tout écran : fond bleu nuit et texte clair.
pub fn base() -> Style {
    Style::default().bg(BG).fg(TEXT)
}

/// Bloc à bordure dont le cadre dit s'il a le focus.
pub fn panel(title: &str, focused: bool) -> Block<'_> {
    let (color, border_type) = if focused {
        (BORDER_FOCUS, ratatui::widgets::BorderType::Thick)
    } else {
        (BORDER, ratatui::widgets::BorderType::Plain)
    };
    Block::default()
        .title(format!(" {title} "))
        .title_style(
            Style::default()
                .fg(if focused { TITLE } else { TEXT_SECONDARY })
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(color))
}

pub fn wave_color(wave: Option<usize>) -> Color {
    wave.map_or(WAVE_UNKNOWN, |number| WAVES[number % WAVES.len()])
}

pub fn status_color(status: &str) -> Color {
    match status {
        "planned" => PLANNED,
        "no_changes" => NO_CHANGES,
        "failed" => FAILED,
        _ => UNKNOWN,
    }
}

pub fn kind_color(kind: ChangeKind) -> Color {
    match kind {
        ChangeKind::Add => ADD,
        ChangeKind::Delete => DELETE,
        ChangeKind::Change => CHANGE,
        ChangeKind::Replace => REPLACE,
        ChangeKind::Read => READ,
        ChangeKind::NoChange => NO_CHANGES,
        ChangeKind::Unknown => UNKNOWN,
    }
}

pub fn selection() -> Style {
    Style::default().bg(SELECTION).add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(TEXT_DIM)
}

pub fn secondary() -> Style {
    Style::default().fg(TEXT_SECONDARY)
}

pub fn title() -> Style {
    Style::default().fg(TITLE).add_modifier(Modifier::BOLD)
}

/// Une touche et son libellé pour le pied de page : `↑↓ module`.
pub fn key_hint(key: &str, label: &str) -> [Span<'static>; 2] {
    [
        Span::styled(
            key.to_owned(),
            Style::default().fg(KEY).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {label}  "), dim()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_colors_cycle_through_distinct_palette_accents() {
        let colors = (0..WAVES.len())
            .map(|w| wave_color(Some(w)))
            .collect::<Vec<_>>();
        for (index, color) in colors.iter().enumerate() {
            assert!(
                !colors[index + 1..].contains(color),
                "la couleur de vague {index} est dupliquée"
            );
        }
        assert_eq!(wave_color(Some(WAVES.len())), colors[0]);
        assert_eq!(wave_color(None), WAVE_UNKNOWN);
        assert!(!colors.contains(&WAVE_UNKNOWN));
    }

    #[test]
    fn statuses_and_kinds_map_to_the_agreed_accents() {
        assert_eq!(status_color("planned"), ADD);
        assert_eq!(status_color("failed"), DELETE);
        assert_eq!(status_color("anything"), UNKNOWN);
        assert_eq!(kind_color(ChangeKind::Replace), REPLACE);
        assert_eq!(kind_color(ChangeKind::Change), CHANGE);
        assert_ne!(kind_color(ChangeKind::Add), kind_color(ChangeKind::Delete));
    }
}
