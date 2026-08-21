# Meet-123

Linux 上 Google Meet **网页版无法整屏带系统音频** 的中转工具。把指定屏幕/窗口画到一个 Chrome 标签页里，再把电脑正在播放的声音注入这个标签页，然后在 Meet 里 **共享该标签页并勾选标签页音频**。

不绑定某种桌面：画面走当前会话的 `xdg-desktop-portal`（niri / KDE Plasma / GNOME 各自的选择器），声音走 PipeWire（Pulse 兼容层）。不需要 root，也不会改 niri 或 KWin 配置。

## 限制（做不到的）

- Meet 会再编码一次。本页可以按 1440p / 60–120fps 捕获，对端通常仍是大约 1080p、15–30fps。
- 不要捕获「放着这个中转页」的那块屏，否则会镜像回授。双屏：中转页在 A 屏全屏，捕获 B 屏。
- 系统音频在 Linux 上不能靠 Meet 的「共享整屏」开关；只能走标签页音频。

## 依赖

- Chromium 内核浏览器：Chrome / Chromium / Edge / Vivaldi / Brave
- PipeWire（或 PulseAudio）以及 `pactl`、`parec`（Arch/CachyOS 一般在 `libpulse` / `pulseaudio-utils`）
- 屏幕共享门户：KDE 用 `xdg-desktop-portal-kde`，niri 用 `xdg-desktop-portal-gnome`，GNOME 同 gnome 门户
- 构建：Node.js、Rust

## 使用

```bash
cd web && npm install && npm run build
cd ../helper && cargo run --release
```

助手会创建静音输出 `Meet123Silent`、在托盘显示图标（没有托盘也没关系），并打开 Chromium 内核浏览器。

1. 把中转页放到 **不打算共享** 的屏幕，必要时点「演示布局」或按 `F` 全屏。
2. 选质量预设，点「捕获屏幕 / 窗口」，在系统对话框里选 **另一块屏或某个窗口**。
3. 点「开始注入系统音频」（默认采集你正在听的输出）。
4. Google Meet → **立即展示** → **一个标签页** → 勾选共享标签页音频。
5. Meet 里打开 **优化动态视频**。

开发时可以两边一起跑：

```bash
# 终端 1
cd helper && cargo run -- --no-open

# 终端 2
cd web && npm run dev
```

常用参数：`--listen 127.0.0.1:17373`、`--no-open`、`--no-tray`。

## 音频回授

中转页会尽量把播放出口切到 `Meet123Silent`，本机听感不变，Meet 采的是标签页内部声音。如果浏览器列不出这个设备，页面会改走「排除浏览器」路由，避免自己录自己。也可以手动勾选「只共享某些应用」。

## 桌面差异（可选，不是依赖）

- **所有桌面**：用系统弹出的屏幕/窗口选择器即可。
- **niri**：可以选 Dynamic Cast Target，再用 niri 快捷键改捕获源；可用 `block-out-from "screencast"` 挡住 Meet/密码窗口。本程序不会写 `config.kdl`。
- **KDE Plasma**：用 `xdg-desktop-portal-kde` 的对话框。本程序不会写 `kwinrc`。

## 仓库结构

- [`web/`](web/) SvelteKit 静态中转页
- [`helper/`](helper/) 用户态助手：PipeWire/Pulse 采集、WebSocket PCM、托盘、打开浏览器
