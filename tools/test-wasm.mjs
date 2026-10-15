// Run the production WebAssembly bytes in Node, independently of browser UI.
import { readFile,readdir } from 'node:fs/promises';
import init,{WasmEncoder} from '../encoders/atrac-rust/atrac_cleanroom.js';
await init({module_or_path:await readFile(new URL('../encoders/atrac-rust/atrac_cleanroom_bg.wasm',import.meta.url))});
const concat=parts=>{const b=new Uint8Array(parts.reduce((n,p)=>n+p.length,0));let p=0;for(const part of parts){b.set(part,p);p+=part.length;}return b;};
function equal(a,b,label){if(a.length!==b.length||a.some((v,i)=>v!==b[i]))throw new Error(label+' differs');}
function pcm(wav){let p=12;while(p+8<=wav.length){const n=wav.readUInt32LE(p+4);if(wav.toString('ascii',p,p+4)==='data'){const x=new Int16Array(n/2);for(let i=0;i<x.length;i++)x[i]=wav.readInt16LE(p+8+2*i);return x;}p+=8+n+n%2;}throw new Error('WAV data missing');}
const root=new URL('../',import.meta.url);let count=0;
for(const name of await readdir(new URL('spec/vectors/',root))){if(!name.endsWith('.wav'))continue;const input=pcm(await readFile(new URL('spec/vectors/'+name,root)));
 for(const [kbps,mode] of [[132,'lp2'],[66,'lp4']]){const e=new WasmEncoder(kbps,2);const out=concat([e.write(input),e.finish()]);e.free();equal(out,await readFile(new URL('spec/vectors/'+name.replace('.wav',`.${mode}.frames`),root)),name+' '+mode);count++;}
 for(const kbps of [132,105,66]){const e=new WasmEncoder(kbps,2);const expected=concat([e.write(input),e.finish()]);e.free();
  for(const n of [1,1023,1024,1025,65536]){const e=new WasmEncoder(kbps,2);const parts=[];for(let i=0;i<input.length;i+=2*n){parts.push(e.write(new Int16Array()));let rejected=false;try{e.write(new Int16Array(1));}catch{rejected=true;}if(!rejected)throw new Error('odd input accepted');parts.push(e.write(input.subarray(i,i+2*n)));}parts.push(e.finish());e.free();equal(concat(parts),expected,`${name} ${kbps} split ${n}`);}
 }
}
if(count!==24)throw new Error('Expected exactly 24 golden cases');
console.log('PASS: 24 WASM golden fixtures; all modes and fixed split sizes on every fixture.');
