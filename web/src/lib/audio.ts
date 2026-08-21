import { audioSocketUrl } from './api';

const WORKLET = `
class PcmPlayerProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.capacity = 48000 * 2 * 4;
    this.buffer = new Float32Array(this.capacity);
    this.read = 0;
    this.write = 0;
    this.filled = 0;
    this.port.onmessage = (event) => {
      const src = new Int16Array(event.data);
      for (let i = 0; i < src.length; i++) {
        if (this.filled >= this.capacity) {
          this.read = (this.read + 2) % this.capacity;
          this.filled -= 2;
        }
        this.buffer[this.write] = src[i] / 32768;
        this.write = (this.write + 1) % this.capacity;
        this.filled++;
      }
    };
  }

  process(_inputs, outputs) {
    const output = outputs[0];
    const left = output[0];
    const right = output[1] || output[0];
    for (let i = 0; i < left.length; i++) {
      if (this.filled >= 2) {
        left[i] = this.buffer[this.read];
        this.read = (this.read + 1) % this.capacity;
        right[i] = this.buffer[this.read];
        this.read = (this.read + 1) % this.capacity;
        this.filled -= 2;
      } else {
        left[i] = 0;
        right[i] = 0;
      }
    }
    return true;
  }
}
registerProcessor('pcm-player', PcmPlayerProcessor);
`;

type AudioContextWithSink = AudioContext & {
	setSinkId?: (id: string) => Promise<void>;
	sinkId?: string;
};

export type AudioPlayback = {
	sinkMode: 'silent' | 'default';
	sinkLabel: string;
};

export async function findSilentOutputId(labelHint: string): Promise<{
	id: string;
	label: string;
} | null> {
	const probe = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
	probe.getTracks().forEach((track) => track.stop());
	const devices = await navigator.mediaDevices.enumerateDevices();
	const re = /meet123|Meet123Silent/i;
	const extra = labelHint ? new RegExp(labelHint.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'i') : null;
	const hit = devices.find((device) => {
		if (device.kind !== 'audiooutput') return false;
		return re.test(device.label) || (extra ? extra.test(device.label) : false);
	});
	return hit ? { id: hit.deviceId, label: hit.label } : null;
}

export async function startPcmPlayback(opts: {
	silentSinkLabel: string;
}): Promise<{ playback: AudioPlayback; stop: () => Promise<void> }> {
	const ctx = new AudioContext({
		sampleRate: 48000,
		latencyHint: 'interactive'
	}) as AudioContextWithSink;
	if (ctx.state === 'suspended') await ctx.resume();

	const blob = new Blob([WORKLET], { type: 'application/javascript' });
	const moduleUrl = URL.createObjectURL(blob);
	try {
		await ctx.audioWorklet.addModule(moduleUrl);
	} finally {
		URL.revokeObjectURL(moduleUrl);
	}

	const node = new AudioWorkletNode(ctx, 'pcm-player', {
		numberOfInputs: 0,
		numberOfOutputs: 1,
		outputChannelCount: [2]
	});
	node.connect(ctx.destination);

	let sinkMode: AudioPlayback['sinkMode'] = 'default';
	let sinkLabel = '系统默认输出（可能有回授，请看提示）';
	try {
		const silent = await findSilentOutputId(opts.silentSinkLabel);
		if (silent && ctx.setSinkId) {
			await ctx.setSinkId(silent.id);
			sinkMode = 'silent';
			sinkLabel = silent.label || opts.silentSinkLabel;
		}
	} catch {
		// Keep default output; helper should already be on exclude-browser routing.
	}

	const socket = new WebSocket(audioSocketUrl());
	socket.binaryType = 'arraybuffer';
	socket.addEventListener('message', (event) => {
		if (event.data instanceof ArrayBuffer) {
			node.port.postMessage(event.data, [event.data]);
		}
	});

	await new Promise<void>((resolve, reject) => {
		socket.addEventListener('open', () => resolve(), { once: true });
		socket.addEventListener('error', () => reject(new Error('无法连接音频助手 WebSocket')), {
			once: true
		});
	});

	return {
		playback: { sinkMode, sinkLabel },
		stop: async () => {
			socket.close();
			node.disconnect();
			await ctx.close();
		}
	};
}
