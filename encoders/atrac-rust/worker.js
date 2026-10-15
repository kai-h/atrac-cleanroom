// One worker owns one codec stream across consecutive album tracks.
let codec;
let stream;
let bitrate;
function discard() { stream?.free(); stream = undefined; bitrate = undefined; }
self.onmessage = async ({ data }) => {
    const { id, action } = data;
    try {
        let result;
        if (action === 'init') {
            discard();
            const { loadCodec } = await import(data.apiURL);
            codec = await loadCodec({ moduleURL: data.moduleURL, wasmURL: data.wasmURL });
        } else if (action === 'encode') {
            if (!codec) throw new Error('Worker is not initialized');
            if (stream && bitrate !== data.kbps) throw new Error('Cannot change bitrate within a gapless stream');
            if (data.buffer.byteLength % 4) throw new Error('PCM must contain complete stereo samples');
            if (!stream) { stream = codec.createEncoder(data.kbps, 2); bitrate = data.kbps; }
            const view = new DataView(data.buffer);
            const pcm = new Int16Array(view.byteLength / 2);
            for (let i = 0; i < pcm.length; i++) pcm[i] = view.getInt16(i * 2, true);
            const chunks = [];
            let size = 0;
            // Bound individual calls while preserving state and complete samples.
            for (let offset = 0; offset < pcm.length; offset += 32768) {
                const bytes = stream.write(pcm.subarray(offset, offset + 32768));
                chunks.push(bytes); size += bytes.length;
            }
            if (!data.gapless) { const tail = stream.finish(); chunks.push(tail); size += tail.length; discard(); }
            result = new Uint8Array(size);
            let position = 0;
            for (const chunk of chunks) { result.set(chunk, position); position += chunk.length; }
        } else if (action === 'discard') {
            discard();
        } else { throw new Error('Unknown worker request'); }
        if (result) self.postMessage({ id, result: result.buffer }, [result.buffer]);
        else self.postMessage({ id });
    } catch (error) {
        discard();
        self.postMessage({ id, error: String(error?.message ?? error) });
    }
};
