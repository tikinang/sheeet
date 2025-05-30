use js_sys::Array;
use wasm_bindgen::prelude::*;
use web_sys::console::log_2;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(catch, js_namespace = window)]
    fn evaluate(fn_name: &str, vars: &Array) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen]
pub fn run_evaluate() {
    let js_array = Array::new();
    js_array.push(&JsValue::from_f64(5.5));
    js_array.push(&JsValue::from_f64(6.5));

    match evaluate("add", &js_array) {
        Ok(result) => {
            log_2(&"evaluate ok!".into(), &result);

            let js_array = Array::new();
            js_array.push(&result);
            js_array.push(&JsValue::from_f64(3.0));

            match evaluate("sub", &js_array) {
                Ok(result) => {
                    log_2(&"evaluate ok!".into(), &result);
                }
                Err(js_error) => {
                    log_2(&"evaluate failed :(".into(), &js_error);
                }
            }
        }
        Err(js_error) => {
            log_2(&"evaluate failed :(".into(), &js_error);
        }
    };
}

fn main() {
    console_error_panic_hook::set_once();
    log("log from wasm main");
}
