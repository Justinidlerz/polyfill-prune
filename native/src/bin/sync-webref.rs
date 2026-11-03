use std::{env, fs, path::Path, process::Command};

fn main() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("Usage: sync-webref <webref-checkout> <output.json>".into());
    }
    let checkout = Path::new(&args[0]);
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(checkout)
        .output()
        .map_err(|e| e.to_string())?;
    if !head.status.success() {
        return Err("Cannot read Webref source revision".into());
    }
    let revision = String::from_utf8(head.stdout)
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    let status = Command::new("git")
        .args(["status", "--porcelain", "--", "ed/idlparsed"])
        .current_dir(checkout)
        .output()
        .map_err(|e| e.to_string())?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("Webref parsed data must match the recorded revision".into());
    }
    let catalog =
        polyfill_prune_core::web_catalog::generate(&checkout.join("ed/idlparsed"), revision)?;
    fs::write(
        &args[1],
        serde_json::to_string_pretty(&catalog).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{} Web IDL API locations; Webref {}",
        catalog.features.len(),
        catalog.revision
    );
    Ok(())
}
