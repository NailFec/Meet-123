use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::broadcast;
use tracing::{info, warn};

pub const SILENT_SINK: &str = "meet123_silent";
pub const SILENT_LABEL: &str = "Meet123Silent";
pub const CAPTURE_SINK: &str = "meet123_capture";
pub const CAPTURE_LABEL: &str = "Meet123Capture";

const BROWSER_MARKERS: &[&str] = &[
    "chrome",
    "chromium",
    "google-chrome",
    "msedge",
    "microsoft-edge",
    "vivaldi",
    "brave",
    "meet123",
];

#[derive(Debug, Clone, Serialize)]
pub struct AudioSource {
    pub name: String,
    pub description: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioApp {
    pub index: u32,
    pub name: String,
    pub media: String,
    pub binary: String,
    pub sink: u32,
    pub browser: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngageResult {
    pub ok: bool,
    pub isolated: bool,
    pub routing_mode: String,
}

pub struct Pulse {
    silent_module: Option<String>,
    capture_module: Option<String>,
    loopback_module: Option<String>,
    moved: Vec<(u32, u32)>,
    snapshot: HashSet<u32>,
    follow_keys: Vec<(String, String)>,
    follow_exclude_browser: bool,
    isolated_key: Option<(String, String, String)>,
    pending_capture: Option<String>,
    child: Option<Child>,
    reader: Option<tokio::task::JoinHandle<()>>,
    pub capture_source: Option<String>,
    pub routing_mode: String,
    pub playback_isolated: bool,
    pub tx: broadcast::Sender<bytes::Bytes>,
}

impl Pulse {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            silent_module: None,
            capture_module: None,
            loopback_module: None,
            moved: Vec::new(),
            snapshot: HashSet::new(),
            follow_keys: Vec::new(),
            follow_exclude_browser: false,
            isolated_key: None,
            pending_capture: None,
            child: None,
            reader: None,
            capture_source: None,
            routing_mode: "idle".into(),
            playback_isolated: false,
            tx,
        }
    }

    pub fn silent_ready(&self) -> bool {
        sink_exists(SILENT_SINK).unwrap_or(false)
    }

    pub fn needs_follow(&self) -> bool {
        self.capture_source.is_some()
            && (self.follow_exclude_browser
                || !self.follow_keys.is_empty()
                || self.playback_isolated)
    }

    pub async fn ensure_silent_sink(&mut self) -> Result<()> {
        if sink_exists(SILENT_SINK)? {
            return Ok(());
        }
        let id = load_null_sink(SILENT_SINK, SILENT_LABEL)?;
        self.silent_module = Some(id);
        info!("created silent sink {SILENT_SINK}");
        Ok(())
    }

    pub fn default_sink() -> Result<String> {
        let info = pactl_json(&["info"])?;
        info.get("default_sink_name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .context("missing default_sink_name")
    }

    pub fn default_monitor() -> Result<String> {
        Ok(format!("{}.monitor", Self::default_sink()?))
    }

    pub fn list_sources() -> Result<Vec<AudioSource>> {
        let default_mon = Self::default_monitor().unwrap_or_default();
        let list = pactl_json(&["list", "sources"])?;
        let mut out = Vec::new();
        if let Some(arr) = list.as_array() {
            for src in arr {
                let name = src.get("name").and_then(Value::as_str).unwrap_or_default();
                if !name.ends_with(".monitor") || name.starts_with("meet123_") {
                    continue;
                }
                out.push(AudioSource {
                    is_default: name == default_mon,
                    description: src
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(name)
                        .to_string(),
                    name: name.to_string(),
                });
            }
        }
        out.sort_by(|a, b| {
            b.is_default
                .cmp(&a.is_default)
                .then(a.description.cmp(&b.description))
        });
        Ok(out)
    }

    pub fn list_apps() -> Result<Vec<AudioApp>> {
        let list = pactl_json(&["list", "sink-inputs"])?;
        let mut out = Vec::new();
        if let Some(arr) = list.as_array() {
            for input in arr {
                let Some(index) = json_u32(input.get("index")) else {
                    continue;
                };
                let sink = json_u32(input.get("sink")).unwrap_or(0);
                let props = input.get("properties").cloned().unwrap_or(Value::Null);
                let name = prop_str(&props, "application.name");
                let media = prop_str(&props, "media.name");
                let binary = prop_str(&props, "application.process.binary");
                let browser = is_browser(&name, &binary);
                out.push(AudioApp {
                    index,
                    name,
                    media,
                    binary,
                    sink,
                    browser,
                });
            }
        }
        Ok(out)
    }

    pub async fn prepare(
        &mut self,
        source: Option<String>,
        app_indices: Vec<u32>,
        exclude_browser: bool,
        loopback: bool,
    ) -> Result<()> {
        self.stop_capture().await?;
        self.restore_routing()?;
        self.ensure_silent_sink().await?;

        let apps = Self::list_apps()?;
        self.snapshot = apps.iter().map(|app| app.index).collect();

        let default_sink = Self::default_sink()?;
        let default_monitor = format!("{default_sink}.monitor");

        if !app_indices.is_empty() || exclude_browser {
            self.ensure_capture_sink()?;
            let selected: Vec<AudioApp> = if !app_indices.is_empty() {
                apps.iter()
                    .filter(|app| app_indices.contains(&app.index))
                    .cloned()
                    .collect()
            } else {
                apps.iter()
                    .filter(|app| !app.browser && !skip_system_stream(app))
                    .cloned()
                    .collect()
            };
            self.follow_keys = selected
                .iter()
                .map(|app| (app.name.clone(), app.binary.clone()))
                .collect();
            self.follow_exclude_browser = exclude_browser && app_indices.is_empty();
            for app in &selected {
                self.move_app(app, CAPTURE_SINK)?;
            }
            if loopback {
                let id = load_loopback(&format!("{CAPTURE_SINK}.monitor"), &default_sink)?;
                self.loopback_module = Some(id);
            }
            self.routing_mode = if self.follow_exclude_browser {
                "exclude-browser".into()
            } else {
                "apps".into()
            };
            self.pending_capture = Some(format!("{CAPTURE_SINK}.monitor"));
        } else {
            self.follow_keys.clear();
            self.follow_exclude_browser = false;
            self.routing_mode = "monitor".into();
            self.pending_capture =
                Some(source.filter(|s| !s.is_empty()).unwrap_or(default_monitor));
        }

        self.playback_isolated = false;
        Ok(())
    }

    pub async fn engage(&mut self) -> Result<EngageResult> {
        let mut isolated = self.routing_mode != "monitor";
        if self.routing_mode == "monitor" {
            isolated = self.isolate_relay_stream().await?;
            if !isolated {
                return Ok(EngageResult {
                    ok: true,
                    isolated: false,
                    routing_mode: self.routing_mode.clone(),
                });
            }
        }

        let capture_name = self
            .pending_capture
            .clone()
            .context("audio prepare was not called")?;
        self.spawn_parec(&capture_name).await?;
        self.capture_source = Some(capture_name);
        self.playback_isolated = isolated && self.routing_mode == "monitor";
        Ok(EngageResult {
            ok: true,
            isolated,
            routing_mode: self.routing_mode.clone(),
        })
    }

    pub fn follow_once(&mut self) -> Result<()> {
        if !self.needs_follow() {
            return Ok(());
        }
        if self.playback_isolated && self.routing_mode == "monitor" {
            return self.follow_isolated();
        }
        let capture_idx = sink_index(CAPTURE_SINK)?.unwrap_or(u32::MAX);
        let apps = Self::list_apps()?;
        let already: HashSet<u32> = self.moved.iter().map(|(index, _)| *index).collect();
        for app in apps {
            if skip_system_stream(&app) || already.contains(&app.index) || app.sink == capture_idx {
                continue;
            }
            let matches = if self.follow_exclude_browser {
                !app.browser
            } else {
                self.follow_keys
                    .iter()
                    .any(|(name, binary)| &app.name == name && &app.binary == binary)
            };
            if matches {
                self.move_app(&app, CAPTURE_SINK)?;
            }
        }
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.stop_capture().await?;
        self.restore_routing()?;
        self.routing_mode = "idle".into();
        self.capture_source = None;
        self.pending_capture = None;
        self.playback_isolated = false;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.stop().await;
        if let Some(id) = self.silent_module.take() {
            let _ = unload_module(&id);
        }
        Ok(())
    }

    fn ensure_capture_sink(&mut self) -> Result<()> {
        if sink_exists(CAPTURE_SINK)? {
            return Ok(());
        }
        let id = load_null_sink(CAPTURE_SINK, CAPTURE_LABEL)?;
        self.capture_module = Some(id);
        Ok(())
    }

    fn restore_routing(&mut self) -> Result<()> {
        for (input, sink) in self.moved.drain(..) {
            let _ = pactl_run(&["move-sink-input", &input.to_string(), &sink.to_string()]);
        }
        if let Some(id) = self.loopback_module.take() {
            let _ = unload_module(&id);
        }
        if let Some(id) = self.capture_module.take() {
            let _ = unload_module(&id);
        }
        self.snapshot.clear();
        self.follow_keys.clear();
        self.follow_exclude_browser = false;
        self.isolated_key = None;
        Ok(())
    }

    fn follow_isolated(&mut self) -> Result<()> {
        let Some((name, media, binary)) = self.isolated_key.clone() else {
            return Ok(());
        };
        let silent_idx = sink_index(SILENT_SINK)?.unwrap_or(u32::MAX);
        let apps = Self::list_apps()?;
        for app in apps {
            if app.sink == silent_idx || skip_system_stream(&app) {
                continue;
            }
            if app.name == name && app.media == media && app.binary == binary {
                self.move_app(&app, SILENT_SINK)?;
                info!("re-isolated playback sink-input {}", app.index);
            }
        }
        Ok(())
    }

    fn move_app(&mut self, app: &AudioApp, sink: &str) -> Result<()> {
        pactl_run(&["move-sink-input", &app.index.to_string(), sink])?;
        self.moved.push((app.index, app.sink));
        Ok(())
    }

    async fn isolate_relay_stream(&mut self) -> Result<bool> {
        let deadline = Instant::now() + Duration::from_millis(1500);
        loop {
            if let Some(app) = self.relay_on_silent()? {
                info!(
                    "playback already on silent sink {} ({}/{})",
                    app.index, app.name, app.media
                );
                self.isolated_key = Some((app.name.clone(), app.media.clone(), app.binary.clone()));
                self.playback_isolated = true;
                if !self.moved.iter().any(|(index, _)| *index == app.index) {
                    let default_idx = sink_index(&Self::default_sink()?)?.unwrap_or(app.sink);
                    self.moved.push((app.index, default_idx));
                }
                return Ok(true);
            }
            if let Some(app) = self.find_relay_stream()? {
                self.move_app(&app, SILENT_SINK)?;
                info!(
                    "isolated playback sink-input {} ({}/{})",
                    app.index, app.name, app.media
                );
                self.isolated_key = Some((app.name.clone(), app.media.clone(), app.binary.clone()));
                self.playback_isolated = true;
                return Ok(true);
            }
            if Instant::now() >= deadline {
                warn!("could not isolate relay playback stream");
                return Ok(false);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn relay_on_silent(&self) -> Result<Option<AudioApp>> {
        let silent_idx = sink_index(SILENT_SINK)?.unwrap_or(u32::MAX);
        Ok(Self::list_apps()?.into_iter().find(|app| {
            app.browser
                && app.sink == silent_idx
                && !skip_system_stream(app)
                && (!self.snapshot.contains(&app.index) || media_relay_score(&app.media) <= 1)
        }))
    }

    fn find_relay_stream(&self) -> Result<Option<AudioApp>> {
        let apps = Self::list_apps()?;
        let silent_idx = sink_index(SILENT_SINK)?.unwrap_or(u32::MAX);
        let mut candidates: Vec<AudioApp> = apps
            .into_iter()
            .filter(|app| {
                app.browser
                    && app.sink != silent_idx
                    && !skip_system_stream(app)
                    && (!self.snapshot.contains(&app.index) || media_relay_score(&app.media) == 0)
            })
            .collect();
        candidates.sort_by_key(|app| {
            let score = media_relay_score(&app.media);
            (score, std::cmp::Reverse(app.index))
        });
        Ok(candidates.into_iter().next())
    }

    async fn stop_capture(&mut self) -> Result<()> {
        if let Some(handle) = self.reader.take() {
            handle.abort();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(())
    }

    async fn spawn_parec(&mut self, source: &str) -> Result<()> {
        let mut cmd = Command::new("parec");
        cmd.args([
            "-d",
            source,
            "--rate=48000",
            "--format=s16le",
            "--channels=2",
            "--latency-msec=20",
            "--raw",
            "--client-name=meet123",
            "--stream-name=system-audio",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .context("failed to spawn parec (install pulseaudio-utils / libpulse)")?;
        let mut stdout = child.stdout.take().context("parec stdout")?;
        let tx = self.tx.clone();
        self.reader = Some(tokio::spawn(async move {
            let mut buf = vec![0u8; 3840];
            let mut leftover = Vec::new();
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        leftover.extend_from_slice(&buf[..n]);
                        let aligned = leftover.len() - (leftover.len() % 4);
                        if aligned > 0 {
                            let _ = tx.send(bytes::Bytes::copy_from_slice(&leftover[..aligned]));
                            leftover.drain(..aligned);
                        }
                        if leftover.len() > 16 {
                            leftover.clear();
                        }
                    }
                    Err(err) => {
                        warn!("parec read error: {err}");
                        break;
                    }
                }
            }
        }));
        self.child = Some(child);
        info!("recording {source}");
        Ok(())
    }
}

fn media_relay_score(media: &str) -> u8 {
    let media = media.to_ascii_lowercase();
    if media.contains("meet-123") || media.contains("meet123") {
        0
    } else if media.contains("audiocontext") || media.contains("web audio") {
        1
    } else {
        2
    }
}

fn skip_system_stream(app: &AudioApp) -> bool {
    let blob = format!("{} {} {}", app.name, app.media, app.binary).to_ascii_lowercase();
    blob.contains("loopback") || blob.contains("meet123")
}

fn is_browser(name: &str, binary: &str) -> bool {
    let blob = format!("{name} {binary}").to_ascii_lowercase();
    BROWSER_MARKERS.iter().any(|marker| blob.contains(marker))
}

fn prop_str(props: &Value, key: &str) -> String {
    props
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn json_u32(value: Option<&Value>) -> Option<u32> {
    match value? {
        Value::Number(n) => n.as_u64().map(|v| v as u32),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn pactl_run(args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("pactl")
        .args(args)
        .output()
        .with_context(|| format!("pactl {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "pactl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn pactl_json(args: &[&str]) -> Result<Value> {
    let mut full = vec!["--format=json"];
    full.extend_from_slice(args);
    let text = pactl_run(&full)?;
    serde_json::from_str(&text).context("pactl json")
}

fn sink_exists(name: &str) -> Result<bool> {
    Ok(sink_index(name)?.is_some())
}

fn sink_index(name: &str) -> Result<Option<u32>> {
    let sinks = pactl_json(&["list", "sinks"])?;
    Ok(sinks.as_array().into_iter().flatten().find_map(|sink| {
        if sink.get("name").and_then(Value::as_str) == Some(name) {
            json_u32(sink.get("index"))
        } else {
            None
        }
    }))
}

fn load_null_sink(name: &str, description: &str) -> Result<String> {
    let arg = format!("sink_name={name} sink_properties=device.description={description}");
    pactl_run(&["load-module", "module-null-sink", &arg])
}

fn load_loopback(source: &str, sink: &str) -> Result<String> {
    let arg = format!("source={source} sink={sink} latency_msec=5");
    pactl_run(&["load-module", "module-loopback", &arg])
}

fn unload_module(id: &str) -> Result<()> {
    pactl_run(&["unload-module", id]).map(|_| ())
}
