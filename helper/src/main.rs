mod browser;
mod desktop;
mod pulse;
mod server;
mod tray;

use anyhow::Result;
use clap::Parser;
use pulse::Pulse;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "meet123",
    about = "Google Meet Linux tab relay with system audio"
)]
struct Args {
    /// HTTP / WebSocket listen address
    #[arg(long, default_value = "127.0.0.1:17373")]
    listen: String,
    /// Chromium-based browser to open (name or path). Also: MEET123_BROWSER
    #[arg(long, env = "MEET123_BROWSER")]
    browser: Option<String>,
    /// Do not open a Chromium-based browser
    #[arg(long)]
    no_open: bool,
    /// Do not show a status-notifier tray icon
    #[arg(long)]
    no_tray: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "meet123=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();
    browser::set_preferred(args.browser.clone());
    let web_root = server::find_web_root()?;
    info!("serving UI from {}", web_root.display());

    let mut pulse = Pulse::new();
    if let Err(err) = pulse.ensure_silent_sink().await {
        warn!("could not create silent sink yet: {err}");
    }

    let listen_url = format!("http://{}", args.listen);
    let state = server::AppState {
        pulse: Arc::new(Mutex::new(pulse)),
        listen_url: listen_url.clone(),
        web_root,
        follow: Arc::new(Mutex::new(None)),
    };

    let shutdown = Arc::new(Notify::new());
    let _tray_handle = if !args.no_tray {
        tray::spawn(listen_url.clone(), shutdown.clone()).await
    } else {
        None
    };

    let app = server::router(state.clone());
    let addr: SocketAddr = args.listen.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on {listen_url}");
    if let Some(bin) = browser::chosen_browser() {
        info!("browser {bin}");
    }

    if !args.no_open {
        match browser::open_in_chromium(&listen_url) {
            Ok(bin) => info!("opened {bin}"),
            Err(err) => warn!("could not open browser: {err}"),
        }
    }

    let server = axum::serve(listener, app).with_graceful_shutdown({
        let shutdown = shutdown.clone();
        async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = shutdown.notified() => {}
            }
        }
    });

    server.await?;

    {
        let mut follow = state.follow.lock().await;
        if let Some(handle) = follow.take() {
            handle.abort();
        }
    }
    let mut pulse = state.pulse.lock().await;
    pulse.shutdown().await?;
    info!("cleaned up PipeWire modules");
    Ok(())
}
