use crate::browser;
use ksni::menu::StandardItem;
use ksni::{MenuItem, Tray, TrayMethods};
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::warn;

pub struct MeetTray {
    url: String,
    shutdown: Arc<Notify>,
}

impl Tray for MeetTray {
    fn id(&self) -> String {
        "meet123".into()
    }

    fn title(&self) -> String {
        "Meet-123".into()
    }

    fn icon_name(&self) -> String {
        "video-display".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        if let Err(err) = browser::open_in_chromium(&self.url) {
            warn!("tray open failed: {err}");
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "打开中转页".into(),
                activate: Box::new(|this: &mut Self| {
                    if let Err(err) = browser::open_in_chromium(&this.url) {
                        warn!("tray open failed: {err}");
                    }
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "退出".into(),
                activate: Box::new(|this: &mut Self| this.shutdown.notify_waiters()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub async fn spawn(url: String, shutdown: Arc<Notify>) -> Option<ksni::Handle<MeetTray>> {
    match (MeetTray { url, shutdown })
        .assume_sni_available(true)
        .spawn()
        .await
    {
        Ok(handle) => Some(handle),
        Err(err) => {
            warn!("tray not available: {err}");
            None
        }
    }
}
