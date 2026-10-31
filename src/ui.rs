use crate::model::{ChangeKind, ModuleResult, ResourceChange, RunResult, Totals};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{collections::BTreeMap, io, io::Stdout};

pub struct TerminalSession {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn confirm_plan(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    root: &str,
    command: &str,
    units: &[crate::graph::Unit],
    workspace: &str,
) -> io::Result<bool> {
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let lines = vec![
                Line::from(Span::styled("WAYFINDER · PLAN UNIQUEMENT", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(format!("Racine : {root}")),
                Line::from(format!("Périmètre : {} unités découvertes", units.len())),
                Line::from(format!("Fichiers de travail temporaires : {workspace}")),
                Line::from(""),
                Line::from(Span::styled("Commande qui sera lancée après Entrée :", Style::default().add_modifier(Modifier::BOLD))),
                Line::from(command),
                Line::from(""),
                Line::from(Span::styled("Effets : calcule des plans Terragrunt; ne les applique jamais.", Style::default().fg(Color::Yellow))),
                Line::from("Aucune commande apply/destroy ni argument libre n'est accepté."),
                Line::from("Les valeurs sensibles sont masquées dans la vue des changements; cette protection n'est pas une garantie absolue."),
                Line::from("Les journaux restent cachés par défaut; les plans structurés existent brièvement dans le répertoire temporaire."),
                Line::from(""),
                Line::from("Entrée : lancer le plan     q / Échap : annuler"),
            ];
            frame.render_widget(
                Paragraph::new(Text::from(lines)).block(Block::default().title("Avant exécution").borders(Borders::ALL)).wrap(Wrap { trim: false }),
                area,
            );
        })?;
        match event::read()? {
            Event::Key(key) if matches!(key.code, KeyCode::Enter) => return Ok(true),
            Event::Key(key) if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) => {
                return Ok(false);
            }
            _ => {}
        }
    }
}

pub fn confirm_cleanup(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    paths: &[String],
) -> io::Result<bool> {
    loop {
        terminal.draw(|frame| {
            let mut lines = vec![
                Line::from(Span::styled(
                    "RÉPERTOIRES TEMPORAIRES WAYFINDER RESTANTS",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(
                    "Seuls les répertoires marqués comme appartenant à Wayfinder sont listés.",
                ),
                Line::from(
                    "Confirmez leur suppression définitive. Aucune suppression automatique.",
                ),
                Line::from(""),
            ];
            lines.extend(paths.iter().map(|path| Line::from(path.clone())));
            lines.push(Line::from(""));
            lines.push(Line::from(
                "y : supprimer les répertoires listés     autre touche : conserver",
            ));
            frame.render_widget(
                Paragraph::new(Text::from(lines)).block(Block::default().borders(Borders::ALL)),
                frame.area(),
            );
        })?;
        match event::read()? {
            Event::Key(key) if key.code == KeyCode::Char('y') => return Ok(true),
            Event::Key(_) => return Ok(false),
            _ => {}
        }
    }
}

pub fn show_progress(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    current: Option<&str>,
    completed: usize,
    total: usize,
) -> io::Result<bool> {
    terminal.draw(|frame| {
        let text = vec![
            Line::from(Span::styled(
                "PLAN TERRAGRUNT EN COURS",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("Mode lecture seule : run --all plan"),
            Line::from(format!(
                "Progression détectée : {completed}/{total} modules"
            )),
            Line::from(format!(
                "Module actif : {}",
                current.unwrap_or("en attente de sortie")
            )),
            Line::from("Journaux bruts masqués · q / Échap / Ctrl-C : interrompre et nettoyer"),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(text))
                .block(Block::default().title("Exécution").borders(Borders::ALL)),
            frame.area(),
        );
    })?;
    if event::poll(std::time::Duration::ZERO)?
        && let Event::Key(key) = event::read()?
        && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
    {
        return Ok(false);
    }
    Ok(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Modules,
    Resources,
    Changes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailView {
    BeforeAfter,
    Diff,
}

#[derive(Debug)]
struct ReviewState {
    pane: Pane,
    module: usize,
    resource: usize,
    view: DetailView,
    show_logs: bool,
    search: String,
    searching: bool,
    finished: bool,
}

impl Default for ReviewState {
    fn default() -> Self {
        Self {
            pane: Pane::Modules,
            module: 0,
            resource: 0,
            view: DetailView::BeforeAfter,
            show_logs: false,
            search: String::new(),
            searching: false,
            finished: false,
        }
    }
}

impl ReviewState {
    fn handle(&mut self, key: KeyEvent, module_count: usize, resource_count: usize) {
        if self.searching {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => self.searching = false,
                KeyCode::Backspace => {
                    self.search.pop();
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.search.push(character)
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.finished = true,
            KeyCode::Esc | KeyCode::Left => {
                self.pane = match self.pane {
                    Pane::Changes => Pane::Resources,
                    Pane::Resources => Pane::Modules,
                    Pane::Modules => {
                        self.finished = true;
                        Pane::Modules
                    }
                }
            }
            KeyCode::Right => {
                self.pane = match self.pane {
                    Pane::Modules => Pane::Resources,
                    Pane::Resources => Pane::Changes,
                    Pane::Changes => Pane::Changes,
                }
            }
            KeyCode::Tab => {
                self.pane = match self.pane {
                    Pane::Modules => Pane::Resources,
                    Pane::Resources => Pane::Changes,
                    Pane::Changes => Pane::Changes,
                }
            }
            KeyCode::Up => match self.pane {
                Pane::Modules => self.module = self.module.saturating_sub(1),
                Pane::Resources | Pane::Changes => self.resource = self.resource.saturating_sub(1),
            },
            KeyCode::Down => match self.pane {
                Pane::Modules => {
                    self.module = self
                        .module
                        .saturating_add(1)
                        .min(module_count.saturating_sub(1))
                }
                Pane::Resources | Pane::Changes => {
                    self.resource = self
                        .resource
                        .saturating_add(1)
                        .min(resource_count.saturating_sub(1))
                }
            },
            KeyCode::Char('d') => {
                self.view = match self.view {
                    DetailView::BeforeAfter => DetailView::Diff,
                    DetailView::Diff => DetailView::BeforeAfter,
                }
            }
            KeyCode::Char('l') => self.show_logs = !self.show_logs,
            KeyCode::Char('/') => self.searching = true,
            _ => {}
        }
    }
}

pub fn review(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    result: &RunResult,
) -> io::Result<()> {
    let mut state = ReviewState::default();
    loop {
        let visible_modules = visible_modules(result, &state.search);
        state.module = state.module.min(visible_modules.len().saturating_sub(1));
        let module = visible_modules
            .get(state.module)
            .and_then(|index| result.modules.get(*index));
        let visible_resources = module
            .map(|item| visible_resources(item, &state.search))
            .unwrap_or_default();
        state.resource = state
            .resource
            .min(visible_resources.len().saturating_sub(1));
        terminal.draw(|frame| {
            render_review(
                frame,
                result,
                &state,
                &visible_modules,
                module,
                &visible_resources,
            )
        })?;
        if state.finished {
            return Ok(());
        }
        if let Event::Key(key) = event::read()? {
            if !state.searching && key.code == KeyCode::Char('/') {
                state.searching = true;
                continue;
            }
            let resource_count = visible_resources.len();
            state.handle(key, visible_modules.len(), resource_count);
        }
    }
}

fn visible_modules(result: &RunResult, search: &str) -> Vec<usize> {
    let needle = search.to_lowercase();
    let mut indices: Vec<_> = result
        .modules
        .iter()
        .enumerate()
        .filter(|(_, module)| module.path.to_lowercase().contains(&needle))
        .map(|(index, _)| index)
        .collect();
    indices.sort_by_key(|index| {
        let module = &result.modules[*index];
        (module.wave.unwrap_or(usize::MAX), module.path.clone())
    });
    indices
}

fn visible_resources(module: &ModuleResult, search: &str) -> Vec<usize> {
    let needle = search.to_lowercase();
    module
        .resources
        .iter()
        .enumerate()
        .filter(|(_, resource)| resource.address.to_lowercase().contains(&needle))
        .map(|(index, _)| index)
        .collect()
}

fn render_review(
    frame: &mut Frame,
    result: &RunResult,
    state: &ReviewState,
    modules: &[usize],
    module: Option<&ModuleResult>,
    resources: &[usize],
) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(2)])
        .split(area);
    let widths = match state.pane {
        Pane::Modules => [
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ],
        Pane::Resources => [
            Constraint::Percentage(16),
            Constraint::Percentage(38),
            Constraint::Percentage(46),
        ],
        Pane::Changes => [
            Constraint::Percentage(16),
            Constraint::Percentage(26),
            Constraint::Percentage(58),
        ],
    };
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(widths)
        .split(rows[0]);

    let module_items: Vec<ListItem> = modules
        .iter()
        .map(|index| {
            let item = &result.modules[*index];
            let marker = item.wave.map_or("W?".into(), |wave| format!("W{wave}"));
            let mut lines = if state.pane == Pane::Modules {
                vec![
                    Line::from(vec![
                        Span::styled(format!("{marker} "), Style::default().fg(Color::Cyan)),
                        Span::styled(status_label(item), status_style(&item.status)),
                    ]),
                    Line::from(item.path.clone()),
                ]
            } else {
                let module_name = item
                    .path
                    .rsplit(|character| character == '/' || character == char::from(92))
                    .next()
                    .unwrap_or(&item.path);
                vec![
                    Line::from(Span::styled(status_label(item), status_style(&item.status))),
                    Line::from(format!("{marker} {module_name}")),
                ]
            };
            if state.pane == Pane::Modules
                && let Some(totals) = &item.totals
            {
                lines.push(counter_line(totals));
            }
            ListItem::new(lines)
        })
        .collect();
    let mut module_state = ListState::default();
    module_state.select((!modules.is_empty()).then_some(state.module));
    frame.render_stateful_widget(
        List::new(module_items)
            .block(panel(
                "Modules · vagues de dépendances",
                state.pane == Pane::Modules,
            ))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› "),
        panes[0],
        &mut module_state,
    );

    let resource_items: Vec<ListItem> = module
        .map(|current| {
            resources
                .iter()
                .map(|index| {
                    let resource = &current.resources[*index];
                    let (label, color) = change_label(resource.kind());
                    ListItem::new(vec![
                        Line::from(Span::styled(
                            label,
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        )),
                        Line::from(resource.address.clone()),
                    ])
                })
                .collect()
        })
        .unwrap_or_default();
    let mut resource_state = ListState::default();
    resource_state.select((!resources.is_empty()).then_some(state.resource));
    frame.render_stateful_widget(
        List::new(resource_items)
            .block(panel("Ressources", state.pane == Pane::Resources))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› "),
        panes[1],
        &mut resource_state,
    );

    let mut detail_lines = Vec::new();
    if let (Some(current), Some(index)) = (module, resources.get(state.resource)) {
        let resource = &current.resources[*index];
        let (label, color) = change_label(resource.kind());
        detail_lines.push(Line::from(Span::styled(
            format!("{label} · {}", resource.address),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
        detail_lines.push(Line::from(format!(
            "Actions: {}",
            resource.actions.join(", ")
        )));
        detail_lines.push(Line::from(""));
        if state.show_logs {
            detail_lines.push(Line::from(Span::styled(
                "JOURNAUX (masquage non garanti)",
                Style::default().fg(Color::Yellow),
            )));
            detail_lines.extend(current.logs.iter().map(|line| Line::from(line.as_str())));
        } else {
            match state.view {
                DetailView::BeforeAfter => {
                    detail_lines.push(Line::from(Span::styled(
                        "BEFORE",
                        Style::default().fg(Color::Red),
                    )));
                    detail_lines.extend(json_lines(&resource.before));
                    detail_lines.push(Line::from(Span::styled(
                        "AFTER",
                        Style::default().fg(Color::Green),
                    )));
                    detail_lines.extend(json_lines(&resource.after));
                }
                DetailView::Diff => detail_lines.extend(diff_lines(resource)),
            }
        }
    } else {
        detail_lines.push(Line::from(
            "Sélectionnez une ressource dans le panneau central.",
        ));
    }
    let title = if state.show_logs {
        "Détails · journaux"
    } else if state.view == DetailView::Diff {
        "Modifications · diff"
    } else {
        "Modifications · avant / après"
    };
    frame.render_widget(
        Paragraph::new(Text::from(detail_lines))
            .block(panel(title, state.pane == Pane::Changes))
            .wrap(Wrap { trim: false }),
        panes[2],
    );

    let totals_text = result.totals.as_ref().map_or_else(
        || "TOTAUX INCOMPLETS — résultat partiel".to_owned(),
        |totals| {
            format!(
                "ADD {} · CHANGE {} · DELETE {}",
                totals.added, totals.changed, totals.deleted
            )
        },
    );
    let search_label = if state.searching {
        format!("Recherche [{}] (Entrée/Esc pour terminer)", state.search)
    } else if state.search.is_empty() {
        "Recherche /".into()
    } else {
        format!("Filtre: {}", state.search)
    };
    let status_text = if result.cancelled {
        "ANNULE / INCOMPLET"
    } else if result.complete {
        "COMPLET"
    } else {
        "INCOMPLET / ILLISIBLE"
    };
    let footer = Line::from(vec![
        Span::styled(
            status_text,
            Style::default()
                .fg(if result.complete {
                    Color::Green
                } else {
                    Color::Red
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::raw(totals_text),
        Span::raw("  │  "),
        Span::raw(search_label),
    ]);
    frame.render_widget(Paragraph::new(vec![footer, Line::from("←/Échap retour · →/Tab avancer · ↑↓ choisir · d before/after↔diff · l journaux · q quitter")]), rows[1]);
}

fn panel(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
}

fn status_label(module: &ModuleResult) -> &'static str {
    match module.status.as_str() {
        "planned" => "PLANNED",
        "no_changes" => "NO CHANGE",
        "failed" => "FAILED",
        _ => "UNKNOWN",
    }
}

fn status_style(status: &str) -> Style {
    Style::default()
        .fg(match status {
            "planned" => Color::Green,
            "no_changes" => Color::DarkGray,
            "failed" => Color::Red,
            _ => Color::Magenta,
        })
        .add_modifier(Modifier::BOLD)
}

fn change_label(kind: ChangeKind) -> (&'static str, Color) {
    match kind {
        ChangeKind::Add => ("ADD", Color::Green),
        ChangeKind::Delete => ("DELETE", Color::Red),
        ChangeKind::Change => ("CHANGE", Color::Rgb(255, 165, 0)),
        ChangeKind::Replace => ("REPLACE", Color::Rgb(255, 165, 0)),
        ChangeKind::NoChange => ("NO CHANGE", Color::DarkGray),
    }
}

fn counter_line(totals: &Totals) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("ADD {} ", totals.added),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            format!("CHANGE {} ", totals.changed),
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
        Span::styled(
            format!("DELETE {}", totals.deleted),
            Style::default().fg(Color::Red),
        ),
    ])
}

fn json_lines(value: &serde_json::Value) -> Vec<Line<'static>> {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "[JSON illisible]".into())
        .lines()
        .map(|line| Line::from(line.to_owned()))
        .collect()
}

fn diff_lines(resource: &ResourceChange) -> Vec<Line<'static>> {
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    flatten("", &resource.before, &mut before);
    flatten("", &resource.after, &mut after);
    let mut keys: Vec<_> = before.keys().chain(after.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    let mut lines = Vec::new();
    for key in keys {
        let old = before.get(&key);
        let new = after.get(&key);
        if old == new {
            continue;
        }
        if let Some(value) = old {
            lines.push(Line::from(Span::styled(
                format!("- {key} = {value}"),
                Style::default().fg(Color::Red),
            )));
        }
        if let Some(value) = new {
            lines.push(Line::from(Span::styled(
                format!("+ {key} = {value}"),
                Style::default().fg(Color::Green),
            )));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from("Aucun champ avant/après distinct disponible."));
    }
    lines
}

fn flatten(prefix: &str, value: &serde_json::Value, output: &mut BTreeMap<String, String>) {
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&path, child, output);
            }
        }
        serde_json::Value::Array(values) if !values.is_empty() => {
            for (index, child) in values.iter().enumerate() {
                flatten(&format!("{prefix}[{index}]"), child, output);
            }
        }
        _ => {
            output.insert(prefix.to_owned(), value.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_left_navigates_three_panes_without_resetting_selection() {
        let mut state = ReviewState::default();
        state.handle(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 4, 3);
        state.handle(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), 4, 3);
        assert_eq!(state.pane, Pane::Resources);
        assert_eq!(state.module, 1);
        state.handle(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 4, 3);
        state.handle(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), 4, 3);
        assert_eq!(state.pane, Pane::Changes);
        assert_eq!(state.resource, 1);
        state.handle(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), 4, 3);
        assert_eq!(state.pane, Pane::Resources);
        assert_eq!(state.resource, 1);
    }

    #[test]
    fn diff_contains_only_changed_fields_and_labels_are_textual() {
        let resource = ResourceChange {
            address: "thing.example".into(),
            actions: vec!["update".into()],
            before: serde_json::json!({"keep":1,"change":"old"}),
            after: serde_json::json!({"keep":1,"change":"new"}),
        };
        let lines = diff_lines(&resource)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(lines.contains("- change = \"old\""));
        assert!(lines.contains("+ change = \"new\""));
        assert!(!lines.contains("keep"));
        assert_eq!(change_label(ChangeKind::Delete).0, "DELETE");
        assert_eq!(change_label(ChangeKind::Change).1, Color::Rgb(255, 165, 0));
        assert_eq!(change_label(ChangeKind::Replace).1, Color::Rgb(255, 165, 0));
        let counter = counter_line(&Totals {
            added: 2,
            changed: 3,
            deleted: 4,
        });
        let counters = counter.to_string();
        assert!(counters.contains("ADD 2"));
        assert!(counters.contains("CHANGE 3"));
        assert!(counters.contains("DELETE 4"));
        assert_eq!(counter.spans[0].style.fg, Some(Color::Green));
        assert_eq!(counter.spans[1].style.fg, Some(Color::Rgb(255, 165, 0)));
        assert_eq!(counter.spans[2].style.fg, Some(Color::Red));
    }

    #[test]
    fn renders_three_visible_panes_at_80_by_24() {
        let resource = ResourceChange {
            address: "demo_item.example".into(),
            actions: vec!["create".into()],
            before: serde_json::Value::Null,
            after: serde_json::json!({"name":"value"}),
        };
        let module = ModuleResult {
            path: "prod/app".into(),
            status: "planned".into(),
            wave: Some(0),
            totals: Some(Totals {
                added: 1,
                changed: 0,
                deleted: 0,
            }),
            resources: vec![resource],
            logs: vec![],
        };
        let result = RunResult {
            schema_version: 1,
            command: vec![
                "terragrunt".into(),
                "run".into(),
                "--all".into(),
                "plan".into(),
            ],
            exit_code: 0,
            complete: true,
            cancelled: false,
            totals: Some(Totals {
                added: 1,
                changed: 0,
                deleted: 0,
            }),
            modules: vec![module.clone()],
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                render_review(
                    frame,
                    &result,
                    &ReviewState::default(),
                    &[0],
                    Some(&module),
                    &[0],
                )
            })
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Modules"));
        assert!(text.contains("Ressources"));
        assert!(text.contains("Modifications"));
        assert!(text.contains("ADD"));
        assert!(text.contains("PLANNED"));
    }

    #[test]
    fn compact_module_context_keeps_status_and_name_at_80_by_24() {
        let module = ModuleResult {
            path: "prod/app".into(),
            status: "planned".into(),
            wave: Some(2),
            totals: Some(Totals::default()),
            resources: vec![],
            logs: vec![],
        };
        let result = RunResult {
            schema_version: 1,
            command: vec![],
            exit_code: 0,
            complete: true,
            cancelled: false,
            totals: Some(Totals::default()),
            modules: vec![module.clone()],
        };
        let state = ReviewState {
            pane: Pane::Changes,
            ..ReviewState::default()
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| render_review(frame, &result, &state, &[0], Some(&module), &[]))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("PLANNED"));
        assert!(text.contains("app"));
    }

    #[test]
    fn tab_advances_to_changes_without_wrapping_to_modules() {
        let mut state = ReviewState::default();
        state.handle(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), 1, 1);
        state.handle(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), 1, 1);
        assert_eq!(state.pane, Pane::Changes);
        state.handle(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), 1, 1);
        assert_eq!(state.pane, Pane::Changes);
    }

    #[test]
    fn module_listing_is_sorted_by_dependency_wave_and_search_is_case_insensitive() {
        let result = RunResult {
            schema_version: 1,
            command: vec![],
            exit_code: 0,
            complete: true,
            cancelled: false,
            totals: None,
            modules: vec![
                ModuleResult {
                    path: "z-service".into(),
                    status: "planned".into(),
                    wave: Some(1),
                    totals: None,
                    resources: vec![],
                    logs: vec![],
                },
                ModuleResult {
                    path: "a-network".into(),
                    status: "planned".into(),
                    wave: Some(0),
                    totals: None,
                    resources: vec![],
                    logs: vec![],
                },
            ],
        };
        assert_eq!(visible_modules(&result, "NET"), vec![1]);
        assert_eq!(visible_modules(&result, ""), vec![1, 0]);
    }
}
