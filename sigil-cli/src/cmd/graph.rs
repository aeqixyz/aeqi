use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::cli::GraphAction;
use crate::helpers::load_config;

pub(crate) async fn cmd_graph(config_path: &Option<PathBuf>, action: GraphAction) -> Result<()> {
    match action {
        GraphAction::Index { project, full } => cmd_graph_index(config_path, &project, full),
        GraphAction::Stats { project } => cmd_graph_stats(config_path, &project),
    }
}

fn cmd_graph_index(config_path: &Option<PathBuf>, project: &str, full: bool) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let data_dir = config.data_dir();

    let repo_path = config
        .projects
        .iter()
        .find(|p| p.name == project)
        .map(|p| {
            let r = p
                .repo
                .replace('~', &dirs::home_dir().unwrap_or_default().to_string_lossy());
            PathBuf::from(r)
        })
        .with_context(|| format!("project '{project}' not found in config"))?;

    let graph_dir = data_dir.join("codegraph");
    std::fs::create_dir_all(&graph_dir).ok();
    let db_path = graph_dir.join(format!("{project}.db"));

    let store = sigil_graph::GraphStore::open(&db_path)
        .with_context(|| format!("failed to open graph DB at {}", db_path.display()))?;
    let indexer = sigil_graph::Indexer::new();

    let result = if full {
        eprintln!("Full indexing {project} at {} ...", repo_path.display());
        indexer.index(&repo_path, &store)?
    } else {
        eprintln!(
            "Incremental indexing {project} at {} ...",
            repo_path.display()
        );
        indexer.index_incremental(&repo_path, &store)?
    };

    eprintln!(
        "  files: {}, nodes: {}, edges: {}, communities: {}, processes: {}",
        result.files_parsed, result.nodes, result.edges, result.communities, result.processes,
    );
    if result.parse_errors > 0 {
        eprintln!("  parse errors: {}", result.parse_errors);
    }
    if result.unresolved > 0 {
        eprintln!("  unresolved symbols: {}", result.unresolved);
    }

    Ok(())
}

fn cmd_graph_stats(config_path: &Option<PathBuf>, project: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let data_dir = config.data_dir();

    let db_path = data_dir.join("codegraph").join(format!("{project}.db"));
    if !db_path.exists() {
        eprintln!("No graph DB for project '{project}'. Run `sigil graph index -r {project}` first.");
        return Ok(());
    }

    let store = sigil_graph::GraphStore::open(&db_path)?;
    let stats = store.stats()?;
    let indexed_at = store.get_meta("indexed_at")?.unwrap_or_default();
    let last_commit = store.get_meta("last_commit")?.unwrap_or_default();

    println!("Project: {project}");
    println!("  Nodes:       {}", stats.node_count);
    println!("  Edges:       {}", stats.edge_count);
    println!("  Files:       {}", stats.file_count);
    println!("  Indexed at:  {}", if indexed_at.is_empty() { "never" } else { &indexed_at });
    println!("  Last commit: {}", if last_commit.is_empty() { "unknown" } else { &last_commit });

    Ok(())
}
