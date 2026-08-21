use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const CANDIDATES: &[&str] = &[
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "vivaldi-stable",
    "vivaldi",
    "brave-browser",
    "brave",
    "microsoft-edge-stable",
    "microsoft-edge",
    "google-chrome-unstable",
    "google-chrome-beta",
];

static PREFERRED: OnceLock<String> = OnceLock::new();

pub fn set_preferred(browser: Option<String>) {
    if let Some(name) = browser.filter(|s| !s.trim().is_empty()) {
        let _ = PREFERRED.set(name);
    }
}

pub fn available_browsers() -> Vec<String> {
    CANDIDATES
        .iter()
        .filter(|name| which(name).is_some())
        .map(|name| name.to_string())
        .collect()
}

pub fn chosen_browser() -> Option<String> {
    pick_browser(PREFERRED.get().map(String::as_str))
}

pub fn open_in_chromium(url: &str) -> Result<String> {
    let Some(bin) = chosen_browser() else {
        open::that(url)?;
        return Ok("system-default".into());
    };
    Command::new(&bin)
        .args([
            "--disable-backgrounding-occluded-windows",
            "--disable-renderer-backgrounding",
            "--disable-background-timer-throttling",
            "--disable-background-media-suspend",
            "--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling",
            "--autoplay-policy=no-user-gesture-required",
            "--new-window",
            url,
        ])
        .spawn()?;
    Ok(bin)
}

fn pick_browser(preferred: Option<&str>) -> Option<String> {
    let available = available_browsers();
    if let Some(pref) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
        if which(pref).is_some() {
            return Some(pref.to_string());
        }
        let lower = pref.to_ascii_lowercase();
        if let Some(hit) = available.iter().find(|bin| {
            let bin_l = bin.to_ascii_lowercase();
            bin_l == lower || bin_l.contains(&lower) || lower.contains(&bin_l)
        }) {
            return Some(hit.clone());
        }
        tracing::warn!("preferred browser {pref} not found, falling back");
    }
    available.into_iter().next()
}

fn which(bin: &str) -> Option<PathBuf> {
    if bin.contains('/') {
        let path = PathBuf::from(bin);
        return path.is_file().then_some(path);
    }
    let Ok(path) = std::env::var("PATH") else {
        return None;
    };
    for dir in path.split(':') {
        let candidate = PathBuf::from(dir).join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
