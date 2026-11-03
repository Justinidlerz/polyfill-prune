use std::{env, fs, path::Path};

fn main() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(
            "Usage: sync-compat <bcd-package-dir> <typescript-package-dir> <webref-data.json> <output.json>".into(),
        );
    }
    let catalog = polyfill_prune_core::catalog::generate(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
    )?;
    fs::write(
        &args[3],
        serde_json::to_string_pretty(&catalog).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{} native APIs; BCD {}, TypeScript {}",
        catalog.features.len(),
        catalog.bcd_version,
        catalog.typescript_version
    );
    Ok(())
}
