use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

const CANDIDATES: &[&str] = &[
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "microsoft-edge-stable",
    "microsoft-edge",
    "vivaldi-stable",
    "vivaldi",
    "brave-browser",
    "brave",
    "google-chrome-unstable",
    "google-chrome-beta",
];

pub fn available_browsers() -> Vec<String> {
    CANDIDATES
        .iter()
        .filter(|name| which(name).is_some())
        .map(|name| name.to_string())
        .collect()
}

pub fn open_in_chromium(url: &str) -> Result<String> {
    let browsers = available_browsers();
    let Some(bin) = browsers.first() else {
        open::that(url)?;
        return Ok("system-default".into());
    };
    Command::new(bin).args(["--new-window", url]).spawn()?;
    Ok(bin.clone())
}

fn which(bin: &str) -> Option<PathBuf> {
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
