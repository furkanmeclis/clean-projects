//! Rendering for the terminal UI.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph, Row, Table, TableState,
    Tabs, Wrap,
};
use ratatui::Frame;

use cleanprojects_core::fsutil::{compact_path, ellipsize_end, ellipsize_middle, human_size};
use cleanprojects_core::{Candidate, Category, Risk};

use super::theme;
use super::{mode_label, App, Modal, Sort, Tab, ViewFilter};
use cleanprojects_core::cleaner::CleanMode;
use cleanprojects_core::VolumeKind;

pub const SETTINGS_ROWS: usize = 12;
const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let [header, tabs, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .areas(area);
    draw_header(f, app, header);
    draw_tabs(f, app, tabs);
    match app.tab {
        Tab::Overview => draw_overview(f, app, body),
        Tab::Candidates | Tab::Git => draw_table(f, app, body),
        Tab::Explore => draw_explore(f, app, body),
        Tab::History => draw_history(f, app, body),
        Tab::Settings => draw_settings(f, app, body),
    }
    draw_footer(f, app, footer);
    match &app.modal {
        Some(Modal::Confirm) => draw_confirm(f, app, area),
        Some(Modal::Cleaning) => draw_cleaning(f, app, area),
        Some(Modal::Done(entry)) => draw_done(f, app, area, entry),
        Some(Modal::Help) => draw_help(f, app, area),
        None => {}
    }
}

fn block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .title(Span::styled(format!(" {} ", title.into()), theme::bold()))
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let (n, sel) = app.selected_total();
    let recoverable: u64 = app
        .candidates
        .iter()
        .filter(|c| !app.env.state.excluded.contains(&c.id))
        .map(|c| c.size)
        .sum();
    let [left, mid, right] = Layout::horizontal([
        Constraint::Length(30),
        Constraint::Min(20),
        Constraint::Length(44),
    ])
    .areas(area);
    let brand = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(" ◆ CleanProjects", theme::accent_bold()),
            Span::styled(format!(" v{}", cleanprojects_core::VERSION), theme::muted()),
        ]),
        Line::from(vec![Span::styled(
            format!(" {} {}  ", s.recoverable, human_size(recoverable)),
            theme::bold(),
        )]),
        Line::from(vec![Span::styled(
            format!(" {} {} · {}", s.selected, n, human_size(sel)),
            theme::accent(),
        )]),
    ]);
    f.render_widget(brand, left);
    let status = if app.scanning {
        let spin = SPINNER[(app.tick / 2) % SPINNER.len()];
        Line::from(vec![
            Span::styled(format!("{spin} {} ", s.scanning), theme::accent()),
            Span::styled(app.scan_phase.clone(), theme::bold()),
            Span::raw(" "),
            Span::styled(
                ellipsize_middle(&app.scan_msg, mid.width.saturating_sub(30) as usize),
                theme::muted(),
            ),
        ])
    } else if let Some((p, r, ms)) = app.scan_stats {
        Line::from(vec![
            Span::styled(
                format!("✓ {}  ", s.scan_done),
                Style::default().fg(theme::GOOD),
            ),
            Span::styled(
                format!(
                    "{} {}  {p} projects  {r} repos  {} {}  {:.1}s{}",
                    app.candidates.len(),
                    s.candidates.to_lowercase(),
                    app.guarded,
                    s.guarded,
                    ms as f64 / 1000.0,
                    if app.cached {
                        format!("  ({})", s.cached)
                    } else {
                        String::new()
                    }
                ),
                theme::muted(),
            ),
        ])
    } else {
        Line::from("")
    };
    let mid_block = Paragraph::new(vec![Line::from(""), status]).alignment(Alignment::Center);
    f.render_widget(mid_block, mid);
    let used = app.disk.used_percent();
    let gauge = Gauge::default()
        .block(Block::default().title(Span::styled(
            format!(
                " {} {} / {} ",
                s.free_space,
                human_size(app.disk.free),
                human_size(app.disk.total)
            ),
            theme::muted(),
        )))
        .gauge_style(Style::default().fg(if used > 90.0 {
            theme::BAD
        } else if used > 75.0 {
            theme::WARN
        } else {
            theme::MUTED
        }))
        .ratio((used / 100.0).clamp(0.0, 1.0))
        .label(format!("{used:.0}% used"));
    let [_, g] = Layout::vertical([Constraint::Length(0), Constraint::Length(2)]).areas(right);
    f.render_widget(gauge, g);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let git_n = app
        .candidates
        .iter()
        .filter(|c| c.category == Category::Git && !app.env.state.excluded.contains(&c.id))
        .count();
    let n = app
        .candidates
        .iter()
        .filter(|c| !app.env.state.excluded.contains(&c.id))
        .count();
    let titles = vec![
        format!(" 1 {} ", s.tab_overview),
        format!(" 2 {} ({n}) ", s.tab_candidates),
        format!(" 3 {} ({git_n}) ", s.tab_git),
        format!(" 4 {} ", s.tab_explore),
        format!(" 5 {} ({}) ", s.tab_history, app.env.state.history.len()),
        format!(" 6 {} ", s.tab_settings),
    ];
    let tabs = Tabs::new(titles)
        .select(Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0))
        .style(theme::muted())
        .highlight_style(theme::accent_bold().add_modifier(Modifier::UNDERLINED))
        .divider("");
    f.render_widget(tabs, area);
}

fn draw_overview(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let excluded = &app.env.state.excluded;
    let active: Vec<&Candidate> = app
        .candidates
        .iter()
        .filter(|c| !excluded.contains(&c.id))
        .collect();
    let [top, bottom] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
    let [cat_area, largest_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(top);

    // category bars
    let mut by_cat: Vec<(Category, u64)> = Category::ALL
        .iter()
        .map(|cat| {
            (
                *cat,
                active
                    .iter()
                    .filter(|c| c.category == *cat)
                    .map(|c| c.size)
                    .sum(),
            )
        })
        .filter(|(_, v)| *v > 0)
        .collect();
    by_cat.sort_by_key(|x| std::cmp::Reverse(x.1));
    let max = by_cat.first().map(|x| x.1).unwrap_or(1).max(1);
    let inner_w = cat_area.width.saturating_sub(4) as usize;
    let bar_w = inner_w.saturating_sub(12 + 10 + 2);
    let lines: Vec<Line> = by_cat
        .iter()
        .map(|(cat, v)| {
            let filled = ((*v as f64 / max as f64) * bar_w as f64).round() as usize;
            Line::from(vec![
                Span::styled(format!("{:<11} ", cat.short()), theme::muted()),
                Span::styled(
                    "█".repeat(filled),
                    Style::default().fg(theme::category(*cat)),
                ),
                Span::styled("░".repeat(bar_w.saturating_sub(filled)), theme::muted()),
                Span::styled(format!(" {:>9}", human_size(*v)), theme::bold()),
            ])
        })
        .collect();
    let lines = if lines.is_empty() {
        vec![Line::from(Span::styled(
            if app.scanning {
                s.scanning
            } else {
                s.no_candidates
            },
            theme::muted(),
        ))]
    } else {
        lines
    };
    f.render_widget(Paragraph::new(lines).block(block(s.by_category)), cat_area);

    // largest
    let mut largest = active.clone();
    largest.sort_by_key(|c| std::cmp::Reverse(c.size));
    let items: Vec<ListItem> = largest
        .iter()
        .take(largest_area.height.saturating_sub(2) as usize)
        .map(|c| {
            let w = largest_area.width.saturating_sub(4) as usize;
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:>9} ", human_size(c.size)), theme::accent_bold()),
                Span::styled(ellipsize_end(&c.label, w.saturating_sub(24)), theme::bold()),
                Span::styled(format!("  {}", c.category.short()), theme::muted()),
            ]))
        })
        .collect();
    f.render_widget(List::new(items).block(block(s.largest)), largest_area);

    // bottom: protected + roots + tips
    let ctx = app.ctx();
    let [prot, roots, tips] = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(30),
        Constraint::Percentage(30),
    ])
    .areas(bottom);
    let prot_lines: Vec<Line> = ctx
        .protected
        .iter()
        .filter(|p| p.exists())
        .map(|p| {
            Line::from(vec![
                Span::styled("🔒 ", theme::muted()),
                Span::raw(compact_path(p, &app.env.paths.home)),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(prot_lines)
            .block(block(s.protected))
            .wrap(Wrap { trim: true }),
        prot,
    );
    let mut root_lines: Vec<Line> = Vec::new();
    for v in &app.volumes {
        let w = roots.width.saturating_sub(4) as usize;
        let bar_w = w.saturating_sub(22).clamp(4, 24);
        let filled = ((v.used_percent() / 100.0) * bar_w as f64).round() as usize;
        let kind = match v.kind {
            VolumeKind::System => "⌂",
            VolumeKind::External => "▣",
            VolumeKind::Network => "☁",
        };
        root_lines.push(Line::from(vec![
            Span::styled(
                format!("{kind} {:<10}", ellipsize_end(&v.name, 10)),
                theme::bold(),
            ),
            Span::styled(
                "█".repeat(filled),
                Style::default().fg(if v.used_percent() > 90.0 {
                    theme::BAD
                } else {
                    theme::ACCENT
                }),
            ),
            Span::styled("░".repeat(bar_w.saturating_sub(filled)), theme::muted()),
            Span::styled(format!(" {} free", human_size(v.free)), theme::muted()),
        ]));
    }
    root_lines.push(Line::from(""));
    for p in &ctx.roots {
        root_lines.push(Line::from(Span::styled(
            compact_path(p, &app.env.paths.home),
            theme::muted(),
        )));
    }
    f.render_widget(
        Paragraph::new(root_lines).block(block(format!("{} · {}", s.volumes, s.setting_roots))),
        roots,
    );
    let stale = active.iter().filter(|c| c.is_stale()).count();
    let tips_lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", s.stale_projects), theme::muted()),
            Span::styled(stale.to_string(), theme::bold()),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", s.excluded), theme::muted()),
            Span::styled(excluded.len().to_string(), theme::bold()),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", s.recovery_bin), theme::muted()),
            Span::styled(
                human_size(app.env.paths.load_journal().total_size()),
                theme::bold(),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(s.keys_global, theme::muted())),
    ];
    f.render_widget(
        Paragraph::new(tips_lines)
            .block(block("ℹ"))
            .wrap(Wrap { trim: true }),
        tips,
    );
}

fn draw_table(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let rows_src = app.visible_all();
    let cursor = if app.tab == Tab::Git {
        app.git_cursor
    } else {
        app.cursor
    };
    let [toolbar, table_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);

    // toolbar: search + filters
    let mut spans = vec![Span::styled(" / ", theme::accent_bold())];
    if app.searching || !app.search.is_empty() {
        spans.push(Span::styled(format!("{}▏", app.search), theme::bold()));
    } else {
        spans.push(Span::styled(s.search_placeholder, theme::muted()));
    }
    spans.push(Span::raw("   "));
    let risk_label = match app.risk_filter {
        None => s.filter_all.to_string(),
        Some(r) => r.as_str().to_string(),
    };
    spans.push(Span::styled("f ", theme::accent()));
    spans.push(Span::styled(
        format!("risk:{risk_label}"),
        if app.risk_filter.is_some() {
            theme::bold()
        } else {
            theme::muted()
        },
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("c ", theme::accent()));
    spans.push(Span::styled(
        format!(
            "cat:{}",
            app.category_filter
                .map(|c| c.short())
                .unwrap_or(s.filter_all)
        ),
        if app.category_filter.is_some() {
            theme::bold()
        } else {
            theme::muted()
        },
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("v ", theme::accent()));
    let vf = match app.view_filter {
        ViewFilter::All => s.filter_all,
        ViewFilter::Stale => s.filter_stale,
        ViewFilter::Excluded => s.filter_excluded,
    };
    spans.push(Span::styled(
        format!("view:{vf}"),
        if app.view_filter != ViewFilter::All {
            theme::bold()
        } else {
            theme::muted()
        },
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("s ", theme::accent()));
    let so = match app.sort {
        Sort::Size => s.sort_size,
        Sort::Name => s.sort_name,
        Sort::Age => s.sort_age,
        Sort::Risk => s.sort_risk,
    };
    spans.push(Span::styled(format!("sort:{so}"), theme::muted()));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("t ", theme::accent()));
    spans.push(Span::styled(
        app.mode.as_str(),
        match app.mode {
            CleanMode::Permanent => Style::default().fg(theme::BAD),
            _ => Style::default().fg(theme::GOOD),
        },
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), toolbar);

    let (list_area, detail_area) =
        if app.show_detail && !rows_src.is_empty() && table_area.width > 90 {
            let [l, d] =
                Layout::horizontal([Constraint::Min(50), Constraint::Length(42)]).areas(table_area);
            (l, Some(d))
        } else {
            (table_area, None)
        };

    let home = &app.env.paths.home;
    let label_w = list_area.width.saturating_sub(3 + 12 + 8 + 6 + 10 + 10) as usize;
    let rows: Vec<Row> = rows_src
        .iter()
        .map(|c| {
            let sel = app.selected.contains(&c.id);
            let ex = app.env.state.excluded.contains(&c.id);
            let mark = if sel {
                Span::styled("●", theme::accent_bold())
            } else if ex {
                Span::styled("⊘", theme::muted())
            } else {
                Span::styled("○", theme::muted())
            };
            let mut label = c.label.clone();
            if c.is_stale() {
                label.push_str(" ⏳");
            }
            if let Some(app_name) = &c.blocked_by {
                label.push_str(&format!(" ⛔{app_name}"));
            }
            let name_style = if ex {
                theme::muted()
            } else if sel {
                theme::accent_bold()
            } else {
                theme::bold()
            };
            let risk_label = match c.risk {
                Risk::Low => s.risk_low,
                Risk::Medium => s.risk_medium,
                Risk::High => s.risk_high,
            };
            let size = if c.size == 0 {
                "—".to_string()
            } else if c.size_exact {
                human_size(c.size)
            } else {
                format!("~{}", human_size(c.size))
            };
            let age = c
                .age_days()
                .map(|d| {
                    if d == 0 {
                        "today".to_string()
                    } else {
                        format!("{d}d")
                    }
                })
                .unwrap_or_else(|| "—".into());
            Row::new(vec![
                Line::from(mark),
                Line::from(vec![
                    Span::styled(ellipsize_end(&label, label_w), name_style),
                    Span::styled(
                        format!(
                            "  {}",
                            ellipsize_middle(
                                &compact_path(std::path::Path::new(&c.detail()), home),
                                label_w.saturating_sub(label.len() + 2)
                            )
                        ),
                        theme::muted(),
                    ),
                ]),
                Line::from(Span::styled(
                    c.category.short(),
                    Style::default().fg(theme::category(c.category)),
                )),
                Line::from(Span::styled(risk_label, theme::risk(c.risk))),
                Line::from(Span::styled(age, theme::muted())),
                Line::from(Span::styled(size, theme::bold())).alignment(Alignment::Right),
            ])
        })
        .collect();
    let total: u64 = rows_src.iter().map(|c| c.size).sum();
    let title = format!(
        "{} · {} {} · {}",
        if app.tab == Tab::Git {
            s.tab_git
        } else {
            s.tab_candidates
        },
        rows_src.len(),
        s.items,
        human_size(total)
    );
    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Min(20),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec![
            "",
            s.col_name,
            s.col_category,
            s.col_risk,
            s.col_age,
            s.col_size,
        ])
        .style(theme::header()),
    )
    .block(block(title))
    .row_highlight_style(theme::cursor())
    .column_spacing(1);
    let mut state = TableState::default();
    if !rows_src.is_empty() {
        state.select(Some(cursor.min(rows_src.len() - 1)));
    }
    f.render_stateful_widget(table, list_area, &mut state);
    if rows_src.is_empty() {
        let msg = if app.scanning {
            s.scanning
        } else if app.tab == Tab::Git && !app.env.config.git.enabled {
            s.git_disabled
        } else if app.tab == Tab::Git {
            s.git_empty
        } else {
            s.no_candidates
        };
        let p = Paragraph::new(Span::styled(msg, theme::muted())).alignment(Alignment::Center);
        let inner = Rect {
            x: list_area.x + 2,
            y: list_area.y + list_area.height / 2,
            width: list_area.width.saturating_sub(4),
            height: 1,
        };
        f.render_widget(p, inner);
    }
    if let (Some(d), Some(c)) = (detail_area, rows_src.get(cursor)) {
        draw_detail(f, app, d, c);
    }
}

fn draw_detail(f: &mut Frame, app: &App, area: Rect, c: &Candidate) {
    let s = app.s();
    let home = &app.env.paths.home;
    let mut lines = vec![
        Line::from(Span::styled(c.label.clone(), theme::accent_bold())),
        Line::from(vec![
            Span::styled(
                c.category.short(),
                Style::default().fg(theme::category(c.category)),
            ),
            Span::raw("  "),
            Span::styled(c.risk.as_str(), theme::risk(c.risk)),
            Span::raw("  "),
            Span::styled(human_size(c.size), theme::bold()),
            Span::styled(if c.size_exact { "" } else { " ~" }, theme::muted()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            compact_path(std::path::Path::new(&c.detail()), home),
            theme::muted(),
        )),
        Line::from(""),
        Line::from(c.description.clone()),
    ];
    if let Some(p) = &c.project {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("project ", theme::muted()),
            Span::raw(format!(
                "{} ({}) idle {}d",
                p.name,
                p.kind.as_str(),
                p.idle_days
            )),
        ]));
    }
    if let Some(m) = c.modified {
        lines.push(Line::from(vec![
            Span::styled("modified ", theme::muted()),
            Span::raw(m.format("%Y-%m-%d %H:%M").to_string()),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("id ", theme::muted()),
        Span::raw(c.id.clone()),
    ]));
    if app.env.state.excluded.contains(&c.id) {
        lines.push(Line::from(Span::styled(
            s.excluded,
            Style::default().fg(theme::WARN),
        )));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block(s.detail))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_history(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let hist = &app.env.state.history;
    if hist.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(s.history_empty, theme::muted()))
                .alignment(Alignment::Center)
                .block(block(s.tab_history)),
            area,
        );
        return;
    }
    let [list_area, items_area] =
        Layout::horizontal([Constraint::Length(46), Constraint::Min(30)]).areas(area);
    let rows: Vec<Row> = hist
        .iter()
        .rev()
        .map(|h| {
            let ok = h.items.iter().filter(|i| i.ok).count();
            let failed = h.items.len() - ok;
            let restorable = h.restorable();
            Row::new(vec![
                Line::from(format!(
                    "{}{}",
                    h.at.format("%Y-%m-%d %H:%M"),
                    if restorable > 0 { " ↺" } else { "" }
                )),
                Line::from(Span::styled(human_size(h.freed), theme::accent_bold()))
                    .alignment(Alignment::Right),
                Line::from(Span::styled(
                    format!("{ok}✓ {failed}✗{}", if h.dry_run { " dry" } else { "" }),
                    theme::muted(),
                )),
            ])
        })
        .collect();
    let total = app.env.state.total_freed();
    let table = Table::new(
        rows,
        [
            Constraint::Length(17),
            Constraint::Length(10),
            Constraint::Min(10),
        ],
    )
    .block(block(format!(
        "{} · {} {}",
        s.tab_history,
        human_size(total),
        s.freed
    )))
    .row_highlight_style(theme::cursor());
    let mut st = TableState::default();
    st.select(Some(app.history_cursor.min(hist.len() - 1)));
    f.render_stateful_widget(table, list_area, &mut st);
    let entry = &hist[hist.len() - 1 - app.history_cursor.min(hist.len() - 1)];
    let lines: Vec<Line> = entry
        .items
        .iter()
        .map(|i| {
            Line::from(vec![
                Span::styled(
                    if i.ok { "✓ " } else { "✗ " },
                    Style::default().fg(if i.ok { theme::GOOD } else { theme::BAD }),
                ),
                Span::styled(format!("{:>9} ", human_size(i.size)), theme::bold()),
                Span::raw(i.label.clone()),
                Span::styled(
                    format!("  {}", i.error.clone().unwrap_or_default()),
                    Style::default().fg(theme::BAD),
                ),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines)
            .block(block(format!("{} {}", entry.items.len(), s.items)))
            .wrap(Wrap { trim: true }),
        items_area,
    );
}

fn draw_settings(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let cfg = &app.env.config;
    let onoff = |b: bool| {
        if b {
            Span::styled("● on ", Style::default().fg(theme::GOOD))
        } else {
            Span::styled("○ off", theme::muted())
        }
    };
    let rows: Vec<(String, Span)> = vec![
        (
            s.setting_trash.into(),
            Span::styled(
                cfg.clean.mode.clone(),
                if cfg.clean.mode == "permanent" {
                    Style::default().fg(theme::BAD)
                } else {
                    Style::default().fg(theme::GOOD)
                },
            ),
        ),
        (s.setting_git.into(), onoff(cfg.git.enabled)),
        (s.setting_branches.into(), onoff(cfg.git.merged_branches)),
        (s.setting_gc.into(), onoff(cfg.git.gc)),
        (
            s.setting_lang.into(),
            Span::styled(cfg.ui.language.as_str().to_uppercase(), theme::bold()),
        ),
        (
            s.setting_stale.into(),
            Span::styled(cfg.scan.stale_days.to_string(), theme::bold()),
        ),
        (
            s.setting_min_size.into(),
            Span::styled(cfg.scan.min_size_mb.to_string(), theme::bold()),
        ),
        (
            s.setting_depth.into(),
            Span::styled(cfg.scan.max_depth.to_string(), theme::bold()),
        ),
        ("Default roots".into(), onoff(cfg.scan.default_roots)),
        (s.setting_volumes.into(), onoff(cfg.scan.volumes)),
        (s.setting_block.into(), onoff(cfg.clean.block_running_apps)),
        (
            s.setting_recovery_days.into(),
            Span::styled(cfg.clean.recovery_days.to_string(), theme::bold()),
        ),
    ];
    let items: Vec<ListItem> = rows
        .into_iter()
        .enumerate()
        .map(|(i, (label, val))| {
            let marker = if i == app.settings_cursor {
                Span::styled("▸ ", theme::accent_bold())
            } else {
                Span::raw("  ")
            };
            ListItem::new(Line::from(vec![
                marker,
                val,
                Span::raw("  "),
                Span::raw(label),
            ]))
        })
        .collect();
    let [list_area, info_area] = Layout::vertical([
        Constraint::Length(SETTINGS_ROWS as u16 + 2),
        Constraint::Min(3),
    ])
    .areas(area);
    f.render_widget(List::new(items).block(block(s.tab_settings)), list_area);
    let ctx = app.ctx();
    let mut lines = vec![
        Line::from(Span::styled(s.setting_hint, theme::muted())),
        Line::from(vec![
            Span::styled("config ", theme::muted()),
            Span::raw(app.env.paths.config_file.display().to_string()),
        ]),
        Line::from(""),
    ];
    lines.push(Line::from(Span::styled(s.setting_roots, theme::bold())));
    for r in &ctx.roots {
        lines.push(Line::from(format!(
            "  {}",
            compact_path(r, &app.env.paths.home)
        )));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), info_area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let keys = match app.tab {
        Tab::Candidates | Tab::Git => s.keys_table,
        Tab::Settings => s.setting_hint,
        Tab::Explore => s.explore_keys,
        Tab::History => s.undo,
        _ => s.keys_global,
    };
    let line1 = if let Some((msg, _)) = &app.toast {
        Line::from(Span::styled(format!(" {msg}"), theme::accent_bold()))
    } else {
        Line::from(Span::styled(format!(" {keys}"), theme::muted()))
    };
    let line2 = Line::from(Span::styled(
        format!(
            " Tab / 1-6 view   r rescan   ? help   E export   L language   {}",
            s.quit_hint
        ),
        theme::muted(),
    ));
    f.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw_confirm(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let (n, total) = app.selected_total();
    let high = app
        .candidates
        .iter()
        .filter(|c| app.selected.contains(&c.id) && c.risk == Risk::High)
        .count();
    let rect = centered(area, 70, 11);
    f.render_widget(Clear, rect);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{n} {}  ·  ", s.items), theme::bold()),
            Span::styled(human_size(total), theme::accent_bold()),
        ]),
        Line::from(""),
        Line::from(Span::styled(s.confirm_body, theme::muted())),
    ];
    if high > 0 {
        lines.push(Line::from(Span::styled(
            format!("⚠ {}", s.confirm_high),
            Style::default().fg(theme::BAD).add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(""));
    let mode = if app.dry_run {
        Span::styled(
            format!("[d] {}", s.dry_run),
            Style::default().fg(theme::WARN),
        )
    } else {
        Span::styled(
            mode_label(app.mode, s),
            match app.mode {
                CleanMode::Permanent => Style::default().fg(theme::BAD),
                _ => Style::default().fg(theme::GOOD),
            },
        )
    };
    lines.push(Line::from(mode));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{}    d dry-run", s.confirm_keys),
        theme::muted(),
    )));
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(block(s.confirm_title).border_style(theme::accent())),
        rect,
    );
}

fn draw_cleaning(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let rect = centered(area, 90, 20);
    f.render_widget(Clear, rect);
    let (idx, total, label) = &app.clean_progress;
    let ratio = if *total == 0 {
        0.0
    } else {
        (*idx as f64 + 1.0) / *total as f64
    };
    let [g, log] = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(rect.inner(
        ratatui::layout::Margin {
            horizontal: 1,
            vertical: 1,
        },
    ));
    f.render_widget(block(s.cleaning).border_style(theme::accent()), rect);
    f.render_widget(
        Gauge::default()
            .gauge_style(theme::accent())
            .ratio(ratio.clamp(0.0, 1.0))
            .label(format!(
                "{}/{} {}",
                idx + 1,
                total,
                ellipsize_end(label, 40)
            )),
        g,
    );
    let visible = log.height as usize;
    let start = app.clean_log.len().saturating_sub(visible);
    let lines: Vec<Line> = app.clean_log[start..]
        .iter()
        .map(|l| {
            let style = if l.contains('✓') {
                Style::default().fg(theme::GOOD)
            } else if l.contains('✗') {
                Style::default().fg(theme::BAD)
            } else if l.starts_with('[') {
                theme::bold()
            } else {
                theme::muted()
            };
            Line::from(Span::styled(ellipsize_middle(l, log.width as usize), style))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), log);
}

fn draw_done(f: &mut Frame, app: &App, area: Rect, entry: &cleanprojects_core::HistoryEntry) {
    let s = app.s();
    let rect = centered(area, 70, 14);
    f.render_widget(Clear, rect);
    let ok = entry.items.iter().filter(|i| i.ok).count();
    let failed = entry.items.len() - ok;
    let mut lines = vec![
        Line::from(vec![
            Span::styled(human_size(entry.freed), theme::accent_bold()),
            Span::styled(format!(" {}", s.freed), theme::muted()),
            Span::raw(if entry.dry_run {
                format!("  ({})", s.dry_run)
            } else {
                String::new()
            }),
        ]),
        Line::from(vec![
            Span::styled(format!("{ok} ✓  "), Style::default().fg(theme::GOOD)),
            Span::styled(
                format!("{failed} ✗"),
                Style::default().fg(if failed > 0 { theme::BAD } else { theme::MUTED }),
            ),
        ]),
        Line::from(""),
    ];
    for i in entry.items.iter().filter(|i| !i.ok).take(6) {
        lines.push(Line::from(Span::styled(
            format!("✗ {}: {}", i.label, i.error.clone().unwrap_or_default()),
            Style::default().fg(theme::BAD),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(s.press_any_key, theme::muted())));
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(block(s.clean_done).border_style(Style::default().fg(theme::GOOD))),
        rect,
    );
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let rect = centered(area, 72, 22);
    f.render_widget(Clear, rect);
    let rows = [
        ("Tab / Shift-Tab / 1-5", "switch view"),
        ("j k ↑ ↓ PgUp PgDn g G", "move"),
        ("space", "select / unselect"),
        ("a / A", "select all low-risk / clear selection"),
        ("x", "exclude / restore from future scans"),
        ("/", "search   (Esc clears filters)"),
        ("f  c  v  s", "cycle risk · category · view · sort"),
        ("i", "toggle detail panel"),
        ("t", "toggle trash / permanent delete"),
        ("d / Enter", "clean selected (confirmation follows)"),
        ("E", "export markdown report to home"),
        ("r", "rescan"),
        ("L", "toggle language"),
        ("q / Ctrl-C", "quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k:<24}"), theme::accent_bold()),
                Span::raw(*v),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(block(s.help_title)), rect);
}

fn draw_explore(f: &mut Frame, app: &App, area: Rect) {
    let s = app.s();
    let total: u64 = app.explore_nodes.iter().map(|n| n.size).sum();
    let title = format!(
        "{} · {} · {}{}",
        s.tab_explore,
        compact_path(&app.explore_dir, &app.env.paths.home),
        human_size(total),
        if app.explore_loading { "  …" } else { "" }
    );
    let max = app
        .explore_nodes
        .first()
        .map(|n| n.size)
        .unwrap_or(1)
        .max(1);
    let bar_w = area.width.saturating_sub(4 + 10 + 2 + 40).clamp(8, 30) as usize;
    let name_w = area.width.saturating_sub(4 + 10 + 2 + bar_w as u16 + 2) as usize;
    let rows: Vec<Row> = app
        .explore_nodes
        .iter()
        .map(|n| {
            let filled = ((n.size as f64 / max as f64) * bar_w as f64).round() as usize;
            let name = if n.is_dir {
                format!("{}/", n.name)
            } else {
                n.name.clone()
            };
            Row::new(vec![
                Line::from(Span::styled(
                    format!("{:>9}", human_size(n.size)),
                    theme::bold(),
                ))
                .alignment(Alignment::Right),
                Line::from(vec![
                    Span::styled("█".repeat(filled), theme::accent()),
                    Span::styled("░".repeat(bar_w.saturating_sub(filled)), theme::muted()),
                ]),
                Line::from(Span::styled(
                    ellipsize_end(&name, name_w),
                    if n.is_dir {
                        theme::bold()
                    } else {
                        Style::default()
                    },
                )),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(bar_w as u16),
            Constraint::Min(10),
        ],
    )
    .block(block(title))
    .row_highlight_style(theme::cursor())
    .column_spacing(1);
    let mut state = TableState::default();
    if !app.explore_nodes.is_empty() {
        state.select(Some(app.explore_cursor.min(app.explore_nodes.len() - 1)));
    }
    f.render_stateful_widget(table, area, &mut state);
    if app.explore_nodes.is_empty() {
        let p = Paragraph::new(Span::styled(
            if app.explore_loading { "…" } else { "empty" },
            theme::muted(),
        ))
        .alignment(Alignment::Center);
        let inner = Rect {
            x: area.x + 2,
            y: area.y + area.height / 2,
            width: area.width.saturating_sub(4),
            height: 1,
        };
        f.render_widget(p, inner);
    }
}
