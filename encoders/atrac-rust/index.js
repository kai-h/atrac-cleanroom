// Original implementation of the Web MiniDisc Pro encoder API v1.
class WorkerClient {
    constructor(url) {
        this.worker = new Worker(url, { type: 'module' });
        this.pending = new Map(); this.sequence = 0; this.dead = false;
        this.worker.onmessage = ({ data }) => {
            const request = this.pending.get(data.id);
            if (!request) return;
            this.pending.delete(data.id);
            if (data.error) request.reject(new Error(data.error));
            else request.resolve(data.result);
        };
        this.worker.onerror = event => this.close(new Error(event.message || 'Codec worker failed'));
        this.worker.onmessageerror = () => this.close(new Error('Invalid codec worker message'));
    }
    call(action, fields = {}, transfer = []) {
        if (this.dead) return Promise.reject(new Error('Codec worker is closed'));
        const id = ++this.sequence;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            try { this.worker.postMessage({ id, action, ...fields }, transfer); }
            catch (error) { this.pending.delete(id); reject(error); }
        });
    }
    close(error = new Error('Encoder batch ended')) {
        this.dead = true; this.worker.terminate();
        for (const request of this.pending.values()) request.reject(error);
        this.pending.clear();
    }
}
export default class AtracRustEncoder {
    static customConstructionParameters = [];
    constructor(api, customParameters = {}) {
        this.api = api; this.urls = []; this.client = undefined;
        this.ffmpeg = undefined; this.initialized = false; this.busy = false; this.generation = 0;
    }
    resource(name, type) {
        const url = URL.createObjectURL(new Blob([this.api.getOwnFile(name)], { type }));
        this.urls.push(url); return url;
    }
    async init() {
        await this.deinit();
        try {
            this.ffmpeg = this.api.getBuiltinEncoder('ffmpeg');
            await this.ffmpeg.init();
            const workerURL = this.resource('worker.js', 'text/javascript');
            const apiURL = this.resource('api.js', 'text/javascript');
            const moduleURL = this.resource('atrac_cleanroom.js', 'text/javascript');
            const wasmURL = this.resource('atrac_cleanroom_bg.wasm', 'application/wasm');
            this.client = new WorkerClient(workerURL);
            await this.client.call('init', { apiURL, moduleURL, wasmURL });
            this.initialized = true;
        } catch (error) { await this.deinit(); throw error; }
    }
    async deinit() {
        this.generation++; this.busy = false;
        this.initialized = false;
        this.client?.close(); this.client = undefined;
        for (const url of this.urls) URL.revokeObjectURL(url);
        this.urls = [];
        const ffmpeg = this.ffmpeg; this.ffmpeg = undefined;
        if (ffmpeg) await ffmpeg.deinit();
    }
    async transcode(source, sourceFileName, exportParams, progressCallback) {
        if (!this.initialized) throw new Error('Call init() before transcode()');
        if (this.busy) throw new Error('Concurrent tracks are not supported');
        const { codec, bitrate } = exportParams.format;
        if (codec === 'A3+') throw new Error('ATRAC3plus is not supported');
        if (!['AT3', 'PCM', 'MP3'].includes(codec)) throw new Error('Unsupported codec');
        if (codec === 'AT3' && ![132, 105, 66].includes(bitrate)) throw new Error('Unsupported ATRAC3 bitrate');
        this.busy = true;
        const generation = this.generation;
        const current = () => { if (generation !== this.generation) throw new Error("Encoder batch was replaced"); };
        const ffmpeg = this.ffmpeg; const client = this.client;
        const suffix = String(sourceFileName).split('.').pop().replace(/[^a-zA-Z0-9]/g, '').slice(0, 16) || 'input';
        const inputName = `atrac-rust-${generation}-input.${suffix}`;
        const outputName = `atrac-rust-${generation}-output.s16le`;
        try {
            if (codec === 'PCM' || codec === 'MP3') {
                await client.call('discard');
                const result = await ffmpeg.transcode(source, sourceFileName, exportParams, progressCallback);
                current(); return result.slice();
            }
            progressCallback?.({ stage: 'pcm', progress: 0, total: 2 });
            const process = ffmpeg.ffmpegProcess;
            try {
                await process.write(inputName, source);
                await process.transcode(inputName, outputName, ffmpeg.createFfmpegParams(exportParams, 's16le'));
                const { data } = await process.read(outputName);
                current();
                const pcm = data.byteOffset === 0 && data.byteLength === data.buffer.byteLength ? data : data.slice();
                progressCallback?.({ stage: 'atrac', progress: 1, total: 2 });
                const result = await client.call('encode', { buffer: pcm.buffer, kbps: bitrate, gapless: !!exportParams.writeGapless }, [pcm.buffer]);
                progressCallback?.({ stage: 'atrac', progress: 2, total: 2 });
                current(); return new Uint8Array(result);
            } finally {
                // Host ffmpeg 0.6 stores pending callbacks by action (both are FS).
                // Concurrent removes overwrite a callback and leave a promise pending.
                for (const name of [inputName, outputName]) {
                    try { await process.remove(name); } catch { /* File may not exist after failure. */ }
                }
            }
        } catch (error) {
            client.close(error); if (generation === this.generation) this.initialized = false;
            throw error;
        } finally { if (generation === this.generation) this.busy = false; }
    }
    getUserFriendyStageName(stage) {
        return ({ pcm: 'Preparing PCM', atrac: 'Encoding ATRAC3' })[stage] ?? this.ffmpeg?.getUserFriendyStageName(stage) ?? null;
    }
    getSupportFor(codec) {
        if (codec === 'AT3') return { state: 'perfect', gapless: true, bitrates: [132, 105, 66] };
        if (codec === 'PCM' || codec === 'MP3') return { state: 'perfect', gapless: true };
        return { state: 'unsupported', gapless: false };
    }
}
