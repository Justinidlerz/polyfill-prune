//! Combine TypeScript standard libraries and normalized Webref API locations
//! with MDN browser compatibility statements. No JS is executed.
use crate::compatibility::{Catalog, Feature};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::scope::ScopeFlags;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Default)]
struct Interface {
    callable: bool,
    methods: BTreeSet<String>,
    prototype: Option<String>,
}
#[derive(Default)]
struct Declarations {
    interfaces: BTreeMap<String, Interface>,
    globals: BTreeMap<String, String>,
    functions: BTreeSet<String>,
}

fn type_name(ty: &TSType<'_>) -> Option<String> {
    // TypeScript's T[] syntax denotes the Array interface, including the
    // ArrayConstructor.prototype declaration in lib.es5.d.ts.
    if matches!(ty, TSType::TSArrayType(_)) {
        return Some("Array".into());
    }
    if let TSType::TSTypeReference(reference) = ty
        && let TSTypeName::IdentifierReference(identifier) = &reference.type_name
    {
        return Some(identifier.name.to_string());
    }
    None
}

impl<'a> Visit<'a> for Declarations {
    // Namespaced declarations are not global bindings. Leave them out until
    // their complete namespace/type relationships can be represented.
    fn visit_ts_namespace_declaration(&mut self, _: &TSNamespaceDeclaration<'a>) {}
    fn visit_ts_external_module_declaration(&mut self, _: &TSExternalModuleDeclaration<'a>) {}
    fn visit_ts_interface_declaration(&mut self, node: &TSInterfaceDeclaration<'a>) {
        let entry = self.interfaces.entry(node.id.name.to_string()).or_default();
        for member in &node.body.body {
            match member {
                TSSignature::TSMethodSignature(method)
                    if !method.computed
                        && !method.optional
                        && method.kind == TSMethodSignatureKind::Method =>
                {
                    if let Some(name) = method.key.static_name() {
                        entry.methods.insert(name.to_string());
                    }
                }
                TSSignature::TSPropertySignature(property)
                    if !property.computed && !property.optional =>
                {
                    if let Some(annotation) = &property.type_annotation
                        && property
                            .key
                            .static_name()
                            .is_some_and(|name| name == "prototype")
                    {
                        entry.prototype = type_name(&annotation.type_annotation);
                    }
                }
                TSSignature::TSCallSignatureDeclaration(_)
                | TSSignature::TSConstructSignatureDeclaration(_) => entry.callable = true,
                _ => {}
            }
        }
    }
    fn visit_variable_declarator(&mut self, node: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(identifier) = &node.id
            && let Some(annotation) = &node.type_annotation
            && let Some(name) = type_name(&annotation.type_annotation)
        {
            self.globals.insert(identifier.name.to_string(), name);
        }
    }
    fn visit_function(&mut self, node: &Function<'a>, _: ScopeFlags) {
        if node.body.is_none()
            && let Some(identifier) = &node.id
        {
            self.functions.insert(identifier.name.to_string());
        }
    }
}

pub fn generate(bcd_dir: &Path, ts_dir: &Path, webref_file: &Path) -> Result<Catalog, String> {
    let read_json = |path: &Path| -> Result<Value, String> {
        serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    };
    let data = read_json(&bcd_dir.join("data.json"))?;
    let bcd_package = read_json(&bcd_dir.join("package.json"))?;
    let ts_package = read_json(&ts_dir.join("package.json"))?;
    let mut declarations = Declarations::default();
    let mut paths = fs::read_dir(ts_dir.join("lib"))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy().starts_with("lib.es")
                    && name.to_string_lossy().ends_with(".d.ts")
            })
        })
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            &source,
            SourceType::from_path(&path).map_err(|e| e.to_string())?,
        )
        .parse();
        if !parsed.diagnostics.is_empty() {
            return Err(format!(
                "Cannot parse {}: {:?}",
                path.display(),
                parsed.diagnostics
            ));
        }
        declarations.visit_program(&parsed.program);
    }
    let mut catalog = Catalog {
        bcd_version: bcd_package["version"]
            .as_str()
            .ok_or("Missing BCD version")?
            .into(),
        typescript_version: ts_package["version"]
            .as_str()
            .ok_or("Missing TypeScript version")?
            .into(),
        webref_revision: String::new(),
        features: BTreeMap::new(),
    };
    let mut add = |name: String, kind: &str| {
        let mut feature = &data["javascript"]["builtins"];
        for part in name.split('.').filter(|part| *part != "prototype") {
            feature = &feature[part];
        }
        if let Some(support) = feature["__compat"]["support"].as_object() {
            catalog.features.insert(
                name,
                Feature {
                    kind: kind.into(),
                    web: false,
                    support: support
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                },
            );
        }
    };
    for function in &declarations.functions {
        add(function.clone(), "function");
    }
    for (global, type_name) in &declarations.globals {
        let Some(interface) = declarations.interfaces.get(type_name) else {
            continue;
        };
        add(
            global.clone(),
            if interface.callable {
                "function"
            } else {
                "object"
            },
        );
        for method in &interface.methods {
            add(format!("{global}.{method}"), "function");
        }
        if let Some(prototype) = &interface.prototype
            && let Some(instance) = declarations.interfaces.get(prototype)
        {
            for method in &instance.methods {
                add(format!("{global}.prototype.{method}"), "function");
            }
        }
    }
    // globalThis is a language-provided global, absent from TS library variables.
    add("globalThis".into(), "object");
    let web: crate::web_catalog::WebCatalog =
        serde_json::from_value(read_json(webref_file)?).map_err(|e| e.to_string())?;
    catalog.webref_revision = web.revision;
    for (path, feature) in web.features {
        for bcd in feature.bcd {
            let mut entry = &data["api"];
            for part in bcd.split('.') {
                entry = &entry[part];
            }
            if let Some(support) = entry["__compat"]["support"].as_object() {
                catalog.features.entry(path).or_insert_with(|| Feature {
                    kind: feature.kind,
                    web: true,
                    support: support
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                });
                break;
            }
        }
    }
    if catalog.features.is_empty() {
        return Err("No native APIs found in the supplied datasets".into());
    }
    Ok(catalog)
}
