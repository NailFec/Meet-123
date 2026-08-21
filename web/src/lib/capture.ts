import type { CaptureStats, QualityPreset } from './types';

export const PRESETS: QualityPreset[] = [
	{ id: '1080p30', label: '1080p · 30fps', width: 1920, height: 1080, frameRate: 30 },
	{ id: '1080p60', label: '1080p · 60fps', width: 1920, height: 1080, frameRate: 60 },
	{ id: '1440p30', label: '1440p · 30fps', width: 2560, height: 1440, frameRate: 30 },
	{ id: '1440p60', label: '1440p · 60fps', width: 2560, height: 1440, frameRate: 60 },
	{ id: '1440p120', label: '1440p · 120fps', width: 2560, height: 1440, frameRate: 120 },
	{ id: 'custom', label: '自定义', width: 0, height: 0, frameRate: 0 }
];

type DisplayMediaExtra = DisplayMediaStreamOptions & {
	selfBrowserSurface?: 'include' | 'exclude';
	surfaceSwitching?: 'include' | 'exclude';
	systemAudio?: 'include' | 'exclude';
	preferCurrentTab?: boolean;
	monitorTypeSurfaces?: 'include' | 'exclude';
};

type DisplayTrackConstraints = MediaTrackConstraints & {
	cursor?: 'always' | 'motion' | 'never';
	displaySurface?: string;
};

export async function captureDisplay(opts: {
	width: number;
	height: number;
	frameRate: number;
}): Promise<MediaStream> {
	const video: DisplayTrackConstraints = {
		width: { ideal: opts.width },
		height: { ideal: opts.height },
		frameRate: { ideal: opts.frameRate, max: opts.frameRate },
		cursor: 'always',
		displaySurface: 'monitor'
	};

	const extra: DisplayMediaExtra = {
		video,
		audio: false,
		selfBrowserSurface: 'exclude',
		surfaceSwitching: 'include',
		systemAudio: 'exclude',
		preferCurrentTab: false
	};

	const stream = await navigator.mediaDevices.getDisplayMedia(extra);
	const track = stream.getVideoTracks()[0];
	if (track) {
		track.contentHint = 'motion';
		try {
			await track.applyConstraints({
				width: { ideal: opts.width },
				height: { ideal: opts.height },
				frameRate: { ideal: opts.frameRate, max: opts.frameRate }
			});
		} catch {
			// Constraints are advisory; the portal / compositor decides.
		}
	}
	return stream;
}

export function readCaptureStats(stream: MediaStream | null): CaptureStats | null {
	const track = stream?.getVideoTracks()[0];
	if (!track) return null;
	const settings = track.getSettings();
	const width = settings.width ?? 0;
	const height = settings.height ?? 0;
	const frameRate = settings.frameRate ?? 0;
	const surface = settings.displaySurface ?? '';
	const label = [track.label, surface].filter(Boolean).join(' · ');
	return { width, height, frameRate, label };
}

export function stopStream(stream: MediaStream | null) {
	if (!stream) return;
	for (const track of stream.getTracks()) track.stop();
}
