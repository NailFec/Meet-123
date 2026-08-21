let sentinel: WakeLockSentinel | null = null;
let wantLock = false;

export async function setWakeLock(enabled: boolean): Promise<void> {
	wantLock = enabled;
	if (!enabled) {
		await sentinel?.release().catch(() => {});
		sentinel = null;
		return;
	}
	await acquire();
}

export function pageIsHidden(): boolean {
	return typeof document !== 'undefined' && document.visibilityState === 'hidden';
}

async function acquire() {
	if (!wantLock || typeof navigator === 'undefined' || !('wakeLock' in navigator)) return;
	try {
		sentinel = await navigator.wakeLock.request('screen');
		sentinel.addEventListener(
			'release',
			() => {
				sentinel = null;
				if (wantLock && !pageIsHidden()) void acquire();
			},
			{ once: true }
		);
	} catch {
		// Wake Lock is optional; niri off-screen windows still need to stay on an output.
	}
}

export async function onPageVisible() {
	if (wantLock && !sentinel) await acquire();
}
