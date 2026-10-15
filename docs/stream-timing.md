# Streaming and track timing

All counts are samples per channel. Stereo writes contain twice as many i16
values. Invalid element counts fail before state changes. A zero-length write
returns no bytes. Finish is terminal; subsequent writes and finish calls error.
Dropping/freeing an encoder discards its buffered data without implicit flushing.

The stream uses the specification's non-looping 955-sample lead-in, installed
once at construction. Each full 1024-sample block advances the codec; the first
two analysis outputs are discarded. No operation depends on write boundaries.

For S accepted input samples, before finish:

    F(S) = max(0, floor((S + 955) / 1024) - 2)

This is the cumulative number of complete, final frames returned. The internal
partial PCM block holds at most 1023 samples; DSP histories have fixed sizes.
Output memory is proportional to the output requested by the current write.

At finish, let T = S + 955 and r = T mod 1024. The cumulative final count is:

    ceil(T / 1024) + (1 if r == 0 or r >= 886, otherwise 0)

Finish pads a partial block and makes 2 empty analysis calls when 0 < r < 886,
otherwise 3. Empty streams are defined to produce one silent frame; short input
is accepted by this API even though Sony's file tool required 6144 samples.
The raw frame API has no RIFF headers or fact chunk. The native CLI also writes
raw frames; its output must not be mistaken for a complete .at3 container.

## Delay and track marks

For this framing, decoded sample i corresponds to input sample i - D, with
D = 1093 samples (24.7846 ms). This is the documented spec delay, not a new
hardware measurement. The host can trim D samples and then retain S samples
when extracting standalone audio with a compatible decoder.

For a track boundary after S input samples, a mark after F(S) frames occurs at
source coordinate 1024*F(S) - D. Thus the early-mark displacement is:

    S - 1024*F(S) + D

After startup, writing r = (S + 955) mod 1024 gives 2186 + r samples:
49.5692 to 72.7664 ms. This combines buffered audio and the decoded delay;
adding transform overlap again would double-count part of it. At startup use
the general formula, since F can be zero.

These are encoder stream coordinates. MiniDisc track marks, sound-group padding,
player state and rip framing still require hardware verification. LP4 odd-frame
padding is unresolved. No frame is held back speculatively. frameBytes is 384,
304 or 192 for 132, 105 or 66 kbps respectively.

A very short non-final track may yield zero frames. The core deliberately returns
an empty buffer; whether the host/device can represent that track is not established.
There is no artificial padding at track boundaries to hide this limitation.

## Numerical policy

Both algorithms use frozen table bits and explicit f32/f64 operations. No fast
math, reassociation, fused multiply-add, signal-path threads, or host transcendental
functions are used by the encoder. Fractional power-of-two gain factors are frozen
binary32 values with exact exponent adjustment.

LP2 gain-level rounding uses frozen sqrt(2) thresholds and power-of-two scaling
instead of platform log10 calls. This ensures reproducibility of the decision;
it matches the supplied fixtures. As the spec itself cautions, ratios extremely
close to half-level boundaries have not established universal x87/Sony equivalence.

Cross-platform equivalence is tested using the same fixtures. Passing on macOS
and browser WASM does not constitute a completed Windows/Linux execution test.
