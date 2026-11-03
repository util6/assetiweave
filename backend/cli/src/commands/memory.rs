use crate::args::memory::{
    MemoryArgs, MemoryCommand, MemoryContextCommand, MemoryProjectCommand, MemoryRecallCommand,
    MemoryRecallSessionCommand, MemoryRecallTurnCommand, MemoryRecentCommand, MemoryStateCommand,
    MemoryTaskCommand, MemoryWatermarkCommand,
};
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::Result;
use asset_core::domain::MemoryScope;

pub async fn handle_memory(args: MemoryArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        MemoryCommand::Recent(r) => match r.command {
            MemoryRecentCommand::Get(g) => {
                let res = service.memory().get_recent_snapshot(g.date).await?;
                render::render_memory_markdown_view(&res, format);
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
                render::render_memory_markdown_view(&res, format);
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
            let target = rb
                .target
                .as_deref()
                .and_then(|t| match t.to_lowercase().as_str() {
                    "recent" => Some(asset_core::application::MemoryRebuildTarget::Recent),
                    "project" => Some(asset_core::application::MemoryRebuildTarget::Project),
                    "global" => Some(asset_core::application::MemoryRebuildTarget::Global),
                    "all" => Some(asset_core::application::MemoryRebuildTarget::All),
                    _ => None,
                });
            let reason = match rb.reason.to_lowercase().as_str() {
                "migration" => Some(asset_core::application::MemoryRebuildReason::Migration),
                "projection_repair" => {
                    Some(asset_core::application::MemoryRebuildReason::ProjectionRepair)
                }
                _ => Some(asset_core::application::MemoryRebuildReason::Manual),
            };
            let params = asset_core::application::MemoryScopeRebuildParams {
                scope: MemoryScope::default(),
                target,
                project_path,
                reason,
                force: rb.force,
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
        MemoryCommand::State(s) => match s.command {
            MemoryStateCommand::Get(g) => {
                let res = service.memory().get_state(g.project).await?;
                if g.raw {
                    println!("{}", serde_json::to_string_pretty(&res)?);
                } else {
                    render::render_json_value(&res);
                }
            }
        },
        MemoryCommand::Commit(c) => {
            let payload: asset_core::domain::TopicEvolutionCommitPayload = if let Some(file_path) =
                c.file
            {
                let json_str = std::fs::read_to_string(&file_path).map_err(|e| {
                    anyhow::anyhow!("failed to read payload file {}: {e}", file_path.display())
                })?;
                serde_json::from_str(&json_str)
                    .map_err(|e| anyhow::anyhow!("invalid commit JSON payload: {e}"))?
            } else if let Some(raw) = c.payload {
                let json_str = if let Some(stripped) = raw.strip_prefix('@') {
                    std::fs::read_to_string(stripped).map_err(|e| {
                        anyhow::anyhow!("failed to read payload file {stripped}: {e}")
                    })?
                } else {
                    raw
                };
                serde_json::from_str(&json_str)
                    .map_err(|e| anyhow::anyhow!("invalid commit JSON payload: {e}"))?
            } else {
                let project_slug = c.project.ok_or_else(|| {
                    anyhow::anyhow!("--project is required when not providing --payload")
                })?;
                let topic_id = c.topic.ok_or_else(|| {
                    anyhow::anyhow!("--topic is required when not providing --payload")
                })?;
                let summary = c.summary.ok_or_else(|| {
                    anyhow::anyhow!("--summary is required when not providing --payload")
                })?;
                let session_id = c.session_id.ok_or_else(|| {
                    anyhow::anyhow!("--session-id is required when not providing --payload")
                })?;
                let turn = c.turn.ok_or_else(|| {
                    anyhow::anyhow!("--turn is required when not providing --payload")
                })?;
                let short_id = c
                    .short_id
                    .unwrap_or_else(|| session_id.chars().take(8).collect::<String>());
                let status: asset_core::domain::NodeStatus =
                    c.status.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
                let parent_new_status = c
                    .parent_status
                    .as_deref()
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let now = chrono::Utc::now().to_rfc3339();
                let node_id = asset_core::domain::MemoryTreeNode::format_id(&short_id, turn, None);
                let new_node = asset_core::domain::MemoryTreeNode {
                    id: node_id,
                    parent_id: c.parent_id.clone(),
                    topic_id: topic_id.clone(),
                    summary,
                    status,
                    source_session_id: session_id,
                    turn_index: turn,
                    created_at: now.clone(),
                    updated_at: now,
                };
                asset_core::domain::TopicEvolutionCommitPayload {
                    project_slug,
                    topic_id,
                    topic_name: c.topic_name,
                    parent_node_id: c.parent_id,
                    parent_new_status,
                    new_node,
                    unexplored_siblings: vec![],
                }
            };
            let res = service.memory().commit_evolution(payload).await?;
            render::render_json_value(&res);
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
