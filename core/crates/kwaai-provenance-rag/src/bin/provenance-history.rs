use kwaai_provenance_rag::history::{
    completeness_report, export, read_archive, ExperimentExpectation,
};
use std::path::Path;
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 4 && args[1] == "completeness" {
        let expected: ExperimentExpectation =
            serde_json::from_slice(&std::fs::read(&args[3]).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let report = completeness_report(Path::new(&args[2]), &expected)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return if report.eligible_for_factual_support_trajectory_analysis {
            Ok(())
        } else {
            Err("experiment is not eligible for factual-support trajectory analysis".into())
        };
    }
    if args.len() != 3 {
        return Err("usage: provenance-history validate|export ARCHIVE | completeness ARCHIVE EXPECTATION.json".into());
    }
    match args[1].as_str() {
        "validate" => {
            let captures = read_archive(Path::new(&args[2]))?;
            println!("{} validated captures", captures.len());
        }
        "export" => {
            for item in export(Path::new(&args[2]))? {
                println!(
                    "{}",
                    serde_json::to_string(&item).map_err(|e| e.to_string())?
                );
            }
        }
        _ => return Err("expected validate or export".into()),
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
