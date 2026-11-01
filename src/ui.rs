use crate::diff::{self, Row, RowKind};
use crate::model::{ChangeKind, ModuleResult, ResourceChange, RunResult, Totals};
use crate::theme;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{io, io::Stdout, path::Path};

/// Largeur de la colonne Modules une fois réduite (bordures comprises).
const REDUCED_MODULES_WIDTH: u16 = 28;
/// Largeur de la colonne Ressources une fois réduite (bordures comprises).
const REDUCED_RESOURCES_WIDTH: u16 = 26;
const STATUS_WIDTH: usize = 10;
const PAGE: isize = 8;

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

fn paint_background(frame: &mut Frame) {
    frame.render_widget(Block::default().style(theme::base()), frame.area());
}

pub fn confirm_plan(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    root: &str,
    command: &str,
    units: &[crate::graph::Unit],
    workspace: &str,
) -> io::Result<bool> {
    let rows = plan_module_rows(Path::new(root), units);
    let mut selected = 0usize;
    let mut navigation_pacer = NavigationPacer::default();
    loop {
        if !event::poll(std::time::Duration::ZERO)? {
            terminal.draw(|frame| {
                render_plan_preview(frame, root, command, &rows, workspace, selected);
            })?;
        }
        if let Event::Key(key) = event::read()? {
            let steps = navigation_pacer.steps(key, std::time::Instant::now());
            if steps == 0 {
                continue;
            }
            match key.code {
                KeyCode::Enter => return Ok(true),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(false),
                KeyCode::Up => selected = selected.saturating_sub(steps),
                KeyCode::Down => {
                    selected = selected
                        .saturating_add(steps)
                        .min(rows.len().saturating_sub(1))
                }
                KeyCode::PageUp => selected = selected.saturating_sub(8),
                KeyCode::PageDown => {
                    selected = selected.saturating_add(8).min(rows.len().saturating_sub(1))
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = rows.len().saturating_sub(1),
                _ => {}
            }
        }
    }
}

fn plan_module_rows<'a>(
    root: &std::path::Path,
    units: &'a [crate::graph::Unit],
) -> Vec<(Option<usize>, &'a str)> {
    let waves = crate::graph::dependency_waves(root, units);
    let mut rows: Vec<_> = units
        .iter()
        .map(|unit| {
            let key = crate::graph::normalize(root, Path::new(&unit.path));
            (waves.get(&key).copied().flatten(), unit.path.as_str())
        })
        .collect();
    rows.sort_by(|left, right| {
        left.0
            .unwrap_or(usize::MAX)
            .cmp(&right.0.unwrap_or(usize::MAX))
            .then_with(|| left.1.cmp(right.1))
    });
    rows
}

fn render_plan_preview(
    frame: &mut Frame,
    root: &str,
    command: &str,
    rows: &[(Option<usize>, &str)],
    workspace: &str,
    selected: usize,
) {
    paint_background(frame);
    let height = frame.area().height;
    let header_height = height.saturating_sub(6).min(10);
    let footer_height = (height / 10).min(2);
    let module_height = height.saturating_sub(header_height + footer_height);
    let sections = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(module_height),
        Constraint::Length(footer_height),
    ])
    .split(frame.area());
    let known_waves = rows
        .iter()
        .filter_map(|(wave, _)| *wave)
        .collect::<std::collections::BTreeSet<_>>();
    let unknown_count = rows.iter().filter(|(wave, _)| wave.is_none()).count();
    let summary = Paragraph::new(vec![
        Line::from(Span::styled(
            "TERRAGRUNT TUI · PLAN TERRAGRUNT UNIQUEMENT",
            theme::title(),
        )),
        Line::from(format!("Racine : {root} · {} unités à lancer", rows.len())),
        Line::from(format!(
            "Vagues de dépendances : {} · unités à vague inconnue : {unknown_count}",
            known_waves.len()
        )),
        Line::from(format!("Fichiers temporaires : {workspace}")),
        Line::from(format!("Commande exacte après confirmation : {command}")),
        Line::from(Span::styled(
            "Effet : calcul de plans seulement; aucun apply/destroy.",
            theme::secondary(),
        )),
        Line::from(Span::styled(
            "VAGUE = niveau de dépendance; ce n'est pas un ordre runtime garanti.",
            theme::secondary(),
        )),
    ])
    .block(theme::panel("Avant exécution", false))
    .wrap(Wrap { trim: false });
    frame.render_widget(summary, sections[0]);

    let items: Vec<ListItem> = rows
        .iter()
        .map(|(wave, path)| {
            let label = wave.map_or_else(
                || "VAGUE INCONNUE".to_owned(),
                |number| format!("VAGUE {number}"),
            );
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{label:<16}"),
                    Style::default()
                        .fg(theme::wave_color(*wave))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(*path),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select((!items.is_empty()).then_some(selected.min(items.len().saturating_sub(1))));
    let title = format!(
        "Modules à lancer · {}/{}",
        selected.saturating_add(1).min(rows.len()),
        rows.len()
    );
    frame.render_stateful_widget(
        List::new(items)
            .block(theme::panel(&title, true))
            .highlight_style(theme::selection())
            .highlight_symbol("› "),
        sections[1],
        &mut state,
    );
    let mut hints = Vec::new();
    for (key, label) in [
        ("↑↓", "parcourir"),
        ("PgUp/PgDn", "page"),
        ("Home/End", "extrémités"),
        ("Entrée", "lancer"),
        ("q/Échap", "annuler"),
    ] {
        hints.extend(theme::key_hint(key, label));
    }
    frame.render_widget(Paragraph::new(Line::from(hints)), sections[2]);
}

pub fn confirm_cleanup(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    paths: &[String],
) -> io::Result<bool> {
    loop {
        terminal.draw(|frame| {
            paint_background(frame);
            let mut lines = vec![
                Line::from(Span::styled(
                    "RÉPERTOIRES TEMPORAIRES TERRAGRUNT TUI RESTANTS",
                    Style::default()
                        .fg(theme::CHANGE)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(
                    "Seuls les répertoires marqués comme appartenant à Terragrunt TUI sont listés.",
                ),
                Line::from(
                    "Confirmez leur suppression définitive. Aucune suppression automatique.",
                ),
                Line::from(""),
            ];
            lines.extend(paths.iter().map(|path| Line::from(path.clone())));
            lines.push(Line::from(""));
            let mut hints = theme::key_hint("y", "supprimer les répertoires listés").to_vec();
            hints.extend(theme::key_hint("autre touche", "conserver"));
            lines.push(Line::from(hints));
            frame.render_widget(
                Paragraph::new(Text::from(lines)).block(theme::panel("Nettoyage", true)),
                frame.area(),
            );
        })?;
        match event::read()? {
            Event::Key(key) if let Some(confirmed) = cleanup_confirmation(key) => {
                return Ok(confirmed);
            }
            _ => {}
        }
    }
}

fn cleanup_confirmation(key: KeyEvent) -> Option<bool> {
    (key.kind == KeyEventKind::Press).then_some(key.code == KeyCode::Char('y'))
}

pub fn show_progress(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    current: Option<&str>,
    completed: usize,
    total: usize,
) -> io::Result<bool> {
    terminal.draw(|frame| {
        paint_background(frame);
        let mut hints = vec![Span::styled("Journaux bruts masqués · ", theme::dim())];
        hints.extend(theme::key_hint(
            "q / Échap / Ctrl-C",
            "interrompre et nettoyer",
        ));
        let text = vec![
            Line::from(Span::styled("PLAN TERRAGRUNT EN COURS", theme::title())),
            Line::from("Mode lecture seule : run --all plan"),
            Line::from(format!(
                "Progression détectée : {completed}/{total} modules"
            )),
            Line::from(format!(
                "Module actif : {}",
                current.unwrap_or("en attente de sortie")
            )),
            Line::from(hints),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(text)).block(theme::panel("Exécution", true)),
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
    Diff,
    BeforeAfter,
}

/// Ordre des ressources : par statut (les plus risquées d'abord) ou dans l'ordre du plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceOrder {
    Status,
    Plan,
}

/// Tailles des listes visibles, pour borner les déplacements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Bounds {
    modules: usize,
    resources: usize,
    detail: usize,
}

#[derive(Debug)]
struct ReviewState {
    pane: Pane,
    module: usize,
    resource: usize,
    scroll: usize,
    view: DetailView,
    order: ResourceOrder,
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
            scroll: 0,
            view: DetailView::Diff,
            order: ResourceOrder::Status,
            show_logs: false,
            search: String::new(),
            searching: false,
            finished: false,
        }
    }
}

impl ReviewState {
    fn handle(&mut self, key: KeyEvent, bounds: Bounds) {
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
            KeyCode::Right | KeyCode::Tab => {
                self.pane = match self.pane {
                    Pane::Modules => Pane::Resources,
                    Pane::Resources | Pane::Changes => Pane::Changes,
                }
            }
            KeyCode::Up => self.step(-1, bounds),
            KeyCode::Down => self.step(1, bounds),
            KeyCode::PageUp => self.step(-PAGE, bounds),
            KeyCode::PageDown => self.step(PAGE, bounds),
            KeyCode::Home => self.step(isize::MIN / 2, bounds),
            KeyCode::End => self.step(isize::MAX / 2, bounds),
            KeyCode::Char('d') => {
                self.view = match self.view {
                    DetailView::BeforeAfter => DetailView::Diff,
                    DetailView::Diff => DetailView::BeforeAfter,
                };
                self.scroll = 0;
            }
            KeyCode::Char('l') => {
                self.show_logs = !self.show_logs;
                self.scroll = 0;
            }
            KeyCode::Char('s') => {
                self.order = match self.order {
                    ResourceOrder::Status => ResourceOrder::Plan,
                    ResourceOrder::Plan => ResourceOrder::Status,
                };
            }
            KeyCode::Char('/') => self.searching = true,
            _ => {}
        }
    }

    /// Déplace la sélection du panneau actif ; changer de module remet la ressource et le
    /// défilement à zéro, changer de ressource remet le défilement à zéro.
    fn step(&mut self, delta: isize, bounds: Bounds) {
        match self.pane {
            Pane::Modules => {
                let next = clamp_index(self.module, delta, bounds.modules);
                if next != self.module {
                    self.module = next;
                    self.resource = 0;
                    self.scroll = 0;
                }
            }
            Pane::Resources => {
                let next = clamp_index(self.resource, delta, bounds.resources);
                if next != self.resource {
                    self.resource = next;
                    self.scroll = 0;
                }
            }
            Pane::Changes => self.scroll = clamp_index(self.scroll, delta, bounds.detail),
        }
    }
}

fn clamp_index(current: usize, delta: isize, len: usize) -> usize {
    let last = len.saturating_sub(1);
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs()).min(last)
    } else {
        current.saturating_add(delta.unsigned_abs()).min(last)
    }
}

/// Transforme les flèches maintenues en déplacements qui accélèrent avec la durée d'appui ;
/// un appui isolé reste un pas unique.
#[derive(Default)]
struct NavigationPacer {
    last_event: Option<(KeyCode, std::time::Instant)>,
    held_since: Option<std::time::Instant>,
}

impl NavigationPacer {
    /// Nombre de pas à appliquer pour cet événement ; zéro l'ignore.
    fn steps(&mut self, key: KeyEvent, now: std::time::Instant) -> usize {
        if key.kind == KeyEventKind::Release {
            return 0;
        }
        if !matches!(key.code, KeyCode::Up | KeyCode::Down) {
            return usize::from(key.kind == KeyEventKind::Press);
        }

        let repeat_window = std::time::Duration::from_millis(150);
        let is_repeat = key.kind == KeyEventKind::Repeat
            || self.last_event.is_some_and(|(code, last)| {
                code == key.code && now.duration_since(last) < repeat_window
            });
        self.last_event = Some((key.code, now));
        if !is_repeat {
            self.held_since = Some(now);
            return 1;
        }
        let held = now
            .duration_since(*self.held_since.get_or_insert(now))
            .as_millis();
        // Un pas par répétition, puis un de plus toutes les 600 ms d'appui, jusqu'à 5.
        (1 + held / 600).min(5) as usize
    }
}

pub fn review(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    result: &RunResult,
) -> io::Result<()> {
    let mut state = ReviewState::default();
    let mut navigation_pacer = NavigationPacer::default();
    loop {
        let visible_modules = visible_modules(result, &state.search);
        state.module = state.module.min(visible_modules.len().saturating_sub(1));
        let module = visible_modules
            .get(state.module)
            .and_then(|index| result.modules.get(*index));
        let visible_resources = module
            .map(|item| visible_resources(item, &state.search, state.order))
            .unwrap_or_default();
        state.resource = state
            .resource
            .min(visible_resources.len().saturating_sub(1));
        let resource = module.and_then(|item| {
            visible_resources
                .get(state.resource)
                .map(|index| &item.resources[*index])
        });
        let detail = detail_lines(module, resource, &state);
        state.scroll = state.scroll.min(detail.body.len().saturating_sub(1));
        if state.finished {
            return Ok(());
        }
        // Les événements déjà en file sont traités avant de redessiner : une touche
        // maintenue ne laisse pas d'arriéré qui ferait défiler après le relâchement.
        if !event::poll(std::time::Duration::ZERO)? {
            terminal.draw(|frame| {
                render_review(
                    frame,
                    result,
                    &state,
                    &visible_modules,
                    module,
                    &visible_resources,
                    &detail,
                )
            })?;
        }
        if let Event::Key(key) = event::read()? {
            let bounds = Bounds {
                modules: visible_modules.len(),
                resources: visible_resources.len(),
                detail: detail.body.len(),
            };
            let order = state.order;
            for _ in 0..navigation_pacer.steps(key, std::time::Instant::now()) {
                state.handle(key, bounds);
            }
            if state.order != order
                && let (Some(item), Some(selected)) =
                    (module, visible_resources.get(state.resource))
            {
                state.resource = self::visible_resources(item, &state.search, state.order)
                    .iter()
                    .position(|index| index == selected)
                    .unwrap_or(0);
            }
        }
    }
}

/// Modules visibles, triés par vague puis modifiés d'abord puis chemin.
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
        (
            module.wave.unwrap_or(usize::MAX),
            module_rank(module),
            module.path.clone(),
        )
    });
    indices
}

fn has_changes(module: &ModuleResult) -> bool {
    module
        .totals
        .as_ref()
        .is_some_and(|totals| totals.added + totals.changed + totals.deleted > 0)
}

fn module_rank(module: &ModuleResult) -> u8 {
    match module.status.as_str() {
        "planned" if has_changes(module) => 0,
        "failed" => 1,
        "planned" | "no_changes" => 3,
        _ => 2,
    }
}

fn visible_resources(module: &ModuleResult, search: &str, order: ResourceOrder) -> Vec<usize> {
    let needle = search.to_lowercase();
    let mut indices: Vec<_> = module
        .resources
        .iter()
        .enumerate()
        .filter(|(_, resource)| resource.address.to_lowercase().contains(&needle))
        .map(|(index, _)| index)
        .collect();
    if order == ResourceOrder::Status {
        indices.sort_by_key(|index| kind_rank(module.resources[*index].kind()));
    }
    indices
}

/// Rang d'une ressource dans le tri par statut : destructions et remplacements d'abord,
/// ressources inchangées en dernier ; le tri est stable, l'ordre du plan départage.
fn kind_rank(kind: ChangeKind) -> u8 {
    match kind {
        ChangeKind::Delete => 0,
        ChangeKind::Replace => 1,
        ChangeKind::Change => 2,
        ChangeKind::Add => 3,
        ChangeKind::Unknown => 4,
        ChangeKind::Read => 5,
        ChangeKind::NoChange => 6,
    }
}

/// Contenu du panneau Modifications : un en-tête fixe et un corps qui défile.
#[derive(Debug, Default)]
struct Detail {
    header: Vec<Line<'static>>,
    body: Vec<Line<'static>>,
}

fn detail_lines(
    module: Option<&ModuleResult>,
    resource: Option<&ResourceChange>,
    state: &ReviewState,
) -> Detail {
    let Some(resource) = resource else {
        return Detail {
            header: vec![Line::from(Span::styled(
                "Sélectionnez une ressource.",
                theme::dim(),
            ))],
            body: Vec::new(),
        };
    };
    let kind = resource.kind();
    let color = theme::kind_color(kind);
    let mut header = vec![Line::from(vec![
        Span::styled(
            format!("{} ", kind_glyph(kind)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(resource.address.clone(), theme::title()),
    ])];
    if kind == ChangeKind::Replace {
        header.push(
            Line::from(Span::styled(
                " REMPLACEMENT · la ressource sera détruite puis recréée ",
                Style::default()
                    .fg(theme::REPLACE)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(theme::BAND_BG)),
        );
    }
    let mut meta = vec![Span::styled(
        format!("{} · ", resource.actions.join(", ")),
        theme::dim(),
    )];
    meta.extend(theme::key_hint(
        "d",
        match state.view {
            DetailView::Diff => "avant/après",
            DetailView::BeforeAfter => "diff",
        },
    ));
    header.push(Line::from(meta));
    header.push(Line::from(""));

    let body = if state.show_logs {
        let logs = module.map_or(&[][..], |item| item.logs.as_slice());
        if logs.is_empty() {
            vec![Line::from(Span::styled("Aucun journal.", theme::dim()))]
        } else {
            logs.iter()
                .map(|line| Line::from(Span::styled(line.clone(), theme::secondary())))
                .collect()
        }
    } else {
        match state.view {
            DetailView::Diff => diff_body(resource),
            DetailView::BeforeAfter => before_after_body(resource),
        }
    };
    Detail { header, body }
}

fn diff_body(resource: &ResourceChange) -> Vec<Line<'static>> {
    let rows = diff::unified(&resource.before, &resource.after);
    if rows.is_empty() {
        return vec![Line::from(Span::styled(
            "Aucune différence structurée disponible.",
            theme::dim(),
        ))];
    }
    rows.iter().map(diff_row_line).collect()
}

fn diff_row_line(row: &Row) -> Line<'static> {
    match row.kind {
        RowKind::Context => Line::from(vec![
            Span::raw("  "),
            Span::styled(row.text.clone(), Style::default().fg(theme::DIFF_CONTEXT)),
        ]),
        RowKind::Fold => Line::from(vec![
            Span::raw("  "),
            Span::styled(
                row.text.clone(),
                Style::default()
                    .fg(theme::DIFF_FOLD)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]),
        RowKind::Removed => changed_line(
            row,
            "- ",
            theme::DELETE,
            theme::DIFF_REMOVED_BG,
            theme::DIFF_REMOVED_HIGHLIGHT,
        ),
        RowKind::Added => changed_line(
            row,
            "+ ",
            theme::ADD,
            theme::DIFF_ADDED_BG,
            theme::DIFF_ADDED_HIGHLIGHT,
        ),
    }
}

fn changed_line(
    row: &Row,
    marker: &'static str,
    marker_color: ratatui::style::Color,
    background: ratatui::style::Color,
    highlight: ratatui::style::Color,
) -> Line<'static> {
    let text_style = Style::default().fg(theme::TEXT);
    let mut spans = vec![Span::styled(
        marker,
        Style::default()
            .fg(marker_color)
            .add_modifier(Modifier::BOLD),
    )];
    match row.highlight {
        Some((start, end)) if start < end => {
            spans.push(Span::styled(row.text[..start].to_owned(), text_style));
            spans.push(Span::styled(
                row.text[start..end].to_owned(),
                Style::default()
                    .bg(highlight)
                    .fg(theme::DIFF_HIGHLIGHT_FG)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(row.text[end..].to_owned(), text_style));
        }
        _ => spans.push(Span::styled(row.text.clone(), text_style)),
    }
    Line::from(spans).style(Style::default().bg(background))
}

fn before_after_body(resource: &ResourceChange) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "BEFORE",
        Style::default()
            .fg(theme::DELETE)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.extend(json_lines(&resource.before));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "AFTER",
        Style::default().fg(theme::ADD).add_modifier(Modifier::BOLD),
    )));
    lines.extend(json_lines(&resource.after));
    lines
}

fn json_lines(value: &serde_json::Value) -> Vec<Line<'static>> {
    let lines = diff::pretty_lines(value);
    if lines.is_empty() {
        return vec![Line::from(Span::styled("null", theme::dim()))];
    }
    lines
        .into_iter()
        .map(|line| Line::from(Span::styled(line, theme::secondary())))
        .collect()
}

fn render_review(
    frame: &mut Frame,
    result: &RunResult,
    state: &ReviewState,
    modules: &[usize],
    module: Option<&ModuleResult>,
    resources: &[usize],
    detail: &Detail,
) {
    paint_background(frame);
    let rows = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).split(frame.area());
    let panes: Vec<Rect> = match state.pane {
        Pane::Modules => {
            Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(rows[0])
                .to_vec()
        }
        Pane::Resources => Layout::horizontal([
            Constraint::Length(REDUCED_MODULES_WIDTH),
            Constraint::Fill(2),
            Constraint::Fill(3),
        ])
        .split(rows[0])
        .to_vec(),
        Pane::Changes => Layout::horizontal([
            Constraint::Length(REDUCED_MODULES_WIDTH),
            Constraint::Length(REDUCED_RESOURCES_WIDTH),
            Constraint::Fill(1),
        ])
        .split(rows[0])
        .to_vec(),
    };
    render_modules_pane(frame, panes[0], result, state, modules);
    render_resources_pane(frame, panes[1], state, module, resources);
    if let Some(area) = panes.get(2) {
        render_changes_pane(frame, *area, state, detail);
    }
    render_footer(
        frame,
        rows[1],
        result,
        state,
        modules.len(),
        detail.body.len(),
    );
}

struct ModuleColumns {
    name: usize,
    status: Option<usize>,
}

fn render_modules_pane(
    frame: &mut Frame,
    area: Rect,
    result: &RunResult,
    state: &ReviewState,
    modules: &[usize],
) {
    let full = state.pane == Pane::Modules;
    let title = format!("Modules · {}", modules.len());
    let block = theme::panel(&title, full);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let width = usize::from(inner.width);
    let counters_width = modules
        .iter()
        .filter_map(|index| result.modules[*index].totals.as_ref())
        .map(counters_width)
        .max()
        .unwrap_or(0)
        .max(8);
    let (list_area, columns) = if full {
        let name = width.saturating_sub(3 + 1 + STATUS_WIDTH + 1 + counters_width);
        let header = Line::from(vec![
            Span::raw("   "),
            Span::styled(pad("Path", name), theme::title()),
            Span::raw(" "),
            Span::styled(pad("Status", STATUS_WIDTH), theme::title()),
            Span::raw(" "),
            Span::styled("Δ", theme::title()),
        ]);
        let split = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
        frame.render_widget(header, split[0]);
        (
            split[1],
            ModuleColumns {
                name,
                status: Some(STATUS_WIDTH),
            },
        )
    } else {
        (
            inner,
            ModuleColumns {
                name: width.saturating_sub(2 + 1 + counters_width),
                status: None,
            },
        )
    };

    let mut items = Vec::new();
    let mut selection_rows = Vec::with_capacity(modules.len());
    for (position, index) in modules.iter().enumerate() {
        let item = &result.modules[*index];
        let starts_wave = position == 0 || result.modules[modules[position - 1]].wave != item.wave;
        let ends_wave = modules
            .get(position + 1)
            .is_none_or(|next| result.modules[*next].wave != item.wave);
        if starts_wave {
            items.push(ListItem::new(wave_heading(item.wave, full)));
        }
        selection_rows.push(items.len());
        items.push(ListItem::new(module_row(item, ends_wave, full, &columns)));
    }
    let mut list_state = ListState::default();
    list_state.select(selection_rows.get(state.module).copied());
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme::selection()),
        list_area,
        &mut list_state,
    );
}

fn wave_heading(wave: Option<usize>, full: bool) -> Line<'static> {
    let label = match (wave, full) {
        (Some(number), true) => format!("VAGUE {number}"),
        (None, true) => "VAGUE INCONNUE".to_owned(),
        (Some(number), false) => format!("V{number}"),
        (None, false) => "V?".to_owned(),
    };
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(theme::wave_color(wave))
            .add_modifier(Modifier::BOLD),
    ))
}

fn module_row(
    item: &ModuleResult,
    last_in_wave: bool,
    full: bool,
    columns: &ModuleColumns,
) -> Line<'static> {
    let guide = match (full, last_in_wave) {
        (true, false) => "├─ ",
        (true, true) => "└─ ",
        (false, false) => "├ ",
        (false, true) => "└ ",
    };
    let name = if full {
        item.path.as_str()
    } else {
        module_name(&item.path)
    };
    let mut spans = vec![
        Span::styled(guide, Style::default().fg(theme::wave_color(item.wave))),
        Span::styled(
            fit_left(name, columns.name),
            Style::default().fg(theme::TEXT),
        ),
        Span::raw(" "),
    ];
    if let Some(status_width) = columns.status {
        spans.push(Span::styled(
            pad(status_label(item), status_width),
            Style::default()
                .fg(theme::status_color(&item.status))
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    match &item.totals {
        Some(totals) => spans.extend(counters_spans(totals)),
        None => spans.push(Span::styled(
            status_glyph(&item.status),
            Style::default()
                .fg(theme::status_color(&item.status))
                .add_modifier(Modifier::BOLD),
        )),
    }
    Line::from(spans)
}

fn render_resources_pane(
    frame: &mut Frame,
    area: Rect,
    state: &ReviewState,
    module: Option<&ModuleResult>,
    resources: &[usize],
) {
    let focused = state.pane == Pane::Resources;
    let reduced = state.pane == Pane::Changes;
    let title = match (reduced, state.order) {
        (true, _) => format!("Ressources · {}", resources.len()),
        (false, ResourceOrder::Status) => format!("Ressources · {} · par statut", resources.len()),
        (false, ResourceOrder::Plan) => format!("Ressources · {} · ordre du plan", resources.len()),
    };
    let block = theme::panel(&title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let list_area = if reduced {
        inner
    } else {
        let split = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
        let path = module.map_or("", |item| item.path.as_str());
        frame.render_widget(
            Line::from(Span::styled(
                fit_left(path, usize::from(inner.width)),
                theme::secondary(),
            )),
            split[0],
        );
        split[1]
    };
    let Some(module) = module else {
        return;
    };
    if resources.is_empty() {
        frame.render_widget(
            Line::from(Span::styled("Aucune ressource.", theme::dim())),
            list_area,
        );
        return;
    }
    let items: Vec<ListItem> = resources
        .iter()
        .map(|index| {
            let resource = &module.resources[*index];
            let kind = resource.kind();
            let name = if reduced {
                resource_name(&resource.address)
            } else {
                resource.address.as_str()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", kind_glyph(kind)),
                    Style::default()
                        .fg(theme::kind_color(kind))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(name.to_owned(), Style::default().fg(theme::TEXT)),
            ]))
        })
        .collect();
    let mut list_state = ListState::default();
    list_state.select(Some(state.resource.min(resources.len() - 1)));
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme::selection()),
        list_area,
        &mut list_state,
    );
}

fn render_changes_pane(frame: &mut Frame, area: Rect, state: &ReviewState, detail: &Detail) {
    let title = if state.show_logs {
        "Journaux"
    } else if state.view == DetailView::Diff {
        "Modifications"
    } else {
        "Modifications · avant / après"
    };
    let block = theme::panel(title, state.pane == Pane::Changes);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let header_height = (detail.header.len() as u16).min(inner.height);
    let split =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).split(inner);
    render_lines(frame, split[0], &detail.header, 0);
    render_lines(frame, split[1], &detail.body, state.scroll);
}

/// Dessine chaque ligne sur sa propre rangée, pour que son style couvre toute la largeur.
fn render_lines(frame: &mut Frame, area: Rect, lines: &[Line<'static>], scroll: usize) {
    for (offset, line) in lines
        .iter()
        .skip(scroll)
        .take(usize::from(area.height))
        .enumerate()
    {
        let row = Rect {
            y: area.y + offset as u16,
            height: 1,
            ..area
        };
        frame.render_widget(line.clone(), row);
    }
}

fn render_footer(
    frame: &mut Frame,
    area: Rect,
    result: &RunResult,
    state: &ReviewState,
    module_count: usize,
    body_len: usize,
) {
    let mut hints: Vec<Span<'static>> = Vec::new();
    if state.searching {
        hints.push(Span::styled(
            format!("Recherche : {}▏  ", state.search),
            Style::default().fg(theme::KEY),
        ));
        hints.extend(theme::key_hint("Entrée/Échap", "terminer"));
    } else {
        let keys: &[(&str, &str)] = match state.pane {
            Pane::Modules => &[
                ("↑↓", "module"),
                ("→", "ressources"),
                ("/", "rechercher"),
                ("l", "journaux"),
                ("q", "quitter"),
            ],
            Pane::Resources => &[
                ("↑↓", "ressource"),
                ("←", "modules"),
                ("→", "diff"),
                ("d", "avant/après"),
                ("s", "tri"),
                ("/", "rechercher"),
                ("q", "quitter"),
            ],
            Pane::Changes => &[
                ("↑↓", "défiler"),
                ("←", "ressources"),
                ("d", "avant/après"),
                ("l", "journaux"),
                ("q", "quitter"),
            ],
        };
        for (key, label) in keys {
            hints.extend(theme::key_hint(key, label));
        }
        if state.pane == Pane::Changes && body_len > 0 {
            hints.push(Span::styled(
                format!("ligne {}/{body_len}", state.scroll + 1),
                theme::secondary(),
            ));
        }
    }

    let (status_text, status_color) = if result.cancelled {
        ("ANNULÉ / INCOMPLET", theme::DELETE)
    } else if result.complete {
        ("COMPLET", theme::ADD)
    } else {
        ("INCOMPLET / ILLISIBLE", theme::DELETE)
    };
    let mut summary = vec![
        Span::styled(
            status_text,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" · {module_count} modules · "), theme::secondary()),
    ];
    match &result.totals {
        Some(totals) => summary.extend(counters_spans(totals)),
        None => summary.push(Span::styled(
            "TOTAUX INCOMPLETS — résultat partiel",
            Style::default().fg(theme::UNKNOWN),
        )),
    }
    if !state.search.is_empty() && !state.searching {
        summary.push(Span::styled(
            format!(" · filtre « {} »", state.search),
            theme::secondary(),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(hints),
            Line::from(summary),
            legend_line(usize::from(area.width)),
        ]),
        area,
    );
}

/// Légende des symboles colorés ; les entrées se resserrent sur un terminal étroit.
fn legend_line(width: usize) -> Line<'static> {
    let entries = [
        (kind_glyph(ChangeKind::Add), theme::ADD, "créer"),
        (kind_glyph(ChangeKind::Change), theme::CHANGE, "modifier"),
        (kind_glyph(ChangeKind::Replace), theme::REPLACE, "remplacer"),
        (kind_glyph(ChangeKind::Delete), theme::DELETE, "détruire"),
        (kind_glyph(ChangeKind::Read), theme::READ, "lire"),
        (
            kind_glyph(ChangeKind::NoChange),
            theme::NO_CHANGES,
            "inchangé",
        ),
        (status_glyph("failed"), theme::FAILED, "échec"),
        (kind_glyph(ChangeKind::Unknown), theme::UNKNOWN, "inconnu"),
    ];
    let text_width: usize = entries
        .iter()
        .map(|(glyph, _, label)| glyph.chars().count() + 1 + label.chars().count())
        .sum();
    let gap = if text_width + 2 * (entries.len() - 1) <= width {
        "  "
    } else {
        " "
    };
    let mut spans = Vec::with_capacity(entries.len() * 3);
    for (position, (glyph, color, label)) in entries.into_iter().enumerate() {
        if position > 0 {
            spans.push(Span::raw(gap));
        }
        spans.push(Span::styled(
            glyph,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {label}"), theme::dim()));
    }
    Line::from(spans)
}

fn status_label(module: &ModuleResult) -> &'static str {
    match module.status.as_str() {
        "planned" => "PLANNED",
        "no_changes" => "NO CHANGES",
        "failed" => "FAILED",
        _ => "UNKNOWN",
    }
}

fn status_glyph(status: &str) -> &'static str {
    match status {
        "planned" | "no_changes" => "·",
        "failed" => "✗",
        _ => "?",
    }
}

fn kind_glyph(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Add => "+",
        ChangeKind::Delete => "-",
        ChangeKind::Change => "~",
        ChangeKind::Replace => "±",
        ChangeKind::Read => "○",
        ChangeKind::NoChange => "·",
        ChangeKind::Unknown => "?",
    }
}

fn counters_spans(totals: &Totals) -> Vec<Span<'static>> {
    let cell = |symbol: char, value: usize, color: ratatui::style::Color| {
        let style = if value == 0 {
            theme::dim()
        } else {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        };
        Span::styled(format!("{symbol}{value}"), style)
    };
    vec![
        cell('+', totals.added, theme::ADD),
        Span::raw(" "),
        cell('~', totals.changed, theme::CHANGE),
        Span::raw(" "),
        cell('-', totals.deleted, theme::DELETE),
    ]
}

fn counters_width(totals: &Totals) -> usize {
    format!("+{} ~{} -{}", totals.added, totals.changed, totals.deleted)
        .chars()
        .count()
}

fn module_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

fn resource_name(address: &str) -> &str {
    address.rsplit('.').next().unwrap_or(address)
}

/// Tronque par la gauche (la fin d'un chemin est la partie discriminante) ou complète à
/// droite pour occuper exactement `width` cellules.
fn fit_left(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return format!("{text}{}", " ".repeat(width - count));
    }
    if width == 0 {
        return String::new();
    }
    let tail: String = text.chars().skip(count - (width - 1)).collect();
    format!("…{tail}")
}

/// Complète à droite ou tronque par la droite pour occuper exactement `width` cellules.
fn pad(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count >= width {
        return text.chars().take(width).collect();
    }
    format!("{text}{}", " ".repeat(width - count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn module(
        path: &str,
        status: &str,
        wave: Option<usize>,
        totals: Option<(usize, usize, usize)>,
    ) -> ModuleResult {
        ModuleResult {
            path: path.into(),
            status: status.into(),
            wave,
            totals: totals.map(|(added, changed, deleted)| Totals {
                added,
                changed,
                deleted,
            }),
            resources: vec![],
            logs: vec![],
        }
    }

    fn run(modules: Vec<ModuleResult>) -> RunResult {
        RunResult {
            schema_version: 1,
            command: vec![],
            exit_code: 0,
            complete: true,
            cancelled: false,
            totals: None,
            modules,
        }
    }

    fn updated_resource() -> ResourceChange {
        ResourceChange {
            address: "thing.example".into(),
            actions: vec!["update".into()],
            before: serde_json::json!({"keep":1,"change":"old"}),
            after: serde_json::json!({"keep":1,"change":"new"}),
        }
    }

    fn screen_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        let width = usize::from(buffer.area.width);
        buffer
            .content()
            .chunks(width)
            .map(|line| line.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    fn draw(
        terminal: &mut Terminal<TestBackend>,
        result: &RunResult,
        state: &ReviewState,
        visible: &[usize],
        resources: &[usize],
    ) {
        let module = visible
            .get(state.module)
            .and_then(|index| result.modules.get(*index));
        let resource = module.and_then(|item| {
            resources
                .get(state.resource)
                .map(|index| &item.resources[*index])
        });
        let detail = detail_lines(module, resource, state);
        terminal
            .draw(|frame| render_review(frame, result, state, visible, module, resources, &detail))
            .unwrap();
    }

    fn bounds(modules: usize, resources: usize, detail: usize) -> Bounds {
        Bounds {
            modules,
            resources,
            detail,
        }
    }

    /// Colonne d'écran (en cellules, pas en octets) où commence `needle`.
    fn column(line: &str, needle: &str) -> u16 {
        let byte = line.find(needle).unwrap();
        line[..byte].chars().count() as u16
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn right_left_navigates_three_panes_without_resetting_selection() {
        let mut state = ReviewState::default();
        state.handle(key(KeyCode::Down), bounds(4, 3, 0));
        state.handle(key(KeyCode::Right), bounds(4, 3, 0));
        assert_eq!(state.pane, Pane::Resources);
        assert_eq!(state.module, 1);
        state.handle(key(KeyCode::Down), bounds(4, 3, 0));
        state.handle(key(KeyCode::Right), bounds(4, 3, 0));
        assert_eq!(state.pane, Pane::Changes);
        assert_eq!(state.resource, 1);
        state.handle(key(KeyCode::Left), bounds(4, 3, 0));
        assert_eq!(state.pane, Pane::Resources);
        assert_eq!(state.resource, 1);
    }

    #[test]
    fn changing_module_resets_resource_and_scroll_but_staying_put_does_not() {
        let mut state = ReviewState {
            pane: Pane::Resources,
            resource: 1,
            scroll: 5,
            ..ReviewState::default()
        };
        state.handle(key(KeyCode::Down), bounds(1, 3, 10));
        assert_eq!(
            state.scroll, 0,
            "changer de ressource remet le défilement à zéro"
        );
        state.pane = Pane::Modules;
        state.resource = 2;
        state.handle(key(KeyCode::Up), bounds(3, 3, 10));
        assert_eq!(state.resource, 2, "en butée, rien ne change");
        state.handle(key(KeyCode::Down), bounds(3, 3, 10));
        assert_eq!(state.module, 1);
        assert_eq!(state.resource, 0);
    }

    #[test]
    fn tab_advances_to_changes_without_wrapping_to_modules() {
        let mut state = ReviewState::default();
        state.handle(key(KeyCode::Tab), bounds(1, 1, 0));
        state.handle(key(KeyCode::Tab), bounds(1, 1, 0));
        assert_eq!(state.pane, Pane::Changes);
        state.handle(key(KeyCode::Tab), bounds(1, 1, 0));
        assert_eq!(state.pane, Pane::Changes);
    }

    #[test]
    fn changes_pane_scrolls_the_body_and_clamps_at_both_ends() {
        let mut state = ReviewState {
            pane: Pane::Changes,
            ..ReviewState::default()
        };
        for _ in 0..5 {
            state.handle(key(KeyCode::Down), bounds(1, 1, 3));
        }
        assert_eq!(state.scroll, 2);
        state.handle(key(KeyCode::Up), bounds(1, 1, 3));
        assert_eq!(state.scroll, 1);
        state.handle(key(KeyCode::PageUp), bounds(1, 1, 3));
        assert_eq!(state.scroll, 0);
        state.handle(key(KeyCode::End), bounds(1, 1, 40));
        assert_eq!(state.scroll, 39);
        state.handle(key(KeyCode::Home), bounds(1, 1, 40));
        assert_eq!(state.scroll, 0);
        state.handle(key(KeyCode::Char('d')), bounds(1, 1, 40));
        assert_eq!(state.view, DetailView::BeforeAfter);
    }

    #[test]
    fn review_opens_on_two_panes_and_the_third_appears_on_the_right() {
        let mut planned = module("prod/app", "planned", Some(0), Some((1, 0, 0)));
        planned.resources.push(ResourceChange {
            address: "demo_item.example".into(),
            actions: vec!["create".into()],
            before: serde_json::Value::Null,
            after: serde_json::json!({"name":"value"}),
        });
        let result = run(vec![planned]);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        draw(&mut terminal, &result, &ReviewState::default(), &[0], &[0]);
        let text = screen_lines(&terminal).join("\n");
        assert!(text.contains("Modules"));
        assert!(text.contains("Ressources"));
        assert!(!text.contains("Modifications"));
        assert!(text.contains("PLANNED"));
        assert!(text.contains("demo_item.example"));

        let state = ReviewState {
            pane: Pane::Resources,
            ..ReviewState::default()
        };
        draw(&mut terminal, &result, &state, &[0], &[0]);
        let text = screen_lines(&terminal).join("\n");
        assert!(text.contains("Modifications"));
        assert!(text.contains("+   \"name\": \"value\""));
    }

    #[test]
    fn background_covers_the_whole_screen() {
        let result = run(vec![module("prod/app", "planned", Some(0), None)]);
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        draw(&mut terminal, &result, &ReviewState::default(), &[0], &[]);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].bg, theme::BG);
        assert_eq!(buffer[(39, 9)].bg, theme::BG);
        assert_eq!(buffer[(20, 5)].bg, theme::BG);
    }

    #[test]
    fn module_listing_has_columns_wave_headings_and_tree_guides() {
        let result = run(vec![
            module("prod/db", "planned", Some(0), Some((1, 2, 0))),
            module("prod/cache", "no_changes", Some(0), Some((0, 0, 0))),
            module("prod/api", "failed", Some(1), None),
            module("prod/unknown", "unknown", None, None),
        ]);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        draw(
            &mut terminal,
            &result,
            &ReviewState::default(),
            &[0, 1, 2, 3],
            &[],
        );
        let lines = screen_lines(&terminal);
        let text = lines.join("\n");

        assert!(text.contains("Path"));
        assert!(text.contains("Status"));
        assert!(text.contains("Δ"));
        assert_eq!(text.matches("VAGUE 0").count(), 1);
        assert_eq!(text.matches("VAGUE 1").count(), 1);
        assert_eq!(text.matches("VAGUE INCONNUE").count(), 1);
        assert!(text.contains("├─ prod/db"));
        assert!(text.contains("└─ prod/cache"));
        assert!(text.contains("NO CHANGES"));
        assert!(text.contains("FAILED"));
        assert!(text.contains("+1 ~2 -0"));

        let buffer = terminal.backend().buffer();
        let heading_row = lines.iter().position(|l| l.contains("VAGUE 0")).unwrap();
        let selected_row = lines.iter().position(|l| l.contains("├─ prod/db")).unwrap();
        assert!(
            lines[heading_row].starts_with("┃VAGUE 0"),
            "{:?}",
            lines[heading_row]
        );
        assert_eq!(buffer[(2, heading_row as u16)].bg, theme::BG);
        assert_eq!(buffer[(2, selected_row as u16)].bg, theme::SELECTION);
    }

    #[test]
    fn reduced_module_listing_keeps_headings_names_and_counters() {
        let result = run(vec![module("prod/db", "planned", Some(2), Some((1, 2, 3)))]);
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let state = ReviewState {
            pane: Pane::Resources,
            ..ReviewState::default()
        };
        draw(&mut terminal, &result, &state, &[0], &[]);
        let text = screen_lines(&terminal).join("\n");

        assert!(!text.contains("VAGUE"));
        assert!(text.contains("V2"));
        assert!(text.contains("└ db"));
        assert!(text.contains("+1 ~2 -3"));
        assert!(text.contains("Aucune ressource."));
    }

    #[test]
    fn narrow_terminal_still_shows_three_panes_when_reviewing_changes() {
        let mut planned = module("prod/app", "planned", Some(0), Some((0, 1, 0)));
        planned.resources.push(updated_resource());
        let result = run(vec![planned]);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let state = ReviewState {
            pane: Pane::Changes,
            ..ReviewState::default()
        };
        draw(&mut terminal, &result, &state, &[0], &[0]);
        let text = screen_lines(&terminal).join("\n");
        assert!(text.contains("Modules"));
        assert!(text.contains("Ressources"));
        assert!(text.contains("Modifications"));
        assert!(text.contains("~ example"));
        assert!(text.contains("ligne 1/5"));
    }

    #[test]
    fn diff_rows_fill_the_width_and_highlight_the_changed_value() {
        let mut planned = module("prod/app", "planned", Some(0), Some((0, 1, 0)));
        planned.resources.push(updated_resource());
        let result = run(vec![planned]);
        let mut terminal = Terminal::new(TestBackend::new(110, 14)).unwrap();
        let state = ReviewState {
            pane: Pane::Changes,
            ..ReviewState::default()
        };
        draw(&mut terminal, &result, &state, &[0], &[0]);
        let lines = screen_lines(&terminal);
        let buffer = terminal.backend().buffer();

        let removed = lines
            .iter()
            .position(|line| line.contains("-   \"change\": \"old\""))
            .unwrap();
        let added = lines
            .iter()
            .position(|line| line.contains("+   \"change\": \"new\""))
            .unwrap();
        let context = lines
            .iter()
            .position(|line| line.contains("    \"keep\": 1"))
            .unwrap();
        let start = column(&lines[removed], "- ");
        assert_eq!(buffer[(start, removed as u16)].bg, theme::DIFF_REMOVED_BG);
        assert_eq!(buffer[(108, removed as u16)].bg, theme::DIFF_REMOVED_BG);
        assert_eq!(buffer[(108, added as u16)].bg, theme::DIFF_ADDED_BG);
        let old_value = column(&lines[removed], "\"old\"");
        assert_eq!(
            buffer[(old_value, removed as u16)].bg,
            theme::DIFF_REMOVED_HIGHLIGHT
        );
        let new_value = column(&lines[added], "\"new\"");
        assert_eq!(
            buffer[(new_value, added as u16)].bg,
            theme::DIFF_ADDED_HIGHLIGHT
        );
        let keep = column(&lines[context], "\"keep\"");
        assert_eq!(buffer[(keep, context as u16)].fg, theme::DIFF_CONTEXT);
        assert_eq!(buffer[(108, context as u16)].bg, theme::BG);
    }

    #[test]
    fn replacement_shows_a_band_and_single_sided_changes_have_one_marker() {
        let replaced = ResourceChange {
            address: "thing.replaced".into(),
            actions: vec!["delete".into(), "create".into()],
            before: serde_json::json!({"port": 8080}),
            after: serde_json::json!({"port": 8443}),
        };
        let detail = detail_lines(None, Some(&replaced), &ReviewState::default());
        let header = detail
            .header
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(header.contains("± thing.replaced"));
        assert!(header.contains("REMPLACEMENT"));
        assert!(header.contains("delete, create"));

        let created = ResourceChange {
            address: "thing.created".into(),
            actions: vec!["create".into()],
            before: serde_json::Value::Null,
            after: serde_json::json!({"name":"value"}),
        };
        let body = detail_lines(None, Some(&created), &ReviewState::default()).body;
        assert!(body.iter().all(|line| line.to_string().starts_with("+ ")));
        let deleted = ResourceChange {
            address: "thing.deleted".into(),
            actions: vec!["delete".into()],
            before: serde_json::json!({"name":"value"}),
            after: serde_json::Value::Null,
        };
        let body = detail_lines(None, Some(&deleted), &ReviewState::default()).body;
        assert!(body.iter().all(|line| line.to_string().starts_with("- ")));
    }

    #[test]
    fn before_after_view_and_logs_replace_the_diff_body() {
        let resource = updated_resource();
        let state = ReviewState {
            view: DetailView::BeforeAfter,
            ..ReviewState::default()
        };
        let text = detail_lines(None, Some(&resource), &state)
            .body
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.starts_with("BEFORE"));
        assert!(text.contains("AFTER"));
        assert!(text.contains("\"change\": \"old\""));

        let mut with_logs = module("prod/app", "planned", Some(0), None);
        with_logs.logs.push("hello from terragrunt".into());
        let state = ReviewState {
            show_logs: true,
            ..ReviewState::default()
        };
        let text = detail_lines(Some(&with_logs), Some(&resource), &state)
            .body
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(text, "hello from terragrunt");
    }

    #[test]
    fn modules_sort_by_wave_then_modified_first_then_path_and_search_ignores_case() {
        let result = run(vec![
            module("z-service", "planned", Some(1), Some((1, 0, 0))),
            module("a-network", "no_changes", Some(0), Some((0, 0, 0))),
            module("m-storage", "planned", Some(0), Some((0, 2, 0))),
            module("b-broken", "failed", Some(0), None),
        ]);
        assert_eq!(visible_modules(&result, ""), vec![2, 3, 1, 0]);
        assert_eq!(visible_modules(&result, "NET"), vec![1]);
    }

    #[test]
    fn status_labels_counters_and_truncation_are_textual() {
        assert_eq!(
            status_label(&module("x", "no_changes", None, None)),
            "NO CHANGES"
        );
        assert_eq!(status_label(&module("x", "weird", None, None)), "UNKNOWN");
        let spans = counters_spans(&Totals {
            added: 2,
            changed: 0,
            deleted: 4,
        });
        assert_eq!(spans[0].content, "+2");
        assert_eq!(spans[0].style.fg, Some(theme::ADD));
        assert_eq!(spans[2].style.fg, Some(theme::TEXT_DIM));
        assert_eq!(spans[4].style.fg, Some(theme::DELETE));
        assert_eq!(fit_left("live/prod/apps/app", 8), "…pps/app");
        assert_eq!(fit_left("app", 5), "app  ");
        assert_eq!(pad("Status", 4), "Stat");
        assert_eq!(module_name("live/prod/app/"), "app");
        assert_eq!(module_name(r"live\prod\app"), "app");
        assert_eq!(resource_name("module.x.aws_vpc.main"), "main");
    }

    #[test]
    fn preflight_screen_normalizes_dot_and_parent_path_segments_for_wave_lookup() {
        let units = vec![
            crate::graph::Unit {
                path: "./prod/api".into(),
                dependencies: vec!["../db".into()],
            },
            crate::graph::Unit {
                path: "./prod/db".into(),
                dependencies: vec![],
            },
        ];

        let rows = plan_module_rows(Path::new("/fixture"), &units);

        assert_eq!(rows, [(Some(0), "./prod/db"), (Some(1), "./prod/api")]);
    }

    #[test]
    fn preflight_screen_handles_compact_terminal_and_long_module_list() {
        let units: Vec<_> = (0..40)
            .map(|index| crate::graph::Unit {
                path: format!("prod/module-{index:02}"),
                dependencies: vec![],
            })
            .collect();
        let rows = plan_module_rows(Path::new("/fixture"), &units);
        let mut terminal = Terminal::new(TestBackend::new(60, 14)).unwrap();

        terminal
            .draw(|frame| {
                render_plan_preview(
                    frame,
                    "/fixture",
                    "terragrunt run --all plan",
                    &rows,
                    "temp-workspace",
                    39,
                )
            })
            .unwrap();

        let text = screen_lines(&terminal).join("\n");
        assert!(text.contains("40/40"));
        assert!(text.contains("prod/module-39"));
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, theme::BG);
    }

    #[cfg(windows)]
    #[test]
    fn preflight_screen_normalizes_windows_separators_and_dot_segments() {
        let units = vec![
            crate::graph::Unit {
                path: r".\prod\api".into(),
                dependencies: vec![r"..\db".into()],
            },
            crate::graph::Unit {
                path: r".\prod\db".into(),
                dependencies: vec![],
            },
        ];

        let rows = plan_module_rows(Path::new(r"C:\fixture"), &units);

        assert_eq!(rows, [(Some(0), r".\prod\db"), (Some(1), r".\prod\api")]);
    }

    #[test]
    fn preflight_screen_lists_all_units_by_wave_and_explains_wave_limits() {
        let units = vec![
            crate::graph::Unit {
                path: "prod/api".into(),
                dependencies: vec!["../db".into()],
            },
            crate::graph::Unit {
                path: "prod/db".into(),
                dependencies: vec![],
            },
            crate::graph::Unit {
                path: "prod/orphan".into(),
                dependencies: vec!["../missing".into()],
            },
        ];
        let rows = plan_module_rows(Path::new("/fixture"), &units);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| {
                render_plan_preview(
                    frame,
                    "/fixture",
                    "terragrunt run --all plan",
                    &rows,
                    "temp-workspace",
                    0,
                )
            })
            .unwrap();
        let text = screen_lines(&terminal).join("\n");
        assert!(text.contains("prod/db"));
        assert!(text.contains("prod/api"));
        assert!(text.contains("prod/orphan"));
        assert!(text.contains("VAGUE 0"));
        assert!(text.contains("VAGUE 1"));
        assert!(text.contains("VAGUE INCONNUE"));
        assert!(text.contains("pas un ordre runtime garanti"));
        assert!(text.contains("3 unités à lancer"));
    }

    #[test]
    fn cleanup_prompt_only_accepts_press_events() {
        let mut release_y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        release_y.kind = KeyEventKind::Release;
        let yes = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let other = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);

        assert_eq!(cleanup_confirmation(release_y), None);
        assert_eq!(cleanup_confirmation(yes), Some(true));
        assert_eq!(cleanup_confirmation(other), Some(false));
    }

    #[test]
    fn held_arrows_repeat_without_throttling_then_accelerate() {
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let start = std::time::Instant::now();
        let at = |millis| start + std::time::Duration::from_millis(millis);
        let mut pacer = NavigationPacer::default();

        assert_eq!(pacer.steps(key, start), 1);
        for elapsed in (33..=561).step_by(33) {
            assert_eq!(
                pacer.steps(key, at(elapsed)),
                1,
                "répétition à {elapsed} ms"
            );
        }
        assert_eq!(pacer.steps(key, at(600)), 2);
        let mut elapsed = 600;
        while elapsed < 3000 {
            elapsed += 33;
            pacer.steps(key, at(elapsed));
        }
        assert_eq!(pacer.steps(key, at(elapsed + 33)), 5);

        let mut release = key;
        release.kind = KeyEventKind::Release;
        assert_eq!(pacer.steps(release, at(elapsed + 34)), 0);
    }

    #[test]
    fn isolated_presses_and_non_navigation_commands_are_single_steps() {
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let start = std::time::Instant::now();
        let at = |millis| start + std::time::Duration::from_millis(millis);
        let mut pacer = NavigationPacer::default();

        assert_eq!(pacer.steps(key, start), 1);
        assert_eq!(pacer.steps(key, at(1000)), 1);
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(pacer.steps(tab, at(1001)), 1);

        let mut repeated = tab;
        repeated.kind = KeyEventKind::Repeat;
        assert_eq!(pacer.steps(repeated, at(1002)), 0);
        let mut release = tab;
        release.kind = KeyEventKind::Release;
        assert_eq!(pacer.steps(release, at(1003)), 0);
    }

    #[test]
    fn resources_sort_by_status_by_default_and_s_restores_plan_order() {
        let resource = |address: &str, actions: &[&str]| ResourceChange {
            address: address.into(),
            actions: actions.iter().map(|action| (*action).into()).collect(),
            ..updated_resource()
        };
        let mut item = module("app", "planned", Some(0), None);
        item.resources = vec![
            resource("a.created", &["create"]),
            resource("b.kept", &["no-op"]),
            resource("c.updated", &["update"]),
            resource("d.deleted", &["delete"]),
            resource("e.replaced", &["delete", "create"]),
        ];
        let addresses = |order| {
            visible_resources(&item, "", order)
                .into_iter()
                .map(|index| item.resources[index].address.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            addresses(ResourceOrder::Status),
            [
                "d.deleted",
                "e.replaced",
                "c.updated",
                "a.created",
                "b.kept"
            ]
        );
        assert_eq!(
            addresses(ResourceOrder::Plan),
            [
                "a.created",
                "b.kept",
                "c.updated",
                "d.deleted",
                "e.replaced"
            ]
        );

        let mut state = ReviewState::default();
        assert_eq!(state.order, ResourceOrder::Status);
        state.handle(key(KeyCode::Char('s')), bounds(1, 5, 0));
        assert_eq!(state.order, ResourceOrder::Plan);
    }

    #[test]
    fn footer_ends_with_a_legend_of_the_coloured_symbols() {
        let result = run(vec![module("app", "planned", Some(0), None)]);
        let visible = visible_modules(&result, "");
        for width in [80, 120] {
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
            draw(
                &mut terminal,
                &result,
                &ReviewState::default(),
                &visible,
                &[],
            );
            let lines = screen_lines(&terminal);
            let legend = lines.last().unwrap();
            for entry in [
                "+ créer",
                "~ modifier",
                "± remplacer",
                "- détruire",
                "○ lire",
                "· inchangé",
                "✗ échec",
                "? inconnu",
            ] {
                assert!(
                    legend.contains(entry),
                    "{width} colonnes : {legend:?} sans {entry:?}"
                );
            }
            let cell = &terminal.backend().buffer()[(column(legend, "- détruire"), 23)];
            assert_eq!(cell.fg, theme::DELETE);
        }
    }

    /// Rend un écran en HTML fidèle (cellules, couleurs, graisse) pour les captures.
    fn snapshot_html(terminal: &Terminal<TestBackend>, title: &str) -> String {
        use ratatui::style::Color;
        let css = |color: Color, fallback: &str| match color {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            _ => fallback.to_owned(),
        };
        let buffer = terminal.backend().buffer();
        let width = usize::from(buffer.area.width);
        let mut html = format!(
            "<h2 style=\"font:600 14px sans-serif;color:#a7b8c2;margin:28px 0 8px\">{title}</h2>\
             <pre style=\"background:#242b4a;color:#dfeded;font-family:'Cascadia Mono',Consolas,monospace;\
             font-size:13px;line-height:1.35;padding:6px 8px;display:inline-block;margin:0;border-radius:6px\">"
        );
        for row in buffer.content().chunks(width) {
            for cell in row {
                let weight = if cell.modifier.contains(Modifier::BOLD) {
                    "font-weight:600;"
                } else {
                    ""
                };
                let slant = if cell.modifier.contains(Modifier::ITALIC) {
                    "font-style:italic;"
                } else {
                    ""
                };
                let symbol = cell
                    .symbol()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                html.push_str(&format!(
                    "<span style=\"color:{};background:{};{weight}{slant}\">{symbol}</span>",
                    css(cell.fg, "#dfeded"),
                    css(cell.bg, "#242b4a"),
                ));
            }
            html.push('\n');
        }
        html.push_str("</pre>");
        html
    }

    #[test]
    #[ignore = "exporte des captures HTML ; lancer avec --ignored et TTUI_SNAPSHOT_DIR=<dossier>"]
    fn export_review_snapshots() {
        let Ok(dir) = std::env::var("TTUI_SNAPSHOT_DIR") else {
            return;
        };
        let result: RunResult =
            serde_json::from_str(include_str!("../docs/specs/fixtures/demo-run.json")).unwrap();
        let visible = visible_modules(&result, "");
        let app = visible
            .iter()
            .position(|index| result.modules[*index].path.ends_with("prod/apps/app"))
            .unwrap();
        let app_module = &result.modules[visible[app]];
        let replaced = visible_resources(app_module, "", ResourceOrder::Status)
            .iter()
            .position(|index| app_module.resources[*index].kind() == ChangeKind::Replace)
            .unwrap();
        let mut html = String::from(
            "<!doctype html><meta charset=\"utf-8\"><title>Captures revue</title>\
             <body style=\"background:#141a33;margin:0;padding:24px\">",
        );
        let states = [
            (
                "État 1 — focus Modules",
                ReviewState {
                    module: app,
                    ..ReviewState::default()
                },
            ),
            (
                "État 2 — focus Ressources",
                ReviewState {
                    pane: Pane::Resources,
                    module: app,
                    resource: replaced,
                    ..ReviewState::default()
                },
            ),
            (
                "État 3 — focus Modifications",
                ReviewState {
                    pane: Pane::Changes,
                    module: app,
                    resource: replaced,
                    ..ReviewState::default()
                },
            ),
        ];
        for (title, state) in states {
            let resources = result
                .modules
                .get(visible[state.module])
                .map(|module| visible_resources(module, "", ResourceOrder::Status))
                .unwrap_or_default();
            let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
            draw(&mut terminal, &result, &state, &visible, &resources);
            html.push_str(&snapshot_html(&terminal, title));
        }
        std::fs::write(std::path::Path::new(&dir).join("captures-revue.html"), html).unwrap();
    }
}
