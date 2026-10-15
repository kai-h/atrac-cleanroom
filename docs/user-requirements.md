## Gapless encoding: required from the first commit

The encoder will upload albums to MiniDisc through Web MiniDisc Pro with its Gapless option, where consecutive tracks must join with no gap, click or repeated audio. Design for this from the start; don't add it later.

### Streaming encoder (core API)
- The core API is a streaming encoder: create(mode), write(pcm) -> frames, finish() -> frames, destroy(). write() accepts interleaved 16-bit PCM of any length per call, including 0 and 1 samples, and returns only complete, final ATRAC3 frames. Samples that don't yet fill a frame, plus any look-ahead the encoder needs, stay inside the encoder until the next call.
- Only finish() ends a stream, and it is the only place that pads with silence and flushes the remaining delay. write() must never pad, flush, prime or reset anything. Any lead-in (priming) happens once, at the start of the stream.
- All state lives in the encoder object, with no globals or statics. Nothing may reset at, or depend on, where one call ends and the next begins: not rate control, transient detection, stereo decisions, gain history or filter memories.
- Invariant, tested from day one: for every mode and every way of splitting the input, write(p1) + write(p2) + ... + finish() is byte-identical to encoding the whole input in one call. Test with random splits, and with pieces of 0, 1, 1023, 1024 and 1025 samples and pieces larger than any internal buffer.
- Output must be deterministic across platforms and builds, so a test tool can re-encode the source and reproduce exactly what was sent to a device. In C: compile with -ffp-contract=off, no fast-math, and no threading in the signal path.
- Document the encoder's total delay D (decoded sample i of the stream corresponds to input sample i - D), and the rule for how many frames have been returned after S input samples. Hosts need both to place track marks and to check rips.
- Keep look-ahead as small as quality allows. In Web MiniDisc Pro each track mark lands early by the audio the encoder still holds: the partial frame, plus look-ahead, plus transform overlap. This is unavoidable, because a track's frames must be final before the next track is seen. Report the resulting range in milliseconds.
- Expose the frame size in bytes, so a host can split output by frame and hold a frame back to the next track if hardware testing shows that is needed. Open question: an LP4 sound group on disc holds two frames, and a track with an odd number of frames may be padded.
- Frame sizes, at 1024 samples per channel per frame and 44.1 kHz: LP2 (132 kbps stereo) 384 bytes, 105 kbps stereo 304 bytes, LP4 (66 kbps joint stereo) 192 bytes.

### WebAssembly module
- Expose the streaming encoder in the JavaScript API, not only a whole-file function: createEncoder(kbps, channels) returning { frameBytes, write(Int16Array) -> Uint8Array, finish() -> Uint8Array, free() }.
- Every returned Uint8Array must own a buffer holding exactly its bytes, never a view into a larger buffer, because hosts read `.buffer`.
- The module must load when every file is a blob: URL. Blob URLs can't import each other by relative path, and can't resolve a sibling .wasm file. Either ship one self-contained ES module, or let callers pass options to the Emscripten factory and load the .wasm with `locateFile: () => wasmBlobURL` (a Blob of type application/wasm). Emscripten 6 doesn't read `Module.wasmBinary` unless it is added to -sINCOMING_MODULE_JS_API.

### Web MiniDisc Pro plugin (encoder API version 1)
Source: github.com/asivery/webminidisc. Read these on current master; they are authoritative over this summary:
- src/services/audio/apiv1/external-interface.ts (the interface);
- src/services/audio/apiv1/dynamic-encoders.ts (how plugins load);
- src/redux/actions.ts, convertAndUpload (how transcode is called);
- src/services/audio/ffmpeg/ffmpeg-audio-export.ts (the built-in ffmpeg);
- encoders/atracdenc/ (a minimal example plugin).

Upstream is GPL-2.0, so write your own code and type declarations rather than copying its files.

**Packaging.** A plugin is a .wme file, Web MiniDisc Pro's SAR archive:
- ASCII "SARFile", then the sub-magic "Web MiniDisc Dynamic Encoder", then each file's name and contents;
- every field is preceded by its big-endian length: u16 for the sub-magic and file names, u32 for contents;
- metadata.json must be the first file. Its fields are encoderId, userFriendlyName, description, version, encoderDependencies (an empty array) and apiVersion "1".

**Loading.**
- index.js is an ES module, imported from a blob: URL. Its default export is a class constructed as `new Encoder(api, customParameters)`.
- The class needs a static `customConstructionParameters` array. It is required, and may be empty.
- Other files are read with `api.getOwnFile(name)`, which returns a Uint8Array. Everything you load is therefore a blob: URL.

**Methods.** init(), deinit(), transcode(source, sourceFileName, exportParams, progressCallback?) -> Promise<Uint8Array>, getUserFriendyStageName(stage) (the misspelling is in the API), and getSupportFor(codec) -> { state, gapless, bitrates? }.

**How the host calls it.**
- init() is called once at the start of an upload batch, then transcode() once per track, strictly in track order and one at a time, then deinit() at the end.
- exportParams is { format: { codec, bitrate }, enableReplayGain, writeGapless }. writeGapless is true when the user ticked Gapless and the track is not the last in the batch.

**Required behaviour.**
- While writeGapless is true, write the track to the open encoder and return what it gives, leaving the stream open. When writeGapless is false, write the track, call finish(), and close the stream. With Gapless off, every track is therefore a complete stream of its own.
- A track's returned bytes must be final, because the host may upload track N while it converts track N+1. Return raw ATRAC3 frames with no header, in a Uint8Array whose buffer holds exactly those frames.
- Treat init() as the start of a new batch and discard any stream left open. A failed or cancelled batch may never call deinit().
- Get PCM through the built-in ffmpeg: `ffmpeg = api.getBuiltinEncoder('ffmpeg')`, then `cmd = ffmpeg.createFfmpegParams(exportParams, 's16le')`, which yields 16-bit little-endian stereo at 44.1 kHz (plus ReplayGain if the user enabled it). Then use ffmpeg.ffmpegProcess.write / transcode / read / remove.
- Pass the PCM and MP3 codecs to ffmpeg.transcode. Reject 'A3+'.
- Run the encoder in a module worker (`new Worker(url, { type: 'module' })`) that owns the encoder object across transcode calls, so long tracks don't block the page. Transfer buffers rather than copying them, and handle the worker's error event.
- getSupportFor('AT3') returns { state: 'perfect', gapless: true, bitrates: [132, 105, 66] }. The Gapless checkbox in the upload dialog is disabled unless gapless is true.

**Host limits (not yours to fix).** Pre-encoded ATRAC files and library files bypass transcode, so they break a gapless chain. ReplayGain is applied per track. Sources that aren't 44.1 kHz are resampled track by track.

**Upstream layout.** Also make the plugin buildable inside the upstream repo as encoders/<id>/, following encoders/atracdenc: a package.json whose "package" script writes <id>.wme, where the name must match the directory name. The hosted Web MiniDisc Pro only serves encoders built from its own repo.

### Acceptance tests
1. Split invariance (above), for every mode, in both native and WebAssembly builds.
2. A browser harness that loads the .wme exactly as Web MiniDisc Pro does: parse the SAR archive, import index.js from a blob: URL, and provide getOwnFile plus a stub ffmpeg that passes PCM through. It must show:
   - a three-track gapless batch whose joined output equals a one-shot encode of the joined audio;
   - a non-gapless batch equal to per-track encodes;
   - returned buffers that are exactly sized;
   - init() discarding an abandoned stream.
3. An end-to-end run in a Web MiniDisc Pro dev build (`npm run dev`), which offers a mock device ("Connect to MockMD"):
   - install the .wme via Settings > Installed encoders > Manage > Install new;
   - select it as the "LP / HiMD encoder to use";
   - upload three tracks with Gapless ticked.
4. Hardware test:
   - signals: a continuous tone and a log sweep, each cut into tracks that don't fall on 1024-sample frame boundaries, with a mix of odd and even frame counts per track;
   - uploads: LP2 and LP4 with Gapless on, plus a Gapless-off control;
   - checks: listen across each track change, then rip with an MZ-RH1 and compare frame by frame with the re-encoded frames that were sent.