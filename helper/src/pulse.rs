use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::process::Stdio;
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

pub struct Pulse {
    silent_module: Option<String>,
    capture_module: Option<String>,
    loopback_module: Option<String>,
    moved: Vec<(u32, u32)>,
    child: Option<Child>,
    reader: Option<tokio::task::JoinHandle<()>>,
    pub capture_source: Option<String>,
    pub routing_mode: String,
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
            child: None,
            reader: None,
            capture_source: None,
            routing_mode: "idle".into(),
            tx,
        }
    }

    pub fn silent_ready(&self) -> bool {
        sink_exists(SILENT_SINK).unwrap_or(false)
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
        out.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.description.cmp(&b.description)));
        Ok(out)
    }

    pub fn list_apps() -> Result<Vec<AudioApp>> {
        let list = pactl_json(&["list", "sink-inputs"])?;
        let mut out = Vec::new();
        if let Some(arr) = list.as_array() {
            for input in arr {
                let Some(index) = json_u32(input.get("index")) else { continue };
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

    pub async fn start(
        &mut self,
        source: Option<String>,
        app_indices: Vec<u32>,
        exclude_browser: bool,
        loopback: bool,
    ) -> Result<()> {
        self.stop_capture().await?;
        self.restore_routing()?;

        let default_sink = Self::default_sink()?;
        let default_monitor = format!("{default_sink}.monitor");

        let capture_name = if !app_indices.is_empty() || exclude_browser {
            self.ensure_capture_sink()?;
            let apps = Self::list_apps()?;
            let selected: Vec<AudioApp> = if !app_indices.is_empty() {
                apps.into_iter()
                    .filter(|app| app_indices.contains(&app.index))
                    .collect()
            } else {
                apps.into_iter().filter(|app| !app.browser).collect()
            };
            for app in &selected {
                pactl_run(&["move-sink-input", &app.index.to_string(), CAPTURE_SINK])?;
                self.moved.push((app.index, app.sink));
            }
            if loopback {
                let id = load_loopback(&format!("{CAPTURE_SINK}.monitor"), &default_sink)?;
                self.loopback_module = Some(id);
            }
            self.routing_mode = if exclude_browser && app_indices.is_empty() {
                "exclude-browser".into()
            } else {
                "apps".into()
            };
            format!("{CAPTURE_SINK}.monitor")
        } else {
            self.routing_mode = "monitor".into();
            source.filter(|s| !s.is_empty()).unwrap_or(default_monitor)
        };

        self.spawn_parec(&capture_name).await?;
        self.capture_source = Some(capture_name);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.stop_capture().await?;
        self.restore_routing()?;
        self.routing_mode = "idle".into();
        self.capture_source = None;
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
        Ok(())
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

        let mut child = cmd.spawn().context("failed to spawn parec (install pulseaudio-utils / libpulse)")?;
        let mut stdout = child.stdout.take().context("parec stdout")?;
        let tx = self.tx.clone();
        self.reader = Some(tokio::spawn(async move {
            let mut buf = vec![0u8; 3840];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = tx.send(bytes::Bytes::copy_from_slice(&buf[..n]));
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
    let sinks = pactl_json(&["list", "sinks"])?;
    Ok(sinks
        .as_array()
        .into_iter()
        .flatten()
        .any(|sink| sink.get("name").and_then(Value::as_str) == Some(name)))
}

fn load_null_sink(name: &str, description: &str) -> Result<String> {
    let arg = format!("sink_name={name} sink_properties=device.description={description}");
    pactl_run(&["load-module", "module-null-sink", &arg])
}

fn load_loopback(source: &str, sink: &str) -> Result<String> {
    let arg = format!("source={source} sink={sink} latency_msec=20");
    pactl_run(&["load-module", "module-loopback", &arg])
}

fn unload_module(id: &str) -> Result<()> {
    pactl_run(&["unload-module", id]).map(|_| ())
}
