import { audioSocketUrl } from './api';

const WORKLET = `
class PcmPlayerProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.channels = 2;
    this.target = Math.round(48000 * 0.05) * this.channels;
    this.maxFilled = Math.round(48000 * 0.12) * this.channels;
    this.capacity = this.maxFilled;
    this.buffer = new Float32Array(this.capacity);
    this.read = 0;
    this.write = 0;
    this.filled = 0;
    this.primed = false;
    this.rmsAcc = 0;
    this.rmsN = 0;
    this.port.onmessage = (event) => {
      if (!(event.data instanceof ArrayBuffer)) return;
      const src = new Int16Array(event.data);
      const count = src.length - (src.length % 2);
      for (let i = 0; i < count; i++) {
        if (this.filled >= this.maxFilled) {
          while (this.filled > this.target) {
            this.read = (this.read + 2) % this.capacity;
            this.filled -= 2;
          }
        }
        this.buffer[this.write] = src[i] / 32768;
        this.write = (this.write + 1) % this.capacity;
        this.filled++;
      }
      if (!this.primed && this.filled >= this.target) this.primed = true;
    };
  }

  process(_inputs, outputs) {
    const output = outputs[0];
    const left = output[0];
    const right = output[1] || output[0];
    for (let i = 0; i < left.length; i++) {
      if (this.primed && this.filled >= 2) {
        left[i] = this.buffer[this.read];
        this.read = (this.read + 1) % this.capacity;
        right[i] = this.buffer[this.read];
        this.read = (this.read + 1) % this.capacity;
        this.filled -= 2;
      } else {
        left[i] = 0;
        right[i] = 0;
      }
      this.rmsAcc += left[i] * left[i];
      this.rmsN++;
    }
    if (this.rmsN >= 4800) {
      this.port.postMessage({ type: 'level', rms: Math.sqrt(this.rmsAcc / this.rmsN) });
      this.rmsAcc = 0;
      this.rmsN = 0;
    }
    return true;
  }
}
registerProcessor('pcm-player', PcmPlayerProcessor);
`;

export type AudioPlayback = {
	mode: 'isolated' | 'exclude-browser' | 'apps';
	sinkLabel: string;
};

export async function startPcmPlayback(opts?: {
	onLevel?: (rms: number) => void;
}): Promise<{
	connectSocket: () => Promise<void>;
	fadeIn: () => void;
	stop: () => Promise<void>;
}> {
	const ctx = new AudioContext({
		sampleRate: 48000,
		latencyHint: 'interactive'
	});
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
	const gain = ctx.createGain();
	gain.gain.value = 0.0001;
	node.connect(gain);
	gain.connect(ctx.destination);

	node.port.onmessage = (event) => {
		if (event.data?.type === 'level') opts?.onLevel?.(event.data.rms);
	};

	let socket: WebSocket | null = null;

	return {
		connectSocket: () => {
			socket = new WebSocket(audioSocketUrl());
			socket.binaryType = 'arraybuffer';
			socket.addEventListener('message', (event) => {
				if (event.data instanceof ArrayBuffer) {
					node.port.postMessage(event.data, [event.data]);
				}
			});
			return new Promise<void>((resolve, reject) => {
				socket?.addEventListener('open', () => resolve(), { once: true });
				socket?.addEventListener('error', () => reject(new Error('无法连接音频助手 WebSocket')), {
					once: true
				});
			});
		},
		fadeIn: () => {
			const t = ctx.currentTime;
			gain.gain.cancelScheduledValues(t);
			gain.gain.setValueAtTime(gain.gain.value, t);
			gain.gain.linearRampToValueAtTime(1, t + 0.05);
		},
		stop: async () => {
			if (socket) socket.close();
			const t = ctx.currentTime;
			gain.gain.cancelScheduledValues(t);
			gain.gain.setValueAtTime(gain.gain.value, t);
			gain.gain.linearRampToValueAtTime(0, t + 0.03);
			await new Promise((resolve) => window.setTimeout(resolve, 40));
			node.disconnect();
			gain.disconnect();
			await ctx.close();
		}
	};
}
