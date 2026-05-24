use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/passkeys.js")]
unsafe extern "C" {
    #[wasm_bindgen(catch)]
    pub async fn registerPasskey(challenge_json: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn loginPasskey(challenge_json: &str) -> Result<JsValue, JsValue>;
}
