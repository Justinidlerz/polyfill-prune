use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}
fn filename() -> String {
    "input.js".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub targets: Vec<String>,
    #[serde(default = "filename")]
    pub filename: String,
    #[serde(default)]
    pub keep: Vec<String>,
    #[serde(default = "yes")]
    pub source_map: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Removal {
    /// UTF-8 byte offsets in the input.
    pub start: u32,
    pub end: u32,
    pub kind: &'static str,
    pub features: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneResult {
    pub code: String,
    pub map: Option<serde_json::Value>,
    pub changed: bool,
    pub removals: Vec<Removal>,
    pub diagnostics: Vec<String>,
}

impl PruneResult {
    pub fn unchanged(code: &str, diagnostics: Vec<String>) -> Self {
        Self {
            code: code.into(),
            map: None,
            changed: false,
            removals: vec![],
            diagnostics,
        }
    }
}
