#![forbid(unsafe_code)]

mod app;
mod config;
mod model;
mod nostr;
mod secrets;
mod theme;
mod ui;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use crossterm::event::EventStream;
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::app::App;
use crate::config::{Config, Paths};
use crate::nostr::commands::Command;
use crate::nostr::updates::Update;

#[derive(Debug, Parser)]
#[command(name = "rastrich", version, about = "A terminal Nostr client")]
struct Args {
    /// Path to config.toml
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Directory for the local event cache
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let paths = Paths::discover(args.config, args.data_dir)?;
    let config = Config::load(&paths.config)?;
    let _log_guard = init_logging(&paths)?;

    tracing::info!("starting rastrich");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start async runtime")?;

    let mut terminal = ratatui::try_init().context("failed to initialize terminal")?;
    let result = runtime.block_on(run(&mut terminal, paths, config));
    ratatui::restore();
    result
}

fn init_logging(paths: &Paths) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let appender = tracing_appender::rolling::daily(&paths.log_dir, "rastrich.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .map_err(|err| anyhow!("failed to initialize logging: {err}"))?;

    Ok(guard)
}

async fn run(terminal: &mut DefaultTerminal, paths: Paths, config: Config) -> Result<()> {
    let (commands_tx, commands_rx) = mpsc::channel::<Command>(64);
    let (updates_tx, mut updates_rx) = mpsc::channel::<Update>(256);

    let mut app = App::new(commands_tx.clone());
    let account = config.account.clone();
    if let Some(account) = &account {
        app.set_restoring(account.npub.clone(), account.mode);
    }

    let worker_updates = updates_tx.clone();
    let worker = tokio::spawn(async move {
        if let Err(err) =
            nostr::worker::run(paths, config, commands_rx, worker_updates.clone()).await
        {
            tracing::error!("nostr worker stopped: {err:#}");
            let _ = worker_updates
                .send(Update::Error(format!("worker stopped: {err:#}")))
                .await;
            let _ = worker_updates.send(Update::Shutdown).await;
        }
    });

    commands_tx
        .send(Command::AutoLogin(account))
        .await
        .context("failed to start the nostr worker")?;

    let mut terminal_events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    terminal.draw(|frame| ui::render(frame, &mut app))?;

    while !app.should_quit {
        tokio::select! {
            Some(event) = terminal_events.next() => match event {
                Ok(event) => app.handle_terminal_event(event),
                Err(err) => tracing::warn!("terminal event error: {err}"),
            },
            Some(update) = updates_rx.recv() => app.apply_update(update),
            _ = tick.tick() => app.tick(),
        }

        terminal.draw(|frame| ui::render(frame, &mut app))?;
    }

    let _ = commands_tx.send(Command::Shutdown).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), worker).await;
    Ok(())
}
