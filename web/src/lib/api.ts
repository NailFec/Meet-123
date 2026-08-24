import type { AudioApp, AudioEngageResult, AudioSource, HelperStatus } from './types';

async function readError(res: Response): Promise<string> {
	try {
		const body = await res.json();
		if (body && typeof body.error === 'string') return body.error;
	} catch {
		// ignore
	}
	return `${res.status} ${res.statusText}`;
}

export async function fetchStatus(): Promise<HelperStatus> {
	const res = await fetch('/api/status');
	if (!res.ok) throw new Error(await readError(res));
	return res.json();
}

export async function fetchSources(): Promise<AudioSource[]> {
	const res = await fetch('/api/sources');
	if (!res.ok) throw new Error(await readError(res));
	return res.json();
}

export async function fetchApps(): Promise<AudioApp[]> {
	const res = await fetch('/api/apps');
	if (!res.ok) throw new Error(await readError(res));
	return res.json();
}

export async function prepareAudio(body: {
	source?: string;
	app_indices?: number[];
	exclude_browser?: boolean;
	loopback?: boolean;
}): Promise<void> {
	const res = await fetch('/api/audio/prepare', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) throw new Error(await readError(res));
}

export async function engageAudio(): Promise<AudioEngageResult> {
	const res = await fetch('/api/audio/engage', { method: 'POST' });
	if (!res.ok) throw new Error(await readError(res));
	return res.json();
}

export async function stopAudio(): Promise<void> {
	const res = await fetch('/api/audio/stop', { method: 'POST' });
	if (!res.ok) throw new Error(await readError(res));
}

export async function openBrowser(): Promise<void> {
	const res = await fetch('/api/open-browser', { method: 'POST' });
	if (!res.ok) throw new Error(await readError(res));
}

export function audioSocketUrl(): string {
	const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
	return `${proto}//${location.host}/ws/audio`;
}
