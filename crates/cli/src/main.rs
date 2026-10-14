use agentbroom_core::{preview, scan_fixture_store, web_catalog};
use clap::{Parser, Subcommand};
#[derive(Parser)]
#[command(
    name = "agentbroom",
    about = "Inspect synthetic fixtures or local Codex metadata read-only. No mutation commands."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Scan,
    /// Aggregate-only real inventory; never prints titles, IDs or paths.
    CodexSummary,
    Plan {
        ids: Vec<String>,
    },
    Verify,
    WebCatalog,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if matches!(args.command, Command::CodexSummary) {
        let inventory = agentbroom_core::codex::scan_default()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "sourceKind":"codex", "executionEnabled":false, "partialCoverage":inventory.partial_coverage, "totals":inventory.totals,
                "recordCount":inventory.threads.len(), "relations": (["root", "spawned", "fork", "unknown"].into_iter().map(|k| (k,inventory.threads.iter().filter(|t|serde_json::to_value(&t.relation).ok().and_then(|v|v.as_str().map(String::from)).as_deref()==Some(k)).count())).collect::<std::collections::BTreeMap<_,_>>()),
                "protectedCount":inventory.threads.iter().filter(|t|!t.protection_reasons.is_empty()).count(), "warnings":inventory.warnings
            }))?
        );
        return Ok(());
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo-store");
    let inventory = scan_fixture_store(&root)?;
    let json = match args.command {
        Command::Verify => serde_json::to_string_pretty(&serde_json::json!({
            "sourceKind": "demo", "executionEnabled": false,
            "consistent": inventory.items.iter().map(|x| x.logical_bytes).sum::<u64>() == inventory.totals.iter().map(|x| x.logical_bytes).sum::<u64>(),
            "sourceFingerprint": inventory.source_fingerprint,
            "note": "Fixture measurement consistency only; not deletion verification."
        }))?,
        Command::Scan => serde_json::to_string_pretty(&inventory)?,
        Command::Plan { ids } => serde_json::to_string_pretty(&preview(
            &inventory,
            &ids,
            &[],
            &inventory.source_fingerprint,
        )?)?,
        Command::CodexSummary => unreachable!(),
        Command::WebCatalog => serde_json::to_string_pretty(&web_catalog(inventory)?)?,
    };
    println!("{json}");
    Ok(())
}
