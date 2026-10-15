import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const names = ['metadata.json', 'index.js', 'worker.js', 'api.js', 'atrac_cleanroom.js', 'atrac_cleanroom_bg.wasm'];
const u16 = n => { if (n > 65535) throw new Error('Field too long'); const b=Buffer.alloc(2); b.writeUInt16BE(n); return b; };
const u32 = n => { const b=Buffer.alloc(4); b.writeUInt32BE(n); return b; };
const magic = Buffer.from('Web MiniDisc Dynamic Encoder');
const parts = [Buffer.from('SARFile'), u16(magic.length), magic];
for (const name of names) {
    const filename=Buffer.from(name); const contents=await readFile(new URL(name, import.meta.url));
    parts.push(u16(filename.length), filename, u32(contents.length), contents);
}
const output = new URL('atrac-rust.wme', import.meta.url);
await writeFile(output, Buffer.concat(parts));
console.log(fileURLToPath(output));
