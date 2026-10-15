# Web MiniDisc Pro interface audit

Inspected upstream master on 2026-10-15, revision
`9637bed5a472f470273912414a182b101e85f3a6`.

Read external-interface.ts, dynamic-encoders.ts, external-archive.ts,
convertAndUpload in src/redux/actions.ts, ffmpeg-audio-export.ts, and
encoders/atracdenc package.json and src/index.ts. No upstream source was copied.

Confirmed contracts:
- Default ES-module class, required static customConstructionParameters.
- getOwnFile returns archive bytes; index is imported using a blob URL.
- SARFile magic, big-endian field lengths and API version string "1".
- The upload path consumes returned Uint8Array.buffer directly.
- Conversion is sequential; upload of earlier results may overlap conversion.
- init precedes batch, deinit follows; cancellation must not be trusted to clean up.
- writeGapless is set for non-final tracks when gapless is enabled.
- ffmpeg createFfmpegParams(..., 's16le') requests stereo 44.1 kHz and ReplayGain.

The new adapter must preserve these behaviours but use an original module worker,
streaming encoder, explicit failure handling and independently authored declarations.
