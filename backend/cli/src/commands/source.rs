use crate::args::source::{SourceArgs, SourceCommands};
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::Result;

pub async fn handle_source(args: SourceArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        SourceCommands::List => {
            let sources = service.list_sources().await?;
            render::render_sources(&sources, format);
        }
        SourceCommands::Add(a) => {
            let res = service
                .add_source(a.path, a.name, a.kind, a.dry_run)
                .await?;
            render::render_json_value(&res);
        }
        SourceCommands::Remove(r) => {
            let res = service.remove_source(r.id, r.yes, r.dry_run).await?;
            render::render_json_value(&res);
        }
        SourceCommands::Scan(s) => {
            let res = service.scan_sources(s.kind, s.dry_run).await?;
            render::render_json_value(&res);
        }
    }
    Ok(())
}
