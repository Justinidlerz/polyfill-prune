//! Normalize curated Webref's parsed Web IDL without executing JavaScript.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize, Serialize)]
pub struct WebFeature {
    pub kind: String,
    pub bcd: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub struct WebCatalog {
    pub revision: String,
    pub features: BTreeMap<String, WebFeature>,
}

fn attribute<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    node["extAttrs"]
        .as_array()?
        .iter()
        .find(|attr| attr["name"] == name)
}

fn window_exposed(node: &Value) -> bool {
    let Some(exposed) = attribute(node, "Exposed") else {
        return false;
    };
    let value = &exposed["rhs"]["value"];
    exposed["rhs"]["type"] == "*"
        || value == "Window"
        || value == "*"
        || value
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value["value"] == "Window"))
}

fn unconditional(node: &Value) -> bool {
    [
        "SecureContext",
        "CrossOriginIsolated",
        "IsolatedContext",
        "LegacyNoInterfaceObject",
    ]
    .iter()
    .all(|name| attribute(node, name).is_none())
}

fn members<'a>(
    name: &str,
    definitions: &'a BTreeMap<String, Value>,
    extensions: &'a BTreeMap<String, Vec<Value>>,
    depth: usize,
    result: &mut Vec<&'a Value>,
) {
    if depth > 16 {
        return;
    }
    for node in definitions
        .get(name)
        .into_iter()
        .chain(extensions.get(name).into_iter().flatten())
    {
        if !unconditional(node) || (attribute(node, "Exposed").is_some() && !window_exposed(node)) {
            continue;
        }
        if node["type"] == "includes" {
            if let Some(mixin) = node["includes"].as_str() {
                members(mixin, definitions, extensions, depth + 1, result);
            }
        } else if let Some(list) = node["members"].as_array() {
            result.extend(list);
        }
    }
}

pub fn generate(directory: &Path, revision: String) -> Result<WebCatalog, String> {
    let mut definitions = BTreeMap::new();
    let mut extensions: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut paths = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    for path in paths {
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let data: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let parsed = &data["idlparsed"];
        if let Some(names) = parsed["idlNames"].as_object() {
            for (name, node) in names {
                if definitions.insert(name.clone(), node.clone()).is_some() {
                    return Err(format!("Duplicate Web IDL definition: {name}"));
                }
            }
        }
        if let Some(names) = parsed["idlExtendedNames"].as_object() {
            for (name, nodes) in names {
                if let Some(nodes) = nodes.as_array() {
                    extensions
                        .entry(name.clone())
                        .or_default()
                        .extend(nodes.iter().cloned());
                }
            }
        }
    }
    let mut catalog = WebCatalog {
        revision,
        features: BTreeMap::new(),
    };
    for (name, definition) in &definitions {
        if definition["type"] != "interface"
            || !window_exposed(definition)
            || !unconditional(definition)
        {
            continue;
        }
        let global = attribute(definition, "Global").is_some();
        catalog.features.insert(
            name.clone(),
            WebFeature {
                kind: "function".into(),
                bcd: vec![name.clone()],
            },
        );
        if !global {
            catalog.features.insert(
                format!("{name}.prototype"),
                WebFeature {
                    kind: "object".into(),
                    bcd: vec![name.clone()],
                },
            );
        }
        let mut list = Vec::new();
        members(name, &definitions, &extensions, 0, &mut list);
        for member in list {
            let Some(key) = member["name"].as_str().filter(|name| !name.is_empty()) else {
                continue;
            };
            if !unconditional(member)
                || (attribute(member, "Exposed").is_some() && !window_exposed(member))
            {
                continue;
            }
            if !global && attribute(member, "LegacyUnforgeable").is_some() {
                continue;
            }
            let kind = match member["type"].as_str() {
                Some("operation") if member["special"] == "" || member["special"] == "static" => {
                    "function"
                }
                Some("attribute" | "const") => "presence",
                _ => continue,
            };
            let path = if global {
                key.to_string()
            } else if member["special"] == "static" {
                format!("{name}.{key}")
            } else {
                format!("{name}.prototype.{key}")
            };
            let mut bcd = vec![format!("{name}.{key}")];
            if global {
                bcd.insert(0, key.to_string());
            }
            catalog.features.insert(
                path,
                WebFeature {
                    kind: kind.into(),
                    bcd,
                },
            );
        }
    }
    if catalog.features.is_empty() {
        return Err("No Window APIs found in Webref".into());
    }
    Ok(catalog)
}
