pub mod catalog;
pub mod compatibility;
mod model;
mod output;
pub mod transform;
pub mod web_catalog;

use napi::{Env, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use serde_json::{Value, json};
use std::sync::OnceLock;

fn catalog() -> Result<&'static compatibility::Catalog, String> {
    static CATALOG: OnceLock<Result<compatibility::Catalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(compatibility::Catalog::bundled)
        .as_ref()
        .map_err(Clone::clone)
}

pub struct PruneTask {
    code: String,
    options: String,
}
impl Task for PruneTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> napi::Result<String> {
        let run = || -> Result<String, String> {
            let options: model::Options =
                serde_json::from_str(&self.options).map_err(|e| e.to_string())?;
            let result = transform::prune(&self.code, &options, catalog()?)?;
            serde_json::to_string(&result).map_err(|e| e.to_string())
        };
        run().map_err(napi::Error::from_reason)
    }
    fn resolve(&mut self, _: Env, output: String) -> napi::Result<String> {
        Ok(output)
    }
}

#[napi(
    js_name = "pruneRaw",
    ts_args_type = "code: string, options: PruneOptions",
    ts_return_type = "Promise<string>"
)]
pub fn prune_raw(code: String, options: Value) -> AsyncTask<PruneTask> {
    AsyncTask::new(PruneTask {
        code,
        options: options.to_string(),
    })
}

#[napi(
    ts_return_type = "{ core: string; parser: string; bcd: string; typescript: string; webref: string; features: number }"
)]
pub fn version() -> napi::Result<Value> {
    let catalog = catalog().map_err(napi::Error::from_reason)?;
    Ok(
        json!({ "core": env!("CARGO_PKG_VERSION"), "parser": "0.154.0", "bcd": catalog.bcd_version, "typescript": catalog.typescript_version, "webref": catalog.webref_revision, "features": catalog.features.len() }),
    )
}
