# Device acceptance handoff

Generate inputs and expected payloads with `node tools/hardware-fixture.mjs`.
The files are written to `dist/hardware/`; retain that exact directory, including
manifest.json, for reproducible comparisons. The manifest hashes each artifact.
Signals are stereo 44.1 kHz 16-bit PCM: a continuous 997 Hz tone and a continuous
logarithmic 30 Hz–18 kHz sweep, each split into three approximately five-second
tracks. Cuts are not on 1024-sample boundaries. Generation uses host math only
for test stimuli; retain the actual WAVs rather than relying on regeneration.

For each signal, upload tracks 1, 2, 3 in order in four separate batches:
LP2 gapless, LP4 gapless, LP2 gapless off, and LP4 gapless off. Disable ReplayGain.
Use ATRAC3 Rust as the selected encoder. Gapless batches return 215, 216, 218
frames; controls return 218, 217, 216. Both exercise odd and even frame counts.
Expected raw frames are named by signal, bitrate, gapless/control and track.

Listen across both boundaries, then rip each track with an MZ-RH1. Preserve the
original rip containers and identify their format before extracting raw ATRAC3
payloads. Do not simply strip a guessed number of header bytes. Compare each
extracted payload against its matching `.frames` file:

```sh
python3 tools/compare-frames.py expected.frames ripped.frames --frame-bytes 192
```

Use 384 for LP2 and 192 for LP4. The comparator reports the first different frame
and rejects partial frames. Do not trim mismatches to make the test pass. Record
any inserted sound-group padding, particularly on the odd LP4 track, plus device
model, firmware if known, recording mode, track order, and audible discontinuities.

The joined expected payload is also supplied. Track marks may be early according
to docs/stream-timing.md even when the continuous audio is gapless. MockMD cannot
establish physical playback or RH1 rip behavior. Those checks require hardware.
