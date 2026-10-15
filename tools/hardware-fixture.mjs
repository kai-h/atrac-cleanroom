// Generate the actual WAV inputs and expected raw frames together; keep both.
import {mkdir,writeFile,readFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import init,{WasmEncoder} from '../encoders/atrac-rust/atrac_cleanroom.js';
await init({module_or_path:await readFile(new URL('../encoders/atrac-rust/atrac_cleanroom_bg.wasm',import.meta.url))});
const root=new URL('../dist/hardware/',import.meta.url);
await mkdir(root,{recursive:true});
const lengths=[221525,221077,219999], total=lengths.reduce((a,b)=>a+b,0);
const hash=b=>createHash('sha256').update(b).digest('hex');
const manifest={sampleRate:44100,channels:2,samplesPerTrack:lengths,files:{},batches:[]};
async function save(name,bytes){await writeFile(new URL(name,root),bytes);manifest.files[name]={bytes:bytes.length,sha256:hash(bytes)};}
function wav(pcm){const b=Buffer.alloc(44+pcm.length*2);b.write('RIFF');b.writeUInt32LE(b.length-8,4);b.write('WAVEfmt ',8);b.writeUInt32LE(16,16);b.writeUInt16LE(1,20);b.writeUInt16LE(2,22);b.writeUInt32LE(44100,24);b.writeUInt32LE(176400,28);b.writeUInt16LE(4,32);b.writeUInt16LE(16,34);b.write('data',36);b.writeUInt32LE(pcm.length*2,40);for(let i=0;i<pcm.length;i++)b.writeInt16LE(pcm[i],44+2*i);return b;}
for(const signal of ['tone','sweep']){
 const pcm=new Int16Array(total*2);const duration=total/44100,rate=Math.log(18000/30)/duration;
 for(let i=0;i<total;i++){const t=i/44100;const phase=signal==='tone'?2*Math.PI*997*t:2*Math.PI*30*Math.expm1(rate*t)/rate;pcm[2*i]=pcm[2*i+1]=Math.round(12000*Math.sin(phase));}
 const tracks=[];let offset=0;for(let i=0;i<lengths.length;i++){const part=pcm.slice(offset*2,(offset+lengths[i])*2);offset+=lengths[i];tracks.push(part);await save(`${signal}-${i+1}.wav`,wav(part));}
 for(const kbps of [132,66])for(const gapless of [true,false]){
  const prefix=`${signal}-${kbps}-${gapless?'gapless':'control'}`;let encoder=new WasmEncoder(kbps,2);const parts=[];const frameCounts=[];
  for(let i=0;i<tracks.length;i++){let bytes=Buffer.from(encoder.write(tracks[i]));if(!gapless||i===tracks.length-1){bytes=Buffer.concat([bytes,Buffer.from(encoder.finish())]);encoder.free();if(i<tracks.length-1)encoder=new WasmEncoder(kbps,2);}
   await save(`${prefix}-${i+1}.frames`,bytes);parts.push(bytes);frameCounts.push(bytes.length/(kbps===132?384:192));}
  await save(`${prefix}-joined.frames`,Buffer.concat(parts));manifest.batches.push({signal,kbps,gapless,frameCounts});
 }
}
await writeFile(new URL('manifest.json',root),JSON.stringify(manifest,null,2)+'\n');
console.log(JSON.stringify(manifest.batches,null,2));
