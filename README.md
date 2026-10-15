# ATRAC3 streaming encoder

## Download v0.1

**[Download the Web MiniDisc Pro plugin (.wme)](https://github.com/kai-h/atrac-cleanroom/releases/download/v0.1/atrac-rust.wme)**

[All release downloads: WebAssembly, macOS, Windows and Linux](https://github.com/kai-h/atrac-cleanroom/releases/tag/v0.1)

Install the `.wme` through **Settings → Installed encoders → Manage → Install new**,
then select **ATRAC3 Rust** as the LP / HiMD encoder. Physical-device gapless
verification remains pending; see the validation notes below.

MIT-licensed Rust implementation derived from the supplied specification and
Python cleanroom implementation. WebAssembly is the primary target. The current
deliverable is an **encoder**, not a decoder or a complete ATRAC-family codec.

Supports 44.1 kHz signed 16-bit **stereo** PCM: LP2 132 kbps (384 bytes/frame),
105 kbps stereo (304), and LP4 66 kbps (192). A frame represents 1024 samples per
channel. A sample is one time point: stereo has two interleaved elements.

## Current status

Native macOS and WebAssembly tests pass all 24 supplied LP2/LP4 golden payloads.
The browser harness passes streaming and plugin tests for all three bitrates.
A three-track LP2 upload through Web MiniDisc Pro to MockMD completed successfully
on 2026-10-15, after fixing sequential cleanup of the host's ffmpeg files.

Physical-device testing is deferred: gapless playback, LP4 odd-frame padding and
MZ-RH1 rip comparisons remain unverified. Windows/Linux CI is configured but has
not yet been run here. Independent 105 kbps golden vectors are still needed.
See [validation](docs/validation.md) for the full limits and outstanding checks.

## Build and test

Use Rust with edition 2024 support. Native builds have no third-party runtime
dependencies. WebAssembly uses the pinned wasm-bindgen version in Cargo.toml.
The WASM packaging and Node tests also require Node.js; the build script uses a
POSIX shell (use an appropriate shell environment on Windows).

Python and the `cleanroom/` directory are **not required to build or run** the
Rust encoder, WASM module or WME plugin. They are retained as a development
reference. Regenerating kernel test fixtures with `tools/export_test_fixtures.py`
requires Python, NumPy and the cleanroom modules; those generated fixtures are
already included. Rust table generation reads `spec/tables/*.json`.

```sh
cargo test --locked
cargo test --locked --release
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
tools/build-wasm.sh
node tools/test-wasm.mjs
```

On this workspace, `tools/cargo-local.sh` selects the locally installed toolchain.
`tools/build-wasm.sh` also selects it when present. It creates the WASM bindings
and `encoders/atrac-rust/atrac-rust.wme`. The plugin directory can be copied into
Web MiniDisc Pro's `encoders/`; `npm run package` there packages its built assets.

Native CLI (output is raw frames, **not** an AT3 container):

```sh
cargo run --release --bin atrac-encode -- 132 input.wav output.frames
```

Raw `.s16le` input is also accepted. The Rust library exposes `Encoder::new(Mode)`,
`write(&[i16])`, `finish()` and `frame_bytes()`. Drop discards unfinished state.
There is no stable C ABI in this version.

## JavaScript

```js
import { loadCodec } from './web/api.js';
const { createEncoder } = await loadCodec({ moduleURL, wasmURL });
const encoder = createEncoder(132, 2);
const first = encoder.write(interleavedInt16Array);
const last = encoder.finish();
encoder.free();
```

Supply explicit URLs for the generated module and WASM; both may be blob URLs.
Each result owns an exactly sized buffer. Odd element counts throw before state
changes; empty writes are valid. Only finish pads and drains the stream. Keep one
encoder alive across all tracks in a gapless batch. The plugin handles this in a
persistent module worker and obtains PCM through the host's built-in ffmpeg.

Serve the repository locally and open `web/harness.html` for browser tests of the
actual WME archive, blob imports and worker. The harness stubs PCM conversion;
Web MiniDisc Pro integration is a separate test.

## Gapless tracks and marker placement

With Gapless enabled, the host passes `writeGapless: true` for each track except
the last. The plugin preserves one encoder across those calls and finishes it
only on the final track. With Gapless disabled, each track is finished separately.
Intermediate boundaries introduce no encoder padding or discarded samples.

The host requires each track's encoded output before supplying the next track.
Audio retained for partial frames and look-ahead therefore appears in the next
track's output. Under the current timing model, track markers land approximately
49.6–72.8 ms early after startup; this is not a measured device result. Accurate
frame assignment around the intended join would require host read-ahead across
tracks, and placement would still depend on device sound-group boundaries.
See [stream timing](docs/stream-timing.md) for formulas and short-track behavior.

Install `encoders/atrac-rust/atrac-rust.wme` through Web MiniDisc Pro's
Settings → Installed encoders → Manage → Install new, then select **ATRAC3 Rust**
as the LP / HiMD encoder. The [hardware test instructions](docs/hardware-testing.md)
describe the generated tone/sweep inputs and expected payload comparisons.

See [validation](docs/validation.md), [stream timing](docs/stream-timing.md),
[hardware testing](docs/hardware-testing.md), and the
[implementation contract](docs/implementation-contract.md). Existing `spec/` and
`cleanroom/` sources are preserved. No source from `spec/reference` was used.

The supplied `spec/reference/` directory is excluded from the public repository;
it was not used for this implementation. Model-regeneration commands mentioned
in the original specification require those separately supplied reference files.
Frozen-table and vector verification, Rust builds and tests do not require them.
