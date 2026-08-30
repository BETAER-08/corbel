use std::io;
use std::path::Path;

use anyhow::Context;
use corbel_core::error::{Error, SchemaCondition};
use corbel_core::path::RepoRoot;
use corbel_core::store::migrate::open_for_serve;

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

    let conn = match open_for_serve(&db_path) {
        Ok(conn) => conn,
        Err(Error::IncompatibleSchema {
            expected,
            condition: SchemaCondition::VersionMismatch(found),
        }) => {
            anyhow::bail!(
                "index at {} has schema version {found}, but this binary expects version {expected}. Run `corbel index {}` to rebuild it.",
                db_path.display(),
                path.display()
            );
        }
        Err(Error::IncompatibleSchema {
            condition: SchemaCondition::Unreadable,
            ..
        }) => {
            anyhow::bail!(
                "the file at {} could not be read as a corbel index. It may be corrupted, or {} may not be the path you meant to serve. Run `corbel index {}` to rebuild it, or double-check the path.",
                db_path.display(),
                db_path.display(),
                path.display()
            );
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!("failed to open index database at {}", db_path.display())
            });
        }
    };

    let server = McpServer::new(conn);

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();

    server.run(&mut input, &mut output)
}
