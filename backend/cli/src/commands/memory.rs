use crate::args::memory::{
    MemoryArgs, MemoryCommand, MemoryContextCommand, MemoryProjectCommand, MemoryRecallCommand,
    MemoryRecallSessionCommand, MemoryRecallTurnCommand, MemoryRecentCommand, MemoryTaskCommand,
};
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::Result;
use asset_core::domain::MemoryScope;

pub async fn handle_memory(args: MemoryArgs, _format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        MemoryCommand::Recent(r) => match r.command {
            MemoryRecentCommand::Get(g) => {
                let res = service.memory().get_recent_snapshot(g.date).await?;
                render::render_json_value(&res);
            }
        },
        MemoryCommand::Context(c) => match c.command {
            MemoryContextCommand::Resolve(r) => {
                let project_path = if r.current_project {
                    std::env::current_dir()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                } else {
                    r.project
                };
                let res = service
                    .memory()
                    .resolve_context(project_path, r.query, r.token_budget)
                    .await?;
                render::render_json_value(&res);
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
                    anyhow::anyhow!("project path is required (provide <project_path> or use --current-project)")
                })?;
                let res = service.memory().get_project(project_path).await?;
                render::render_json_value(&res);
            }
        },
        MemoryCommand::Rebuild(rb) => {
            let project_path = if rb.current_project {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
            } else {
                rb.project
            };
            let params = asset_core::application::MemoryScopeRebuildParams {
                scope: MemoryScope::default(),
                target: None,
                project_path,
                reason: None,
            };
            let res = service.memory().rebuild(params).await?;
            render::render_json_value(&res);
        }
        MemoryCommand::Task(t) => match t.command {
            MemoryTaskCommand::List(l) => {
                let res = service.memory().list_tasks(l.active_only)?;
                render::render_json_value(&res);
            }
            MemoryTaskCommand::Get(g) => {
                let res = service.memory().get_task(g.task_id)?;
                render::render_json_value(&res);
            }
            MemoryTaskCommand::Cancel(c) => {
                let res = service.memory().cancel_task(c.task_id)?;
                render::render_json_value(&res);
            }
            MemoryTaskCommand::Retry(rt) => {
                let res = service.memory().retry_task(rt.task_id).await?;
                render::render_json_value(&res);
            }
        },
        MemoryCommand::Recall(rc) => match rc.command {
            MemoryRecallCommand::Search(s) => {
                let params = asset_core::application::MemoryRecallSearchParams {
                    query: s.query,
                    scope: MemoryScope::default(),
                    since: None,
                    until: None,
                    file: None,
                    command: None,
                    error: None,
                    limit: Some(s.limit),
                    offset: Some(s.offset),
                };
                let res = service.memory().search_recall(params).await?;
                render::render_json_value(&res);
            }
            MemoryRecallCommand::Session(sess) => match sess.command {
                MemoryRecallSessionCommand::Create(_c) => {
                    let res = service
                        .memory()
                        .create_recall_session(MemoryScope::default())
                        .await?;
                    render::render_json_value(&res);
                }
                MemoryRecallSessionCommand::Get(g) => {
                    let res = service.memory().get_recall_session(g.session_id).await?;
                    render::render_json_value(&res);
                }
            },
            MemoryRecallCommand::Turn(t) => match t.command {
                MemoryRecallTurnCommand::Send(sd) => {
                    let res = service
                        .memory()
                        .send_recall_turn(sd.session_id, sd.query)
                        .await?;
                    render::render_json_value(&res);
                }
                MemoryRecallTurnCommand::Cancel(cx) => {
                    let res = service.memory().cancel_recall_turn(cx.turn_id).await?;
                    render::render_json_value(&res);
                }
            },
        },
    }
    Ok(())
}
