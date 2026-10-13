use agentbroom_core::{preview, scan_fixture_store, web_catalog};
use clap::{Parser, Subcommand};
#[derive(Parser)]
#[command(
    name = "agentbroom",
    about = "Inspect the fixed synthetic AgentBroom fixture. No real-store or mutation commands."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Scan,
    Plan { ids: Vec<String> },
    Verify,
    WebCatalog,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
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
        Command::WebCatalog => serde_json::to_string_pretty(&web_catalog(inventory)?)?,
    };
    println!("{json}");
    Ok(())
}
