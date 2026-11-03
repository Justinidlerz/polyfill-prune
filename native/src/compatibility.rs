use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type Targets = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Feature {
    pub kind: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub web: bool,
    pub support: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Catalog {
    pub bcd_version: String,
    pub typescript_version: String,
    #[serde(default)]
    pub webref_revision: String,
    pub features: BTreeMap<String, Feature>,
}

impl Catalog {
    pub fn bundled() -> Result<Self, String> {
        serde_json::from_str(include_str!("../data/compat.json")).map_err(|e| e.to_string())
    }

    pub fn supported<'a>(
        &'a self,
        name: &str,
        targets: &Targets,
        keep: &[String],
    ) -> Option<&'a Feature> {
        if keep
            .iter()
            .any(|key| name == key || name.starts_with(&format!("{key}.")))
        {
            return None;
        }
        let feature = self.features.get(name)?;
        targets
            .iter()
            .all(|(engine, version)| {
                feature.support.get(engine).is_some_and(|data| match data {
                    Value::Array(entries) => entries.iter().any(|entry| supports(entry, version)),
                    entry => supports(entry, version),
                })
            })
            .then_some(feature)
    }
}

fn supports(entry: &Value, target: &str) -> bool {
    if !entry.is_object()
        || ["version_removed", "prefix", "alternative_name"]
            .iter()
            .any(|key| entry.get(key).is_some())
        || entry.get("partial_implementation").and_then(Value::as_bool) == Some(true)
        || entry
            .get("flags")
            .and_then(Value::as_array)
            .is_some_and(|flags| !flags.is_empty())
    {
        return false;
    }
    let Some(added) = entry.get("version_added").and_then(Value::as_str) else {
        return false;
    };
    let Some(added) = version(added.trim_start_matches('≤')) else {
        return false;
    };
    version(target).is_some_and(|target| target >= added)
}

fn version(value: &str) -> Option<[u32; 4]> {
    let mut result = [0; 4];
    for (index, part) in value.split('.').enumerate() {
        if index >= result.len() || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        result[index] = part.parse().ok()?;
    }
    Some(result)
}

pub fn normalize_targets(values: &[String]) -> Result<Targets, String> {
    let mut targets = Targets::new();
    for value in values {
        if value == "esnext"
            || value
                .strip_prefix("es")
                .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        {
            continue;
        }
        let split = value
            .find(|c: char| c.is_ascii_digit())
            .ok_or_else(|| format!("Target requires an explicit runtime version: {value}"))?;
        let (name, minimum) = value.split_at(split);
        let engine = match name {
            "chrome" => "chrome",
            "edge" => "edge",
            "firefox" => "firefox",
            "safari" => "safari",
            "ios" => "safari_ios",
            "opera" => "opera",
            "node" => "nodejs",
            "deno" => "deno",
            _ => return Err(format!("Unsupported runtime target: {value}")),
        };
        let parsed = version(minimum).ok_or_else(|| format!("Invalid target version: {value}"))?;
        let previous = targets.get(engine).and_then(|v| version(v));
        if previous.is_none_or(|previous| parsed < previous) {
            targets.insert(engine.into(), minimum.into());
        }
    }
    if targets.is_empty() {
        return Err("Explicit browser/runtime targets are required; ES syntax targets do not determine native API support".into());
    }
    Ok(targets)
}
