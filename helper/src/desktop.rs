pub fn desktop_name() -> String {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
}

pub fn session_type() -> String {
    std::env::var("XDG_SESSION_TYPE").unwrap_or_default()
}

pub fn tips(desktop: &str) -> Vec<String> {
    let mut tips = vec![
        "双屏：中转页放在不共享的那块屏并全屏，捕获另一块屏或窗口，避免把本页采进去。".into(),
        "Meet → 立即展示 → 一个标签页 → 勾选共享标签页音频。不要选整屏/窗口来带系统音频。".into(),
        "Meet 里打开「优化动态视频」。对端会再编码，通常到不了你设的 120fps。".into(),
        "画面选择器由当前桌面的 xdg-desktop-portal 提供，本程序不绑定 niri 或 KWin。".into(),
    ];
    let desktop = desktop.to_ascii_lowercase();
    if desktop.contains("niri") {
        tips.push(
            "niri：中转窗口必须落在一块「正在显示」的输出上。划到看不见的 workspace / 视口外时，niri 大约只给 1Hz 刷新，Meet 会非常卡。可以放在另一块显示器上不聚焦，但不要藏进后台。"
                .into(),
        );
        tips.push(
            "niri 可选：捕获时选 Dynamic Cast Target，再用 niri 快捷键切换窗口/显示器。本程序不会改 config.kdl。"
                .into(),
        );
        tips.push(
            "niri 可选：window-rule 使用 block-out-from \"screencast\" 可挡住 Meet/密码窗口。"
                .into(),
        );
    }
    if desktop.contains("kde") || desktop.contains("plasma") {
        tips.push(
            "KDE：换虚拟桌面通常比 niri 藏窗口更顺（概览还要缩略图）。不要最小化中转窗口；被完全挡住时 Chrome 也可能掉帧。"
                .into(),
        );
        tips.push(
            "KDE：用系统自带的屏幕/窗口对话框即可，需要 xdg-desktop-portal-kde。本程序不会改 kwinrc。"
                .into(),
        );
    }
    if desktop.contains("gnome") && !desktop.contains("niri") {
        tips.push("GNOME：在系统屏幕共享对话框里选择显示器或窗口。".into());
    }
    if session_type().eq_ignore_ascii_case("wayland") {
        tips.push(
            "Wayland：若浏览器只能看到标签页、看不到整屏，确认 Chrome 走 Wayland（Ozone）且 portal 后端在跑。"
                .into(),
        );
    }
    tips
}
