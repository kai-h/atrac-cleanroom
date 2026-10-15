"""Compare raw ATRAC3 payloads; deliberately does not guess container headers."""
import argparse
from pathlib import Path
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('expected', type=Path)
p.add_argument('actual', type=Path)
p.add_argument('--frame-bytes', type=int, choices=(192,304,384), required=True)
a = p.parse_args()
x, y = a.expected.read_bytes(), a.actual.read_bytes()
if len(x) % a.frame_bytes or len(y) % a.frame_bytes:
    p.error('Both inputs must contain only complete raw frames; remove container headers first.')
for i in range(max(len(x), len(y)) // a.frame_bytes):
    start = i * a.frame_bytes
    if x[start:start+a.frame_bytes] != y[start:start+a.frame_bytes]:
        raise SystemExit(f'FAIL: frame {i} differs (zero-based); total frames {len(x)//a.frame_bytes} vs {len(y)//a.frame_bytes}')
print(f'PASS: {len(x)//a.frame_bytes} identical frames')
