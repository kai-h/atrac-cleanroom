# ATRAC3 encoder v0.1

First public release of the MIT-licensed Rust ATRAC3 streaming encoder.

## Downloads

- **atrac-rust.wme** — install in Web MiniDisc Pro via Settings → Installed encoders → Manage → Install new, then select ATRAC3 Rust as the LP / HiMD encoder.
- **webassembly.zip** — standalone WebAssembly module, JavaScript facade and generated TypeScript declarations. Supports explicit blob URLs.
- **aarch64-apple-darwin.zip** / **x86_64-apple-darwin.zip** — native CLI for Apple silicon / Intel macOS.
- **x86_64-pc-windows-msvc.zip** — native CLI for Windows x64.
- **x86_64-unknown-linux-gnu.zip** / **aarch64-unknown-linux-gnu.zip** — native CLI for Linux x64 / ARM64 (glibc 2.35 or newer).
- **SHA256SUMS** — checksums for the downloadable assets.

Native usage: `atrac-encode 132 input.wav output.frames` (Windows: `atrac-encode.exe`).
Input is stereo 44.1 kHz signed 16-bit PCM WAV or raw `.s16le`. Supported bitrates
are 132 (LP2), 105, and 66 (LP4). Output is raw frames, not an AT3 container.
Rust applications can build the library from source; no stable C ABI is provided.
Native binaries are unsigned and macOS binaries are not notarized.

## Validation and limits

All supplied LP2/LP4 golden frames and streaming tests are required to pass on
each native release runner and in Node WebAssembly before packaging. The browser
WME harness and a real Web MiniDisc Pro three-track LP2 MockMD upload also passed.

Physical-device gapless playback, odd-frame LP4 padding and MZ-RH1 rip comparisons
remain deferred. The plugin preserves a continuous stream, but current host track
marks can land approximately 50–73 ms early. Independent 105 kbps golden vectors
are not available. This release does not include a decoder, ATRAC1/SP encoder,
ATRAC3plus encoder, or a completed security/fuzzing qualification.
