use crate::{Encoder, Mode};
use wasm_bindgen::prelude::*;
fn error(e: crate::Error) -> JsValue {
    JsValue::from_str(&e.to_string())
}
#[wasm_bindgen]
pub struct WasmEncoder {
    inner: Encoder,
}
#[wasm_bindgen]
impl WasmEncoder {
    #[wasm_bindgen(constructor)]
    pub fn new(kbps: u32, channels: usize) -> Result<WasmEncoder, JsValue> {
        Ok(Self {
            inner: Encoder::new(Mode::from_bitrate(kbps, channels).map_err(error)?)
                .map_err(error)?,
        })
    }
    #[wasm_bindgen(getter,js_name=frameBytes)]
    pub fn frame_bytes(&self) -> usize {
        self.inner.frame_bytes()
    }
    pub fn write(&mut self, pcm: &[i16]) -> Result<Vec<u8>, JsValue> {
        self.inner.write(pcm).map_err(error)
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, JsValue> {
        self.inner.finish().map_err(error)
    }
}
