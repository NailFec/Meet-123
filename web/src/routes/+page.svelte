<script lang="ts">
	import { engageAudio, fetchApps, fetchSources, fetchStatus, prepareAudio, stopAudio } from '$lib/api';
	import { startPcmPlayback, type AudioPlayback } from '$lib/audio';
	import { captureDisplay, PRESETS, readCaptureStats, stopStream } from '$lib/capture';
	import { onPageVisible, pageIsHidden, setWakeLock } from '$lib/keepalive';
	import type { AudioApp, AudioSource, CaptureStats, HelperStatus } from '$lib/types';

	let presenting = $state(false);
	let helper = $state.raw<HelperStatus | null>(null);
	let helperError = $state('');
	let sources = $state.raw<AudioSource[]>([]);
	let apps = $state.raw<AudioApp[]>([]);
	let selectedSource = $state('');
	let selectedApps = $state.raw<number[]>([]);
	let excludeBrowser = $state(false);

	let presetId = $state('1440p60');
	let customWidth = $state(2560);
	let customHeight = $state(1440);
	let customFps = $state(60);
	let objectFit = $state<'contain' | 'cover'>('contain');

	let stream = $state.raw<MediaStream | null>(null);
	let stats = $state.raw<CaptureStats | null>(null);
	let captureError = $state('');

	let audioBusy = $state(false);
	let audioActive = $state(false);
	let audioError = $state('');
	let playback = $state.raw<AudioPlayback | null>(null);
	let audioLevel = $state(0);
	let stopPlayback: (() => Promise<void>) | null = null;
	let surfaceHidden = $state(false);

	const capturing = $derived(stream !== null);
	const keepAlive = $derived(capturing || audioActive || presenting);
	const preset = $derived(PRESETS.find((item) => item.id === presetId) ?? PRESETS[3]);
	const target = $derived({
		width: preset.width || customWidth,
		height: preset.height || customHeight,
		frameRate: preset.frameRate || customFps
	});

	void refreshHelper();
	surfaceHidden = pageIsHidden();

	$effect(() => {
		void setWakeLock(keepAlive);
	});

	function attachStage(node: HTMLVideoElement) {
		const current = stream;
		node.srcObject = current;
		if (current) void node.play().catch(() => {});
		if (!current) {
			stats = null;
			return () => {
				node.srcObject = null;
			};
		}
		const tick = () => {
			stats = readCaptureStats(current);
		};
		tick();
		const id = window.setInterval(tick, 500);
		return () => {
			clearInterval(id);
			node.srcObject = null;
		};
	}

	async function refreshHelper() {
		try {
			helper = await fetchStatus();
			sources = await fetchSources();
			apps = await fetchApps();
			helperError = '';
			if (!selectedSource) {
				const fallback = sources.find((item) => item.is_default) ?? sources[0];
				selectedSource = fallback?.name ?? '';
			}
		} catch (error) {
			helperError = error instanceof Error ? error.message : String(error);
		}
	}

	async function startCapture() {
		captureError = '';
		try {
			const next = await captureDisplay(target);
			stopStream(stream);
			stream = next;
			const track = next.getVideoTracks()[0];
			track?.addEventListener(
				'ended',
				() => {
					stream = null;
					presenting = false;
				},
				{ once: true }
			);
		} catch (error) {
			captureError = error instanceof Error ? error.message : String(error);
		}
	}

	function endCapture() {
		stopStream(stream);
		stream = null;
		presenting = false;
	}

	function toggleApp(index: number) {
		if (selectedApps.includes(index)) {
			selectedApps = selectedApps.filter((item) => item !== index);
		} else {
			selectedApps = [...selectedApps, index];
		}
	}

	async function startSystemAudio() {
		audioBusy = true;
		audioError = '';
		audioLevel = 0;
		try {
			await refreshHelper();
			const useApps = selectedApps.length > 0;
			const source = useApps ? undefined : selectedSource || undefined;
			await prepareAudio({
				source,
				app_indices: useApps ? selectedApps : undefined,
				exclude_browser: excludeBrowser || undefined,
				loopback: true
			});
			const session = await startPcmPlayback({
				onLevel: (rms) => {
					audioLevel = rms;
				}
			});
			stopPlayback = session.stop;
			await new Promise((resolve) => window.setTimeout(resolve, 120));
			let result = await engageAudio();
			if (!result.isolated && !excludeBrowser && !useApps) {
				excludeBrowser = true;
				await prepareAudio({
					source,
					exclude_browser: true,
					loopback: true
				});
				result = await engageAudio();
			}
			if (!result.isolated) {
				throw new Error('无法隔离中转音频流');
			}
			await session.connectSocket();
			session.fadeIn();
			const mode = useApps ? 'apps' : result.isolated && !excludeBrowser ? 'isolated' : 'exclude-browser';
			playback = {
				mode,
				sinkLabel:
					mode === 'isolated'
						? '已隔离中转流（本机听感不变）'
						: mode === 'exclude-browser'
							? '排除浏览器（本机可能多一点延迟）'
							: '指定应用'
			};
			audioActive = true;
		} catch (error) {
			audioError = error instanceof Error ? error.message : String(error);
			if (stopPlayback) await stopPlayback().catch(() => {});
			stopPlayback = null;
			playback = null;
			await stopAudio().catch(() => {});
		} finally {
			audioBusy = false;
		}
	}

	async function endSystemAudio() {
		audioBusy = true;
		try {
			if (stopPlayback) await stopPlayback();
			stopPlayback = null;
			playback = null;
			audioLevel = 0;
			audioActive = false;
			await stopAudio().catch(() => {});
		} finally {
			audioBusy = false;
		}
	}

	async function enterPresent() {
		presenting = true;
		try {
			await document.documentElement.requestFullscreen();
		} catch {
			// Tab capture still works without OS fullscreen.
		}
	}

	async function exitPresent() {
		presenting = false;
		if (document.fullscreenElement) {
			await document.exitFullscreen().catch(() => {});
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && presenting) {
			event.preventDefault();
			void exitPresent();
		}
		if (event.key === 'f' && !event.ctrlKey && !event.metaKey && !event.altKey) {
			const tag = (event.target as HTMLElement | null)?.tagName;
			if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
			event.preventDefault();
			if (presenting) void exitPresent();
			else void enterPresent();
		}
	}

	function onVisibilityChange() {
		surfaceHidden = pageIsHidden();
		if (!surfaceHidden) void onPageVisible();
	}

	function onFullscreenChange() {
		if (!document.fullscreenElement) presenting = false;
	}
</script>

<svelte:window onkeydown={onKeydown} />
<svelte:document onfullscreenchange={onFullscreenChange} onvisibilitychange={onVisibilityChange} />

<div class={['shell', presenting && 'presenting']}>
	<section class="stage">
		{#key stream?.id ?? 'none'}
			<video {@attach attachStage} class={objectFit} autoplay playsinline muted></video>
		{/key}
		{#if !capturing}
			<div class="placeholder">
				<p>先捕获一块<strong>不会放这个标签页</strong>的屏幕或窗口。</p>
				<p class="muted">Google Meet 里再共享本标签页，并勾选标签页音频。</p>
			</div>
		{/if}
		{#if presenting}
			<div class="present-hint">按 Esc 或 F 退出演示布局</div>
		{/if}
		{#if surfaceHidden && capturing}
			<div class="hidden-warn">
				窗口当前不可见。niri 对看不见的窗口大约 1fps；请放到另一块仍在显示的屏幕上，不要藏进后台
				workspace。
			</div>
		{/if}
	</section>

	<aside class="panel">
		<header>
			<h1>Meet-123</h1>
			<p>Linux 整屏中转 · 系统音频走标签页</p>
		</header>

		<section>
			<h2>1. 画面</h2>
			<label>
				质量预设
				<select bind:value={presetId}>
					{#each PRESETS as item (item.id)}
						<option value={item.id}>{item.label}</option>
					{/each}
				</select>
			</label>
			{#if presetId === 'custom'}
				<div class="row">
					<label>宽 <input type="number" min="640" bind:value={customWidth} /></label>
					<label>高 <input type="number" min="360" bind:value={customHeight} /></label>
					<label>fps <input type="number" min="1" max="240" bind:value={customFps} /></label>
				</div>
			{/if}
			<label>
				画面适配
				<select bind:value={objectFit}>
					<option value="contain">完整显示</option>
					<option value="cover">铺满裁切</option>
				</select>
			</label>
			<div class="row">
				<button class="primary" onclick={startCapture} disabled={capturing}>捕获屏幕 / 窗口</button>
				<button onclick={endCapture} disabled={!capturing}>停止画面</button>
			</div>
			{#if stats}
				<p class="stats">
					实际 {stats.width}×{stats.height} @ {stats.frameRate.toFixed(1)}fps
					{#if stats.label}<span class="muted"> · {stats.label}</span>{/if}
				</p>
			{/if}
			{#if captureError}
				<p class="error">{captureError}</p>
			{/if}
		</section>

		<section>
			<h2>2. 电脑音频</h2>
			{#if helperError}
				<p class="error">助手未连接：{helperError}。请先运行 <code>meet123</code>。</p>
			{:else if helper}
				<p class="muted">
					桌面 {helper.desktop || '未知'} · 会话 {helper.session || '?'} · 静音槽
					{helper.silent_sink_ready ? '已就绪' : '未就绪'}
				</p>
			{/if}
			<label>
				监听源（默认＝你正在听的输出）
				<select bind:value={selectedSource} disabled={audioActive}>
					{#each sources as source (source.name)}
						<option value={source.name}>
							{source.description}{source.is_default ? '（默认）' : ''}
						</option>
					{/each}
				</select>
			</label>
			<details>
				<summary>只共享某些应用（可选）</summary>
				<p class="muted">勾选后只采集这些应用，并临时把它们改到中转用虚拟槽。</p>
				<div class="apps">
					{#each apps as app (app.index)}
						<label class="app">
							<input
								type="checkbox"
								checked={selectedApps.includes(app.index)}
								onchange={() => toggleApp(app.index)}
								disabled={audioActive}
							/>
							<span>{app.name}{app.media ? ` · ${app.media}` : ''}</span>
							{#if app.browser}<em>浏览器</em>{/if}
						</label>
					{/each}
					{#if apps.length === 0}
						<p class="muted">当前没有可列出的播放流。</p>
					{/if}
				</div>
			</details>
			<label class="app">
				<input type="checkbox" bind:checked={excludeBrowser} disabled={audioActive} />
				排除浏览器自身（隔离失败时的回授兜底）
			</label>
			<div class="row">
				<button class="primary" onclick={startSystemAudio} disabled={audioBusy || audioActive}>
					开始注入系统音频
				</button>
				<button onclick={endSystemAudio} disabled={audioBusy || !audioActive}>停止音频</button>
				<button onclick={refreshHelper} disabled={audioBusy}>刷新设备</button>
			</div>
			{#if playback}
				<p class="stats">
					播放出口：{playback.sinkLabel}
					<span class="muted"> · 电平 {(audioLevel * 100).toFixed(0)}%</span>
				</p>
			{/if}
			{#if audioError}
				<p class="error">{audioError}</p>
			{/if}
		</section>

		<section>
			<h2>3. 交给 Meet</h2>
			<ol>
				<li>把本标签页放到<strong>另一块仍在显示的屏幕</strong>上全屏（不要放进 niri 看不见的 workspace）。</li>
				<li>Meet → 立即展示 → <strong>一个标签页</strong> → 勾选共享标签页音频。</li>
				<li>Meet 里打开「优化动态视频」，不要用优化文字。</li>
			</ol>
			<div class="row">
				<button class="primary" onclick={enterPresent} disabled={!capturing}>演示布局</button>
				<button onclick={exitPresent} disabled={!presenting}>退出演示</button>
			</div>
		</section>

		<section>
			<h2>提示</h2>
			<ul>
				{#if helper}
					{#each helper.tips as tip (tip)}
						<li>{tip}</li>
					{/each}
				{:else}
					<li>双屏：中转页在 A 屏全屏，捕获 B 屏，避免把本页采进去。</li>
					<li>Meet 会再编码，对端通常到不了你设的 120fps；源越清晰，下行越不容易糊。</li>
				{/if}
			</ul>
		</section>
	</aside>
</div>

<style>
	.shell {
		display: grid;
		grid-template-columns: 1fr 380px;
		min-height: 100vh;
	}

	.shell.presenting {
		grid-template-columns: 1fr;
	}

	.shell.presenting .panel {
		display: none;
	}

	.stage {
		position: relative;
		background: var(--stage);
		min-height: 100vh;
		overflow: hidden;
	}

	video {
		width: 100%;
		height: 100%;
		min-height: 100vh;
		background: #000;
	}

	video.contain {
		object-fit: contain;
	}

	video.cover {
		object-fit: cover;
	}

	.placeholder,
	.present-hint {
		position: absolute;
		inset: auto 1.5rem 1.5rem;
		color: var(--muted);
		pointer-events: none;
	}

	.placeholder {
		top: 50%;
		transform: translateY(-50%);
		text-align: center;
		inset: auto 2rem;
	}

	.present-hint {
		opacity: 0;
		transition: opacity 0.3s;
		font-size: 0.85rem;
	}

	.hidden-warn {
		position: absolute;
		top: 1rem;
		left: 1rem;
		right: 1rem;
		padding: 0.65rem 0.85rem;
		background: #3b2f12;
		color: var(--warn);
		border-radius: 8px;
		font-size: 0.9rem;
	}

	.stage:hover .present-hint {
		opacity: 1;
	}

	.panel {
		border-left: 1px solid var(--line);
		background: var(--bg-panel);
		padding: 1.1rem 1.15rem 2rem;
		overflow: auto;
	}

	header h1 {
		margin: 0;
		font-size: 1.25rem;
	}

	header p,
	.muted {
		color: var(--muted);
	}

	section {
		margin-top: 1.15rem;
	}

	h2 {
		margin: 0 0 0.55rem;
		font-size: 0.95rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		margin-bottom: 0.65rem;
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.45rem;
		margin: 0.4rem 0 0.7rem;
	}

	.stats,
	.error,
	ol,
	ul {
		font-size: 0.9rem;
	}

	.error {
		color: var(--danger);
	}

	ol,
	ul {
		padding-left: 1.15rem;
		margin: 0.3rem 0 0.7rem;
	}

	li + li {
		margin-top: 0.35rem;
	}

	.apps {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		max-height: 10rem;
		overflow: auto;
		margin: 0.4rem 0;
	}

	.app {
		flex-direction: row;
		align-items: center;
		gap: 0.45rem;
	}

	.app em {
		color: var(--muted);
		font-style: normal;
		font-size: 0.8rem;
	}

	details {
		margin-bottom: 0.7rem;
	}

	code {
		font-size: 0.85em;
	}

	@media (max-width: 960px) {
		.shell:not(.presenting) {
			grid-template-columns: 1fr;
		}

		.panel {
			border-left: 0;
			border-top: 1px solid var(--line);
		}

		video,
		.stage {
			min-height: 45vh;
		}
	}
</style>
