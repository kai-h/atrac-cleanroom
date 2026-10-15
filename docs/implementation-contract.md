# Implementation contract

The user requirements in user-requirements.md apply, with these subsequent
clarifications taking precedence:

- Rust; MIT license for the new implementation.
- Target priority: WebAssembly, macOS, Windows, Linux.
- Input is 44.1 kHz, signed 16-bit interleaved PCM.
- A sample means one point in time, per channel. Stereo one-sample writes have
  TWO elements. All S, D, block lengths and split sizes use samples per channel.
- Reject writes whose element count is not divisible by the channel count,
  before changing state. Never retain part of an interleaved sample.
- Empty writes are valid; only finish pads and drains the stream.
- All mutable codec state belongs to its encoder. Immutable tables may be shared.
- Preserve the supplied specification and Python implementation.
- Flag contradictory or ambiguous requirements with the user immediately.

## Verification boundaries

The existing fixtures cover LP2/LP4 stereo, not 105 kbps or decoded PCM.
Two LP4 branches explicitly raise SpecificationGap in Python. Do not invent
Sony-compatible behaviour for these branches or claim exhaustive compatibility.
Hardware gaplessness and odd LP4 frame padding require device tests and RH1 rips.
Do not advertise completed acceptance tests that have not actually run.

## Source boundary

Implement from spec Markdown, exported JSON tables and cleanroom Python.
Do not use spec/reference source. Inspect Web MiniDisc Pro solely for the API
contract; write original adapter code and declarations, without copying GPL files.
