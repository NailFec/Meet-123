export type QualityPreset = {
	id: string;
	label: string;
	width: number;
	height: number;
	frameRate: number;
};

export type AudioSource = {
	name: string;
	description: string;
	is_default: boolean;
};

export type AudioApp = {
	index: number;
	name: string;
	media: string;
	binary: string;
	sink: number;
	browser: boolean;
};

export type HelperStatus = {
	ok: boolean;
	desktop: string;
	session: string;
	tips: string[];
	silent_sink: string;
	silent_sink_ready: boolean;
	silent_sink_label: string;
	default_sink: string;
	default_monitor: string;
	audio_running: boolean;
	capture_source: string | null;
	routing_mode: string;
	playback_isolated: boolean;
	browsers: string[];
	listen: string;
};

export type AudioEngageResult = {
	ok: boolean;
	isolated: boolean;
	routing_mode: string;
};

export type CaptureStats = {
	width: number;
	height: number;
	frameRate: number;
	label: string;
};
