use agentbroom_core::{preview, scan_fixture_store, web_catalog};
use clap::{Parser, Subcommand};
#[derive(Parser)]
#[command(
    name = "agentbroom",
    about = "Inspect local Codex metadata read-only. No mutation commands."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Full real metadata for local inspection. Do not publish this output.
    Scan,
    /// Aggregate-only real inventory; never prints titles, IDs or paths.
    CodexSummary,
    /// Local read-only cleanup analysis. Output may contain private metadata.
    CleanupScope { ids: Vec<String> },
    /// Development-only synthetic fixture commands; never used by the app.
    #[command(hide = true)]
    Fixtures {
        #[command(subcommand)]
        command: FixtureCommand,
    },
}
#[derive(Subcommand)]
enum FixtureCommand {
    Scan,
    Plan { ids: Vec<String> },
    Verify,
    WebCatalog,
    CleanupCatalog,
    NativeScope,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if matches!(args.command, Command::Scan) {
        println!(
            "{}",
            serde_json::to_string(&agentbroom_core::codex::scan_default()?)?
        );
        return Ok(());
    }
    if let Command::CleanupScope { ref ids } = args.command {
        let inventory = agentbroom_core::codex::scan_default()?;
        let analysis = agentbroom_core::cleanup::plan(&inventory, ids, &[])?;
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "inventoryPartial": inventory.partial_coverage,
                "analysis": analysis
            }))?
        );
        return Ok(());
    }
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
    let Command::Fixtures { command } = args.command else {
        unreachable!()
    };
    if matches!(command, FixtureCommand::NativeScope) {
        use std::io::Read;
        let mut input = Vec::new();
        std::io::stdin().take(65537).read_to_end(&mut input)?;
        if input.len() > 65536 {
            return Err("Native evidence exceeds budget".into());
        }
        let evidence: agentbroom_core::native_scope::NativeScopeEvidence =
            serde_json::from_slice(&input)?;
        println!(
            "{}",
            serde_json::to_string(&agentbroom_core::native_scope::assess(&evidence))?
        );
        return Ok(());
    }
    if matches!(command, FixtureCommand::CleanupCatalog) {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/cleanup-cases.json"))?;
        let mut result = Vec::new();
        for c in cases.as_array().ok_or("Invalid test cases")? {
            let inventory: agentbroom_core::Inventory =
                serde_json::from_value(c["inventory"].clone())?;
            let selected: Vec<String> = serde_json::from_value(c["selected"].clone())?;
            let kept: Vec<String> = serde_json::from_value(c["kept"].clone())?;
            result.push(serde_json::json!({"name":c["name"],"expected":agentbroom_core::cleanup::plan(&inventory,&selected,&kept)?}));
        }
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo-store");
    let inventory = scan_fixture_store(&root)?;
    let json = match command {
        FixtureCommand::Verify => serde_json::to_string_pretty(&serde_json::json!({
            "sourceKind": "demo", "executionEnabled": false,
            "consistent": inventory.items.iter().map(|x| x.logical_bytes).sum::<u64>() == inventory.totals.iter().map(|x| x.logical_bytes).sum::<u64>(),
            "sourceFingerprint": inventory.source_fingerprint,
            "note": "Fixture measurement consistency only; not deletion verification."
        }))?,
        FixtureCommand::Scan => serde_json::to_string_pretty(&inventory)?,
        FixtureCommand::Plan { ids } => serde_json::to_string_pretty(&preview(
            &inventory,
            &ids,
            &[],
            &inventory.source_fingerprint,
        )?)?,
        FixtureCommand::CleanupCatalog | FixtureCommand::NativeScope => unreachable!(),
        FixtureCommand::WebCatalog => serde_json::to_string_pretty(&web_catalog(inventory)?)?,
    };
    println!("{json}");
    Ok(())
}
