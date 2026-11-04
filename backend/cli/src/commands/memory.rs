#[path = "memory_commit.rs"]
mod memory_commit;

use crate::args::memory::{
    MemoryArgs, MemoryCommand, MemoryProjectCommand, MemoryRecentCommand, MemoryStateCommand,
    MemoryStatusArgs, MemoryWatermarkCommand,
};
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::Result;
use asset_core::domain::NodeStatus;

pub async fn handle_memory(args: MemoryArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        MemoryCommand::Recent(r) => match r.command {
            MemoryRecentCommand::Get(g) => {
                let res = service.memory().get_recent_snapshot(g.date).await?;
                render::render_memory_markdown_view(&res, format);
            }
        },
        MemoryCommand::Project(p) => match p.command {
            MemoryProjectCommand::Get(g) => {
                let project_path = if g.current_project {
                    std::env::current_dir()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                } else {
                    g.project_path
                };
                let project_path = project_path.ok_or_else(|| {
                    anyhow::anyhow!(
                        "project path is required (provide <project_path> or use --current-project)"
                    )
                })?;
                let res = service.memory().get_project(project_path).await?;
                render::render_memory_markdown_view(&res, format);
            }
        },
        MemoryCommand::State(s) => match s.command {
            MemoryStateCommand::Get(g) => {
                let project = if g.current_project {
                    std::env::current_dir()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                } else {
                    g.project
                };
                let res = service.memory().get_state(project).await?;
                if g.raw {
                    println!("{}", serde_json::to_string_pretty(&res)?);
                } else {
                    render::render_json_value(&res);
                }
            }
        },
        MemoryCommand::Status(st) => {
            handle_status(st, &service, format).await?;
        }
        MemoryCommand::Commit(c) => {
            memory_commit::handle_commit(c, &service, format).await?;
        }
        MemoryCommand::Watermark(w) => match w.command {
            MemoryWatermarkCommand::Check(chk) => {
                let res = service
                    .memory()
                    .check_watermark(&chk.project, &chk.session_id)
                    .await?;
                render::render_json_value(&res);
            }
            MemoryWatermarkCommand::Advance(adv) => {
                let res = service
                    .memory()
                    .advance_watermark(&adv.project, &adv.session_id, adv.turn)
                    .await?;
                render::render_json_value(&res);
            }
        },
    }
    Ok(())
}

async fn handle_status(
    st: MemoryStatusArgs,
    service: &CliService,
    format: OutputFormat,
) -> Result<()> {
    let status: NodeStatus = st.status.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
    let project_slug = st.project.as_deref().map(|p| {
        if p.contains('/') || p.contains('\\') {
            asset_core::application::memory::project_path_to_slug(p)
        } else {
            p.to_string()
        }
    });
    let res = service
        .memory()
        .update_node_status(
            &st.node_id,
            status,
            project_slug.as_deref(),
            st.window.as_deref(),
        )
        .await?;

    match format {
        OutputFormat::Json => render::render_json_value(&serde_json::to_value(&res)?),
        OutputFormat::Table => {
            let mut table = comfy_table::Table::new();
            table.load_preset(comfy_table::presets::UTF8_FULL);
            table.set_header(vec![
                "Node ID",
                "Status",
                "Summary",
                "Turn Started At",
                "Updated At",
            ]);
            let started = res.turn_started_at.as_deref().unwrap_or("-");
            table.add_row(vec![
                &res.id,
                res.status.as_str(),
                &res.summary,
                started,
                &res.updated_at,
            ]);
            println!("{table}");
        }
    }
    Ok(())
}
