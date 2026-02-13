//! Live TUI demo of the Sovereign Engine scheduler.
//!
//! Spins up an in-memory database, registers models with slot limits,
//! spawns simulated users, and renders a ratatui dashboard showing
//! gate status, queue depths, per-user priority scores, and an event log.
//!
//! Run with:
//!   cargo run --example queue_demo --features demo

use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table, Wrap};

use sovereign_engine::db::Database;
use sovereign_engine::scheduler::fairness;
use sovereign_engine::scheduler::gate::GateSnapshot;
use sovereign_engine::scheduler::queue::QueueStats;
use sovereign_engine::scheduler::Scheduler;

use rand::rngs::StdRng;
use rand::RngExt;
use tokio::sync::mpsc;

// ─── Demo events ──────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum DemoEvent {
    Acquired {
        user: String,
        model: String,
        waited_ms: u64,
    },
    Released {
        user: String,
        model: String,
        tokens: i64,
    },
    Timeout {
        user: String,
        model: String,
    },
}

impl DemoEvent {
    fn to_line(&self) -> Line<'_> {
        match self {
            DemoEvent::Acquired {
                user,
                model,
                waited_ms,
            } => {
                let wait = if *waited_ms == 0 {
                    "(immediate)".to_string()
                } else {
                    format!("(waited {}ms)", waited_ms)
                };
                Line::from(vec![
                    Span::styled(
                        format!("{} ", user),
                        Style::default().fg(Color::White).bold(),
                    ),
                    Span::raw("acquired "),
                    Span::styled(model.as_str(), Style::default().fg(Color::Cyan)),
                    Span::raw(format!(" {}", wait)),
                ])
                .style(Style::default().fg(Color::Green))
            }
            DemoEvent::Released {
                user,
                model,
                tokens,
            } => Line::from(vec![
                Span::styled(
                    format!("{} ", user),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::raw("released "),
                Span::styled(model.as_str(), Style::default().fg(Color::Cyan)),
                Span::raw(format!(" (+{} tokens)", format_tokens(*tokens))),
            ])
            .style(Style::default().fg(Color::Blue)),
            DemoEvent::Timeout { user, model } => Line::from(vec![
                Span::styled(
                    format!("{} ", user),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::raw("timed out on "),
                Span::styled(model.as_str(), Style::default().fg(Color::Cyan)),
            ])
            .style(Style::default().fg(Color::Red)),
        }
    }
}

fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

// ─── Per-user state tracked by the TUI ────────────────────────────

#[derive(Clone)]
struct UserProfile {
    name: String,
    label: &'static str,
    interval_ms: (u64, u64),
    inference_ms: (u64, u64),
    tokens_per_req: (i64, i64),
}

#[derive(Clone, Default)]
struct UserStats {
    completed: u32,
    timeouts: u32,
    total_tokens: i64,
    priority: f64,
}

// ─── Sparkline history per model ──────────────────────────────────

const SPARK_LEN: usize = 40;

struct SparkHistory {
    data: Vec<u64>,
}

impl SparkHistory {
    fn new() -> Self {
        Self {
            data: vec![0; SPARK_LEN],
        }
    }

    fn push(&mut self, val: u64) {
        if self.data.len() >= SPARK_LEN {
            self.data.remove(0);
        }
        self.data.push(val);
    }
}

// ─── Main ─────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Database setup ──
    let db = Database::test_db().await;

    // Seed IdP config + users (needed for FK constraints on usage_log)
    sqlx::query(
        "INSERT INTO idp_configs (id, name, issuer, client_id, client_secret_enc)
         VALUES ('demo-idp', 'Demo', 'https://demo', 'client', 'secret')",
    )
    .execute(&db.pool)
    .await?;

    let users = vec![
        UserProfile {
            name: "Alice".into(),
            label: "heavy",
            interval_ms: (500, 1500),
            inference_ms: (200, 800),
            tokens_per_req: (2000, 5000),
        },
        UserProfile {
            name: "Bob".into(),
            label: "heavy",
            interval_ms: (800, 2000),
            inference_ms: (300, 1000),
            tokens_per_req: (1500, 4000),
        },
        UserProfile {
            name: "Charlie".into(),
            label: "medium",
            interval_ms: (2000, 4000),
            inference_ms: (200, 600),
            tokens_per_req: (500, 1500),
        },
        UserProfile {
            name: "Diana".into(),
            label: "light",
            interval_ms: (4000, 8000),
            inference_ms: (100, 400),
            tokens_per_req: (100, 500),
        },
        UserProfile {
            name: "Eve".into(),
            label: "light",
            interval_ms: (5000, 10000),
            inference_ms: (100, 300),
            tokens_per_req: (50, 200),
        },
    ];

    for u in &users {
        let uid = u.name.to_lowercase();
        sqlx::query("INSERT INTO users (id, idp_id, subject, email) VALUES (?, 'demo-idp', ?, ?)")
            .bind(&uid)
            .bind(&uid)
            .bind(format!("{}@demo.local", uid))
            .execute(&db.pool)
            .await?;
    }

    // ── Scheduler setup ──
    let scheduler = Scheduler::new();

    // Write demo settings to DB: faster timeouts, more aggressive usage penalty
    sovereign_engine::scheduler::settings::save_setting(&db, "queue_timeout_secs", "5").await?;
    sovereign_engine::scheduler::settings::save_setting(&db, "fairness_usage_weight", "15.0")
        .await?;
    scheduler.reload_settings(&db).await?;

    // Models with tight slot limits to create contention
    let models = vec![
        ("llama-70b", 2u32),
        ("mistral-7b", 4u32),
        ("phi-3-mini", 3u32),
    ];
    for (model_id, slots) in &models {
        scheduler.gate().register(model_id, *slots).await;
    }

    // Model names + traffic weights for random selection
    let model_weights: Vec<(&str, f64)> = vec![
        ("llama-70b", 0.50),
        ("mistral-7b", 0.30),
        ("phi-3-mini", 0.20),
    ];

    // ── Event channel ──
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<DemoEvent>();

    // ── Spawn user simulation tasks ──
    for profile in &users {
        let db = db.clone();
        let scheduler = scheduler.clone();
        let tx = event_tx.clone();
        let profile = profile.clone();
        let model_weights = model_weights.clone();

        tokio::spawn(async move {
            let mut rng: StdRng = rand::make_rng();
            loop {
                // Wait between requests
                let interval = rng.random_range(profile.interval_ms.0..=profile.interval_ms.1);
                tokio::time::sleep(Duration::from_millis(interval)).await;

                // Pick a model based on traffic weights
                let model_id = pick_model(&model_weights, &mut rng);
                let user_id = profile.name.to_lowercase();

                let settings = scheduler.settings().await;
                let timeout = Duration::from_secs(settings.queue_timeout_secs);

                let start = Instant::now();
                let result = scheduler
                    .gate()
                    .acquire_with_timeout(
                        model_id,
                        &user_id,
                        &db,
                        &settings,
                        scheduler.queue(),
                        timeout,
                    )
                    .await;

                match result {
                    Ok(_slot) => {
                        let waited_ms = start.elapsed().as_millis() as u64;
                        let _ = tx.send(DemoEvent::Acquired {
                            user: profile.name.clone(),
                            model: model_id.to_string(),
                            waited_ms,
                        });

                        // Simulate inference
                        let inference_time =
                            rng.random_range(profile.inference_ms.0..=profile.inference_ms.1);
                        tokio::time::sleep(Duration::from_millis(inference_time)).await;

                        // Generate token count and log usage
                        let tokens =
                            rng.random_range(profile.tokens_per_req.0..=profile.tokens_per_req.1);
                        let input_tokens = tokens / 3;
                        let output_tokens = tokens - input_tokens;

                        let _ = sqlx::query(
                            "INSERT INTO usage_log (id, user_id, model_id, input_tokens, output_tokens, latency_ms, queued_ms)
                             VALUES (?, ?, ?, ?, ?, ?, ?)",
                        )
                        .bind(uuid::Uuid::new_v4().to_string())
                        .bind(&user_id)
                        .bind(model_id)
                        .bind(input_tokens)
                        .bind(output_tokens)
                        .bind(inference_time as i64)
                        .bind(waited_ms as i64)
                        .execute(&db.pool)
                        .await;

                        let _ = tx.send(DemoEvent::Released {
                            user: profile.name.clone(),
                            model: model_id.to_string(),
                            tokens,
                        });

                        // _slot drops here, releasing the gate
                    }
                    Err(_) => {
                        let _ = tx.send(DemoEvent::Timeout {
                            user: profile.name.clone(),
                            model: model_id.to_string(),
                        });
                    }
                }
            }
        });
    }

    // ── Terminal setup ──
    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // ── TUI state ──
    let mut user_stats: HashMap<String, UserStats> = users
        .iter()
        .map(|u| (u.name.clone(), UserStats::default()))
        .collect();
    let mut event_log: Vec<DemoEvent> = Vec::new();
    let mut spark_history: HashMap<String, SparkHistory> = models
        .iter()
        .map(|(m, _)| (m.to_string(), SparkHistory::new()))
        .collect();
    let mut paused = false;
    let start_time = Instant::now();
    let mut last_priority_update = Instant::now();

    // ── Render loop ──
    loop {
        // Handle keyboard input
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char(' ') => paused = !paused,
                        _ => {}
                    }
                }
            }
        }

        // Drain events from simulation tasks
        while let Ok(evt) = event_rx.try_recv() {
            match &evt {
                DemoEvent::Acquired { user, .. } => {
                    if let Some(s) = user_stats.get_mut(user) {
                        s.completed += 1;
                    }
                }
                DemoEvent::Released { user, tokens, .. } => {
                    if let Some(s) = user_stats.get_mut(user) {
                        s.total_tokens += tokens;
                    }
                }
                DemoEvent::Timeout { user, .. } => {
                    if let Some(s) = user_stats.get_mut(user) {
                        s.timeouts += 1;
                    }
                }
            }
            event_log.push(evt);
            // Keep last 200 events
            if event_log.len() > 200 {
                event_log.remove(0);
            }
        }

        // Update sparkline data from queue stats
        let queue_stats = scheduler.get_queue_stats().await;
        for (model_id, _) in &models {
            let depth = queue_stats.get(*model_id).map_or(0, |s| s.depth as u64);
            if let Some(spark) = spark_history.get_mut(*model_id) {
                spark.push(depth);
            }
        }

        // Recalculate priorities periodically (not every frame — DB queries)
        if last_priority_update.elapsed() > Duration::from_secs(1) {
            let settings = scheduler.settings().await;
            for profile in &users {
                let uid = profile.name.to_lowercase();
                if let Ok(p) = fairness::calculate_user_priority(&db, &settings, &uid, 0.0).await {
                    if let Some(s) = user_stats.get_mut(&profile.name) {
                        s.priority = p;
                    }
                }
            }
            last_priority_update = Instant::now();
        }

        // Get gate snapshots
        let gate_status = scheduler.gate().status().await;

        // ── Draw ──
        let elapsed = start_time.elapsed();
        terminal.draw(|f| {
            draw_ui(
                f,
                &models,
                &gate_status,
                &queue_stats,
                &spark_history,
                &users,
                &user_stats,
                &event_log,
                paused,
                elapsed,
            );
        })?;
    }

    // ── Cleanup ──
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}

// ─── Weighted random model selection ──────────────────────────────

fn pick_model<'a>(weights: &[(&'a str, f64)], rng: &mut impl RngExt) -> &'a str {
    let total: f64 = weights.iter().map(|(_, w)| w).sum();
    let mut roll: f64 = rng.random_range(0.0..total);
    for (model, weight) in weights {
        roll -= weight;
        if roll <= 0.0 {
            return model;
        }
    }
    weights.last().unwrap().0
}

// ─── UI drawing ───────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn draw_ui(
    f: &mut Frame,
    models: &[(&str, u32)],
    gate_status: &HashMap<String, GateSnapshot>,
    queue_stats: &HashMap<String, QueueStats>,
    spark_history: &HashMap<String, SparkHistory>,
    users: &[UserProfile],
    user_stats: &HashMap<String, UserStats>,
    event_log: &[DemoEvent],
    paused: bool,
    elapsed: Duration,
) {
    let area = f.area();

    // Main layout: title bar, body, status bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Min(10),   // body
            Constraint::Length(1), // status bar
        ])
        .split(area);

    // Title
    let title = Line::from(vec![
        Span::styled(
            " Sovereign Engine ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "— Queue & Fairness Demo",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(title), outer[0]);

    // Body: split into top (gates + sparklines) and bottom (users + events)
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(models.len() as u16 + 2), // gates table
            Constraint::Length(users.len() as u16 + 2),  // user stats table
            Constraint::Min(4),                          // event log
        ])
        .split(outer[1]);

    // ── Model Gates + Sparklines ──
    let gates_and_sparks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(body[0]);

    draw_gates_table(f, gates_and_sparks[0], models, gate_status, queue_stats);
    draw_sparklines(f, gates_and_sparks[1], models, spark_history);

    // ── User Stats ──
    draw_user_table(f, body[1], users, user_stats);

    // ── Event Log ──
    draw_event_log(f, body[2], event_log);

    // ── Status bar ──
    let elapsed_secs = elapsed.as_secs();
    let mins = elapsed_secs / 60;
    let secs = elapsed_secs % 60;
    let pause_indicator = if paused { " PAUSED " } else { "" };
    let status = Line::from(vec![
        Span::styled(" [q]", Style::default().fg(Color::Yellow)),
        Span::raw(" Quit  "),
        Span::styled("[space]", Style::default().fg(Color::Yellow)),
        Span::raw(" Pause/Resume  "),
        Span::styled(pause_indicator, Style::default().fg(Color::Red).bold()),
        Span::raw(format!(
            "{}Elapsed: {:02}:{:02}",
            " ".repeat(area.width.saturating_sub(50) as usize),
            mins,
            secs
        )),
    ]);
    f.render_widget(Paragraph::new(status), outer[2]);
}

fn draw_gates_table(
    f: &mut Frame,
    area: Rect,
    models: &[(&str, u32)],
    gate_status: &HashMap<String, GateSnapshot>,
    queue_stats: &HashMap<String, QueueStats>,
) {
    let header = Row::new(vec!["Model", "Slots", "Queue", "Avg Wait"])
        .style(Style::default().fg(Color::Cyan).bold());

    let rows: Vec<Row> = models
        .iter()
        .map(|(model_id, _max)| {
            let snap = gate_status.get(*model_id);
            let stats = queue_stats.get(*model_id);

            let in_flight = snap.map_or(0, |s| s.in_flight);
            let max_slots = snap.map_or(0, |s| s.max_slots);
            let depth = stats.map_or(0, |s| s.depth);
            let avg_wait = stats.map_or(0, |s| s.avg_wait_ms);

            // Slot bar: filled = in_flight, empty = remaining
            let bar = slot_bar(in_flight, max_slots);
            let utilization = if max_slots > 0 {
                in_flight as f64 / max_slots as f64
            } else {
                0.0
            };
            let bar_color = if utilization >= 1.0 {
                Color::Red
            } else if utilization >= 0.5 {
                Color::Yellow
            } else {
                Color::Green
            };

            let wait_str = if avg_wait > 0 {
                format!("{}ms", avg_wait)
            } else {
                "-".to_string()
            };

            Row::new(vec![
                Cell::from(*model_id),
                Cell::from(bar).style(Style::default().fg(bar_color)),
                Cell::from(format!("{}", depth)),
                Cell::from(wait_str),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Length(14),
        Constraint::Length(6),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Model Gates "),
    );

    f.render_widget(table, area);
}

fn slot_bar(in_flight: u32, max_slots: u32) -> String {
    let filled = in_flight.min(max_slots) as usize;
    let empty = (max_slots.saturating_sub(in_flight)) as usize;
    format!(
        "[{}{}]{}/{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty),
        in_flight,
        max_slots
    )
}

fn draw_sparklines(
    f: &mut Frame,
    area: Rect,
    models: &[(&str, u32)],
    spark_history: &HashMap<String, SparkHistory>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Queue History ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    if models.is_empty() || inner.height == 0 {
        return;
    }

    let row_height = (inner.height / models.len() as u16).max(1);

    for (i, (model_id, _)) in models.iter().enumerate() {
        let y = inner.y + (i as u16) * row_height;
        if y >= inner.y + inner.height {
            break;
        }

        let spark_area = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: row_height.min(inner.y + inner.height - y),
        };

        if let Some(history) = spark_history.get(*model_id) {
            let label = format!("{:<12}", model_id);
            let sparkline = Sparkline::default()
                .data(&history.data)
                .max(8) // reasonable max queue depth for display
                .style(Style::default().fg(Color::Cyan))
                .bar_set(ratatui::symbols::bar::NINE_LEVELS);

            // Render label on the first line of the row, sparkline below if space
            if spark_area.height >= 2 {
                let label_area = Rect {
                    height: 1,
                    ..spark_area
                };
                let spark_sub = Rect {
                    y: spark_area.y + 1,
                    height: spark_area.height - 1,
                    ..spark_area
                };
                f.render_widget(
                    Paragraph::new(label).style(Style::default().fg(Color::DarkGray)),
                    label_area,
                );
                f.render_widget(sparkline, spark_sub);
            } else {
                // Not enough space for both — just show sparkline
                f.render_widget(sparkline, spark_area);
            }
        }
    }
}

fn draw_user_table(
    f: &mut Frame,
    area: Rect,
    users: &[UserProfile],
    user_stats: &HashMap<String, UserStats>,
) {
    let header = Row::new(vec!["User", "Profile", "Done", "T/O", "Tokens", "Priority"])
        .style(Style::default().fg(Color::Cyan).bold());

    let rows: Vec<Row> = users
        .iter()
        .map(|u| {
            let stats = user_stats.get(&u.name).cloned().unwrap_or_default();

            let priority_color = if stats.priority > 90.0 {
                Color::Green
            } else if stats.priority > 70.0 {
                Color::Yellow
            } else {
                Color::Red
            };

            Row::new(vec![
                Cell::from(u.name.as_str()),
                Cell::from(u.label),
                Cell::from(format!("{}", stats.completed)),
                Cell::from(format!("{}", stats.timeouts)),
                Cell::from(format_tokens(stats.total_tokens)),
                Cell::from(format!("{:.1}", stats.priority))
                    .style(Style::default().fg(priority_color)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" User Stats "));

    f.render_widget(table, area);
}

fn draw_event_log(f: &mut Frame, area: Rect, event_log: &[DemoEvent]) {
    let block = Block::default().borders(Borders::ALL).title(" Event Log ");

    let inner = block.inner(area);
    let visible_count = inner.height as usize;
    let start = event_log.len().saturating_sub(visible_count);
    let visible_events = &event_log[start..];

    let lines: Vec<Line> = visible_events.iter().map(|e| e.to_line()).collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });

    f.render_widget(block, area);
    f.render_widget(paragraph, inner);
}
