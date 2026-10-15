// Blob-safe public facade. The caller supplies both resources explicitly.
export async function loadCodec({ moduleURL, wasmURL }) {
    if (!moduleURL || !wasmURL) throw new TypeError('moduleURL and wasmURL are required');
    const module = await import(moduleURL);
    await module.default({ module_or_path: wasmURL });
    return {
        createEncoder(kbps, channels) {
            if (!Number.isInteger(kbps) || !Number.isInteger(channels)) throw new TypeError('Integer mode parameters required');
            const encoder = new module.WasmEncoder(kbps, channels);
            let freed = false;
            const alive = () => { if (freed) throw new Error('Encoder has been freed'); };
            return {
                frameBytes: encoder.frameBytes,
                write(pcm) {
                    alive();
                    if (!(pcm instanceof Int16Array)) throw new TypeError('Expected Int16Array');
                    if (pcm.length % channels) throw new RangeError('Incomplete interleaved sample');
                    return encoder.write(pcm).slice();
                },
                finish() { alive(); return encoder.finish().slice(); },
                free() { if (!freed) { encoder.free(); freed = true; } },
            };
        },
    };
}
