import { loadCodec } from './api.js';
const output=document.querySelector('#results');output.textContent='';
const lines=[];
function report(s) {lines.push(s);output.textContent=lines.join('\n');}
function assert(v,s) {if (!v) throw new Error(s);}
function equal(a,b,label) {assert(a.length===b.length,`${label}: length ${a.length} != ${b.length}`);for(let i=0;i<a.length;i++) if(a[i]!==b[i])throw new Error(`${label}: mismatch at ${i}`);}
function join(parts) {const out=new Uint8Array(parts.reduce((n,p)=>n+p.length,0));let n=0;for(const p of parts){out.set(p,n);n+=p.length;}return out;}
function parseSAR(bytes) {
    const text=new TextDecoder();const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);let p=0;
    const read=n=>{if(n>bytes.length-p)throw new Error('Truncated SAR');const out=bytes.slice(p,p+n);p+=n;return out;};
    const size=n=>{const b=read(n);const d=new DataView(b.buffer);return n===2?d.getUint16(0):d.getUint32(0);};
    assert(text.decode(read(7))==='SARFile','SAR magic');assert(text.decode(read(size(2)))==='Web MiniDisc Dynamic Encoder','SAR submagic');
    const files=new Map();while(p<bytes.length){const name=text.decode(read(size(2)));assert(!files.has(name),'Duplicate SAR filename');files.set(name,read(size(4)));}
    assert(files.keys().next().value==='metadata.json','Metadata first');return files;
}
function ffmpegStub() {
    let files=new Map();let removing=false;let overlaps=0;return {
        assertSequential(){assert(overlaps===0,'Concurrent host filesystem requests');},
        async init(){files=new Map();},async deinit(){files.clear();},
        createFfmpegParams(params,format){assert(format==='s16le','PCM format');return 'stub s16le';},
        ffmpegProcess:{async write(n,b){files.set(n,b.slice());},async transcode(a,b){files.set(b,files.get(a).slice());},async read(n){return {data:files.get(n)};},async remove(n){if(removing)overlaps++;removing=true;try{await new Promise(resolve=>setTimeout(resolve,0));files.delete(n);}finally{removing=false;}}},
        async transcode(source){return source.slice();},getUserFriendyStageName(){return null;},
    };
}
const urls=[];const url=(bytes,type='text/javascript')=>{const u=URL.createObjectURL(new Blob([bytes],{type}));urls.push(u);return u;};
async function main() {
    const archive=new Uint8Array(await (await fetch('../encoders/atrac-rust/atrac-rust.wme')).arrayBuffer());const files=parseSAR(archive);
    const metadata=JSON.parse(new TextDecoder().decode(files.get('metadata.json')));assert(metadata.apiVersion==='1','API version');
    const Plugin=(await import(url(files.get('index.js')))).default;
    assert(Array.isArray(Plugin.customConstructionParameters),'Construction parameters');
    const codec=await loadCodec({moduleURL:url(files.get('atrac_cleanroom.js')),wasmURL:url(files.get('atrac_cleanroom_bg.wasm'),'application/wasm')});
    const pcm=new Int16Array(16391*2);let state=7;for(let i=0;i<pcm.length;i++){state=(Math.imul(state,1664525)+1013904223)>>>0;pcm[i]=(state>>>16)-32768;}
    const asBytes=p=>{const b=new Uint8Array(p.length*2);const d=new DataView(b.buffer);p.forEach((v,i)=>d.setInt16(2*i,v,true));return b;};
    const tracks=[pcm.slice(0,3073*2),pcm.slice(3073*2,9218*2),pcm.slice(9218*2)];
    const encode=(p,kbps)=>{const e=codec.createEncoder(kbps,2);try{return join([e.write(p),e.finish()]);}finally{e.free();}};
    for(const kbps of [132,105,66]) {
        const expected=encode(pcm,kbps);
        for(const samples of [1,1023,1024,1025,65536]) {
            const e=codec.createEncoder(kbps,2);const chunks=[];
            for(let i=0;i<pcm.length;i+=samples*2){assert(e.write(new Int16Array()).length===0,'Empty write');let threw=false;try{e.write(new Int16Array(1));}catch{threw=true;}assert(threw,'Odd element count rejected');chunks.push(e.write(pcm.subarray(i,i+samples*2)));}
            chunks.push(e.finish());e.free();equal(join(chunks),expected,`${kbps} split ${samples}`);
        }
        const random=codec.createEncoder(kbps,2);const chunks=[];let seed=123;let i=0;
        while(i<pcm.length){seed=(Math.imul(seed,1664525)+1013904223)>>>0;const n=(1+seed%4097)*2;chunks.push(random.write(pcm.subarray(i,i+n)));i+=n;}chunks.push(random.finish());random.free();equal(join(chunks),expected,`${kbps} random splits`);
        report(`PASS ${kbps} kbps: WASM split invariance and invalid-write atomicity`);
        const stub=ffmpegStub();const plugin=new Plugin({getOwnFile:n=>files.get(n).slice(),getBuiltinEncoder:()=>stub},{});
        const params=gapless=>({format:{codec:'AT3',bitrate:kbps},enableReplayGain:false,writeGapless:gapless});
        await plugin.init();const results=[];
        for(let t=0;t<3;t++){const b=await plugin.transcode(asBytes(tracks[t]),`track-${t}.pcm`,params(t<2));assert(b.byteOffset===0&&b.byteLength===b.buffer.byteLength,'Exact returned buffer');results.push(b);}
        stub.assertSequential();equal(join(results),expected,`${kbps} gapless album`);await plugin.deinit();
        await plugin.init();for(let t=0;t<3;t++){const b=await plugin.transcode(asBytes(tracks[t]),'track.pcm',params(false));equal(b,encode(tracks[t],kbps),`${kbps} standalone track ${t}`);}await plugin.deinit();
        await plugin.init();await plugin.transcode(asBytes(tracks[0]),'abandoned.pcm',params(true));await plugin.init();
        equal(await plugin.transcode(asBytes(pcm),'fresh.pcm',params(false)),expected,`${kbps} abandoned stream`);await plugin.deinit();
        report(`PASS ${kbps} kbps: blob-only plugin, gapless/non-gapless batches, exact buffers, abandoned-stream reset`);
    }
    // Verify all supplied golden files using the actual browser WASM build.
    const cases=await (await fetch('../spec/vectors/cases.json')).json();
    for(const c of cases.cases){const wav=new Uint8Array(await(await fetch('../spec/vectors/'+c.pcm)).arrayBuffer());const d=new DataView(wav.buffer);let p=12;let input;
        while(p+8<=wav.length){const n=d.getUint32(p+4,true);if(new TextDecoder().decode(wav.subarray(p,p+4))==='data'){input=new Int16Array(n/2);for(let i=0;i<input.length;i++)input[i]=d.getInt16(p+8+i*2,true);break;}p+=8+n+(n%2);}
        for(const [kbps,mode] of [[132,'lp2'],[66,'lp4']]){const golden=new Uint8Array(await(await fetch(`../spec/vectors/${c.name}.${mode}.frames`)).arrayBuffer());equal(encode(input,kbps),golden,`${c.name} ${mode}`);}
    }
    report('PASS all 24 LP2/LP4 golden raw-frame fixtures in browser WebAssembly');
    window.testResult={ok:true,lines};output.className='pass';
}
main().catch(error=>{report('FAIL '+error.stack);output.className='fail';window.testResult={ok:false,error:String(error),lines};}).finally(()=>{for(const u of urls)URL.revokeObjectURL(u);});
