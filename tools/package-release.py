"""Package tested release outputs; Python is a packaging tool, not a runtime dependency."""
import argparse
from pathlib import Path
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('kind', choices=('native', 'wasm'))
p.add_argument('target', nargs='?')
a = p.parse_args()
out = ROOT / 'dist/release'
out.mkdir(parents=True, exist_ok=True)
common = ['LICENSE', 'README.md', 'docs/stream-timing.md', 'docs/validation.md']
if a.kind == 'native':
    if not a.target:
        p.error('native packaging requires a target triple')
    name = 'atrac-encode.exe' if 'windows' in a.target else 'atrac-encode'
    binary = ROOT / 'target' / a.target / 'release' / name
    # Exercise the distributable itself, including file I/O, not just library tests.
    for bitrate, mode in [(132, 'lp2'), (66, 'lp4')]:
        actual = out / f'smoke-{mode}.frames'
        subprocess.run([str(binary), str(bitrate), str(ROOT / 'spec/vectors/tone.wav'), str(actual)], check=True)
        if actual.read_bytes() != (ROOT / f'spec/vectors/tone.{mode}.frames').read_bytes():
            raise SystemExit(f'{mode} CLI smoke test failed')
        actual.unlink()
    with zipfile.ZipFile(out / f'atrac-cleanroom-v0.1-{a.target}.zip', 'w', zipfile.ZIP_DEFLATED) as z:
        z.write(binary, name)
        for f in common:
            z.write(ROOT / f, f)
else:
    plugin = ROOT / 'encoders/atrac-rust'
    shutil.copyfile(plugin / 'atrac-rust.wme', out / 'atrac-rust.wme')
    with zipfile.ZipFile(out / 'atrac-cleanroom-v0.1-webassembly.zip', 'w', zipfile.ZIP_DEFLATED) as z:
        for f in ['api.js', 'atrac_cleanroom.js', 'atrac_cleanroom.d.ts', 'atrac_cleanroom_bg.wasm', 'atrac_cleanroom_bg.wasm.d.ts']:
            z.write(plugin / f, f)
        z.writestr('package.json', '{"type":"module","private":true}\n')
        z.writestr('USAGE.md', '''# WebAssembly bundle
Import loadCodec from ./api.js, then pass explicit moduleURL and wasmURL for
./atrac_cleanroom.js and ./atrac_cleanroom_bg.wasm. Both may be blob URLs.
Call createEncoder(132, 2), write(Int16Array), finish(), and free().
Supported bitrates: 132, 105, 66. Input: stereo 44100 Hz signed 16-bit PCM.
Every sample has two interleaved elements. Outputs are raw ATRAC3 frames.
''')
        for f in common:
            z.write(ROOT / f, f)
