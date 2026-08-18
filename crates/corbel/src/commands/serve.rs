use std::io;
use std::path::Path;

use anyhow::Context;
use corbel_core::path::RepoRoot;
use corbel_core::store::migrate::open_connection;

use crate::mcp::server::McpServer;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let root = RepoRoot::new(path)
        .with_context(|| format!("failed to open repository root at {}", path.display()))?;

    let db_path = root.as_path().join(".corbel").join("index.db");
    if !db_path.exists() {
        anyhow::bail!(
            "no index found at {}. Run `corbel index {}` first.",
            db_path.display(),
            path.display()
        );
    }

    let conn = open_connection(&db_path)
        .with_context(|| format!("failed to open index database at {}", db_path.display()))?;

    let server = McpServer::new(conn);

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();

    server.run(&mut input, &mut output)
}
