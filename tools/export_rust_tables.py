"""Export frozen specification data as Rust constants; no reference-model imports."""
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]

def literal(v, kind):
    if kind == 'binary32': return f'f32::from_bits(0x{v})'
    if kind == 'binary64': return f'f64::from_bits(0x{v})'
    return str(v)

def typed(v):
    kind = v['type']
    typ = {'binary32':'f32','binary64':'f64','integer':'i32'}[kind]
    shape = v['shape']
    data = iter(v.get('bits', v.get('values')))
    def tree(d):
        if not d: return literal(next(data),kind)
        return '[' + ','.join(tree(d[1:]) for _ in range(d[0])) + ']'
    t=typ
    for n in reversed(shape): t=f'[{t}; {n}]'
    return t,tree(shape)

out=['// Generated from spec/tables/*.json by tools/export_rust_tables.py.\n#![allow(dead_code)]']
for mode in ('lp2','lp4','common'):
    out.append(f'pub mod {mode} {{')
    tables=json.loads((ROOT/'spec/tables'/f'{mode}.json').read_text())['tables']
    for name,v in tables.items():
        if isinstance(v,int): out.append(f'pub const {name}: i32 = {v};')
        elif v.get('type') in ('integer','binary32','binary64'):
            t,s=typed(v);out.append(f'pub const {name}: {t} = {s};')
        elif name in ('ENC_CODES','VLC'):
            for entry in v['entries']:
                a,b=entry['key'];t,s=typed(entry['value']);out.append(f'const CODE_{a}_{b}: {t} = {s};')
            out.append('pub fn codes(mode: usize, selector: usize) -> &\'static [[i32;2]] { match (mode,selector) {')
            for entry in v['entries']:
                a,b=entry['key'];out.append(f'({a},{b}) => &CODE_{a}_{b},')
            out.append('_ => &[], } }')
    out.append('}')
(ROOT/'src/tables.rs').write_text('\n'.join(out)+'\n')
