# Validation status

The implementation is an encoder candidate; physical gapless playback is pending.

Verified in this workspace on macOS ARM64:

- Native debug and release conformance: all 24 supplied LP2/LP4 golden payloads;
  analysis-stage checkpoints, streaming boundaries, invalid-input atomicity,
  all three modes and adversarial PCM patterns.
- Production WASM in Node: all 24 golden payloads and all three modes with fixed
  split sizes, empty writes and rejected incomplete stereo samples.
- Actual browser harness: WME parsing, metadata-first archive, blob module and
  WASM loading, module worker, three-track gapless and independent batches,
  exact output buffers, abandoned-stream reset, random/fixed split invariance,
  all supplied golden payloads.
- WME packaged successfully from the upstream-style encoder directory and
  installed through Web MiniDisc Pro's normal Settings flow. ATRAC3 Rust selected
  and MockMD connected. On 2026-10-15 the user completed the three-track LP2
  retry using the requested Gapless-on / ReplayGain-off settings. The resulting
  Mock Disc was inspected: tracks 6–8 are tone-1, tone-2, tone-3, all marked LP2,
  and the recording dialog has closed. This verifies the real host conversion
  and mock upload flow; it does not verify physical playback or rip payloads.
  Screenshot: ../dist/mockmd-success.png.

The first host attempt exposed a plugin cleanup deadlock: ffmpeg 0.6 keys pending
callbacks by action, so concurrent FS removals overwrite a callback. Cleanup is
now sequential. The browser regression harness checks for overlapping cleanup
requests; all three modes pass after the fix. The corrected WME was installed
before the successful host retry.

Native debug and release conformance passed on macOS, Windows and Linux in
GitHub Actions on 2026-10-15. Release builds additionally test the target-specific
CLI against golden LP2/LP4 payloads before packaging. Passing common fixtures is
evidence of reproducibility, not exhaustive cross-platform proof.

Public repository: https://github.com/kai-h/atrac-cleanroom
Release build workflow: .github/workflows/release.yml

Outstanding acceptance and specification limits:

- Physical LP2/LP4 gapless uploads, LP4 odd-frame padding and MZ-RH1 rip comparison.
- Independent 105 kbps golden outputs and decoded-PCM fixtures were not supplied.
  105 kbps is tested for streaming consistency, not Sony byte equivalence.
- Two explicitly unspecified LP4 branches return SpecificationGap instead of
  inventing behavior. No exercised PCM test has reached them.
- LP2 threshold rounding is deterministic and matches supplied vectors; universal
  x87 equivalence at half-level boundaries is unproven. See stream-timing.md.
- Native C ABI, decoder, fuzzing campaign and performance qualification are not
  implemented acceptance claims. Native Rust library and raw-frame CLI exist.
- Host behavior for non-final tracks so short they emit zero frames is unverified.

The upstream API audit revision is recorded in upstream-interface.md. The browser
harness uses a PCM ffmpeg stub and is not a substitute for the real host upload.
