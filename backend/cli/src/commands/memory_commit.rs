use crate::args::memory::MemoryCommitArgs;
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::{Context, Result};
use asset_core::domain::{
    MemoryCommitResult, MemoryTreeNode, NodeStatus, TopicEvolutionCommitPayload,
};

pub async fn handle_commit(
    c: MemoryCommitArgs,
    service: &CliService,
    format: OutputFormat,
) -> Result<()> {
    let json_val = if let Some(file_path) = &c.file {
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            anyhow::anyhow!("failed to read payload file {}: {e}", file_path.display())
        })?;
        Some(serde_json::from_str::<serde_json::Value>(&content)?)
    } else if let Some(raw) = &c.payload {
        let content = if let Some(stripped) = raw.strip_prefix('@') {
            std::fs::read_to_string(stripped)
                .map_err(|e| anyhow::anyhow!("failed to read payload file {stripped}: {e}"))?
        } else {
            raw.clone()
        };
        Some(serde_json::from_str::<serde_json::Value>(&content)?)
    } else {
        None
    };

    if let Some(val) = json_val {
        let is_batch = val.is_array()
            || val
                .as_object()
                .map(|obj| obj.contains_key("commits"))
                .unwrap_or(false);

        if is_batch {
            let window = val
                .get("window")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| c.window.clone());

            let commits_array = if let Some(arr) = val.as_array() {
                arr.clone()
            } else {
                val.get("commits")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default()
            };

            let mut commit_results = Vec::new();
            for item in commits_array {
                let payload = value_to_commit_payload(item, service, c.project.as_deref()).await?;
                let res = service.memory().commit_evolution(payload).await?;
                commit_results.push(res);
            }

            let mut watermark_results = Vec::new();
            if let Some(wm_arr) = val.get("advance_watermarks").and_then(|v| v.as_array()) {
                for wm in wm_arr {
                    if let Some(obj) = wm.as_object() {
                        let raw_proj = obj
                            .get("project_slug")
                            .or_else(|| obj.get("project"))
                            .and_then(|v| v.as_str())
                            .or(c.project.as_deref())
                            .unwrap_or("");
                        let proj = if raw_proj.contains('/') || raw_proj.contains('\\') {
                            asset_core::application::memory::project_path_to_slug(raw_proj)
                        } else {
                            raw_proj.to_string()
                        };
                        let raw_sess = obj
                            .get("session_id")
                            .or_else(|| obj.get("short_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let resolved_sess = service.memory().resolve_session_id(raw_sess).await;
                        let turn = obj
                            .get("turn")
                            .or_else(|| obj.get("turn_index"))
                            .or_else(|| obj.get("advanced_to_turn"))
                            .and_then(|v| v.as_u64())
                            .map(|u| u as u32)
                            .unwrap_or(0);
                        let wm_res = service
                            .memory()
                            .advance_watermark(&proj, &resolved_sess, turn)
                            .await?;
                        watermark_results.push(wm_res);
                    }
                }
            }

            service
                .memory()
                .refresh_recent(window.as_deref())
                .await
                .context("failed to refresh recent memory state")?;

            match format {
                OutputFormat::Json => {
                    render::render_json_value(&serde_json::json!({
                        "commits": commit_results,
                        "advanced_watermarks": watermark_results,
                    }));
                }
                OutputFormat::Table => {
                    let mut table = comfy_table::Table::new();
                    table.load_preset(comfy_table::presets::UTF8_FULL);
                    table.set_header(vec![
                        "Committed Node ID",
                        "Project",
                        "Topic",
                        "Unexplored Sibling IDs",
                        "Updated At",
                    ]);
                    for cr in &commit_results {
                        let unexplored_str = cr.unexplored_node_ids.join(", ");
                        table.add_row(vec![
                            &cr.committed_node_id,
                            &cr.project_slug,
                            &cr.topic_id,
                            &unexplored_str,
                            &cr.updated_at,
                        ]);
                    }
                    println!("{table}");
                }
            }
            return Ok(());
        }

        // Single commit object in JSON
        let payload = value_to_commit_payload(val, service, c.project.as_deref()).await?;
        let res = service.memory().commit_evolution(payload).await?;
        if let Some(w) = &c.window {
            let _ = service.memory().refresh_recent(Some(w.as_str())).await;
        }

        render_single_commit_result(&res, format);
        return Ok(());
    }

    // Flat CLI arguments
    let raw_project = c
        .project
        .ok_or_else(|| anyhow::anyhow!("--project is required when not providing --payload"))?;
    let project_slug = if raw_project.contains('/') || raw_project.contains('\\') {
        asset_core::application::memory::project_path_to_slug(&raw_project)
    } else {
        raw_project
    };
    let raw_session_id = c
        .session_id
        .or_else(|| c.short_id.clone())
        .ok_or_else(|| anyhow::anyhow!("--session-id is required when not providing --payload"))?;
    let full_session_id = service.memory().resolve_session_id(&raw_session_id).await;
    let short_id = c
        .short_id
        .unwrap_or_else(|| asset_core::domain::conversation_id_fragment(&full_session_id));
    let turn = c
        .turn
        .ok_or_else(|| anyhow::anyhow!("--turn is required when not providing --payload"))?;
    let topic_id = c.topic.unwrap_or_else(|| "general".to_string());
    let summary = c
        .summary
        .ok_or_else(|| anyhow::anyhow!("--summary is required when not providing --payload"))?;
    let status: NodeStatus = c.status.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
    let parent_new_status = c
        .parent_status
        .as_deref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let now = chrono::Utc::now().to_rfc3339();
    let node_id = MemoryTreeNode::format_id(&short_id, turn, None);
    let new_node = MemoryTreeNode {
        id: node_id,
        parent_id: c.parent_id.clone(),
        topic_id: topic_id.clone(),
        summary,
        status,
        source_session_id: full_session_id.clone(),
        turn_index: turn,
        turn_started_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    let mut unexplored_siblings = Vec::new();
    for (i, unexp) in c.unexplored.into_iter().enumerate() {
        unexplored_siblings.push(MemoryTreeNode {
            id: MemoryTreeNode::format_id(&short_id, turn, Some((i + 1) as u32)),
            parent_id: c.parent_id.clone(),
            topic_id: topic_id.clone(),
            summary: unexp,
            status: NodeStatus::Unexplored,
            source_session_id: full_session_id.clone(),
            turn_index: turn,
            turn_started_at: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }

    let payload = TopicEvolutionCommitPayload {
        project_slug,
        topic_id,
        topic_name: c.topic_name,
        parent_node_id: c.parent_id,
        parent_new_status,
        new_node,
        unexplored_siblings,
    };

    let res = service.memory().commit_evolution(payload).await?;
    if let Some(w) = &c.window {
        let _ = service.memory().refresh_recent(Some(w.as_str())).await;
    }

    render_single_commit_result(&res, format);
    Ok(())
}

fn render_single_commit_result(res: &MemoryCommitResult, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render::render_json_value(&serde_json::to_value(res).unwrap_or_default());
        }
        OutputFormat::Table => {
            let mut table = comfy_table::Table::new();
            table.load_preset(comfy_table::presets::UTF8_FULL);
            table.set_header(vec![
                "Project",
                "Topic",
                "Committed Node ID",
                "Parent Node ID",
                "Unexplored IDs",
            ]);
            let parent = res.parent_node_id.as_deref().unwrap_or("-");
            let unexplored_str = res.unexplored_node_ids.join(", ");
            table.add_row(vec![
                &res.project_slug,
                &res.topic_id,
                &res.committed_node_id,
                parent,
                &unexplored_str,
            ]);
            println!("{table}");
        }
    }
}

async fn value_to_commit_payload(
    val: serde_json::Value,
    service: &CliService,
    cli_fallback_project: Option<&str>,
) -> Result<TopicEvolutionCommitPayload> {
    if let Ok(mut payload) = serde_json::from_value::<TopicEvolutionCommitPayload>(val.clone()) {
        if payload.project_slug.contains('/') || payload.project_slug.contains('\\') {
            payload.project_slug =
                asset_core::application::memory::project_path_to_slug(&payload.project_slug);
        }
        let resolved = service
            .memory()
            .resolve_session_id(&payload.new_node.source_session_id)
            .await;
        payload.new_node.source_session_id = resolved.clone();
        for sib in &mut payload.unexplored_siblings {
            sib.source_session_id = resolved.clone();
        }
        return Ok(payload);
    }

    let obj = val
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("commit payload must be a JSON object"))?;

    let raw_project = obj
        .get("project_slug")
        .or_else(|| obj.get("project"))
        .and_then(|v| v.as_str())
        .or(cli_fallback_project)
        .ok_or_else(|| anyhow::anyhow!("'project' or 'project_slug' is required"))?;
    let project_slug = if raw_project.contains('/') || raw_project.contains('\\') {
        asset_core::application::memory::project_path_to_slug(raw_project)
    } else {
        raw_project.to_string()
    };

    let raw_session_id = obj
        .get("session_id")
        .or_else(|| obj.get("short_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'session_id' is required"))?;
    let full_session_id = service.memory().resolve_session_id(raw_session_id).await;
    let short_id = obj
        .get("short_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| asset_core::domain::conversation_id_fragment(&full_session_id));

    let turn = obj
        .get("turn")
        .or_else(|| obj.get("turn_index"))
        .and_then(|v| v.as_u64())
        .map(|u| u as u32)
        .unwrap_or(1);

    let topic_id = obj
        .get("topic_id")
        .or_else(|| obj.get("topic"))
        .and_then(|v| v.as_str())
        .unwrap_or("general")
        .to_string();

    let topic_name = obj
        .get("topic_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let parent_node_id = obj
        .get("parent_node_id")
        .or_else(|| obj.get("parent_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && *s != "null" && *s != "none")
        .map(|s| s.to_string());

    let parent_new_status: Option<NodeStatus> = obj
        .get("parent_new_status")
        .or_else(|| obj.get("parent_status"))
        .and_then(|v| v.as_str())
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let now = chrono::Utc::now().to_rfc3339();

    let new_node = if let Some(node_val) = obj.get("new_node") {
        let mut node: MemoryTreeNode = serde_json::from_value(node_val.clone())
            .map_err(|e| anyhow::anyhow!("invalid new_node in commit payload: {e}"))?;
        node.source_session_id = service
            .memory()
            .resolve_session_id(&node.source_session_id)
            .await;
        node
    } else {
        let summary = if let Some(s) = obj.get("summary").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(t) = obj.get("title").and_then(|v| v.as_str()) {
            if let Some(d) = obj
                .get("detail")
                .and_then(|v| v.as_str())
                .filter(|d| !d.is_empty())
            {
                format!("{t}: {d}")
            } else {
                t.to_string()
            }
        } else {
            format!("Session {short_id} turn {turn}")
        };

        let status: NodeStatus = obj
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.parse())
            .transpose()
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .unwrap_or(NodeStatus::Completed);

        let node_id = obj
            .get("node_id")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| MemoryTreeNode::format_id(&short_id, turn, None));

        let turn_started_at = obj
            .get("turn_started_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let created_at = obj
            .get("created_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| now.clone());

        let updated_at = obj
            .get("updated_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| now.clone());

        MemoryTreeNode {
            id: node_id,
            parent_id: parent_node_id.clone(),
            topic_id: topic_id.clone(),
            summary,
            status,
            source_session_id: full_session_id.clone(),
            turn_index: turn,
            turn_started_at,
            created_at,
            updated_at,
        }
    };

    let mut unexplored_siblings = Vec::new();
    let raw_siblings = obj
        .get("unexplored_siblings")
        .or_else(|| obj.get("unexplored_branches"))
        .or_else(|| obj.get("unexplored"))
        .and_then(|v| v.as_array());

    if let Some(arr) = raw_siblings {
        for (i, item) in arr.iter().enumerate() {
            if let Some(s) = item.as_str() {
                unexplored_siblings.push(MemoryTreeNode {
                    id: MemoryTreeNode::format_id(&short_id, turn, Some((i + 1) as u32)),
                    parent_id: parent_node_id.clone(),
                    topic_id: topic_id.clone(),
                    summary: s.to_string(),
                    status: NodeStatus::Unexplored,
                    source_session_id: full_session_id.clone(),
                    turn_index: turn,
                    turn_started_at: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            } else if let Some(item_obj) = item.as_object() {
                let sib_summary = item_obj
                    .get("summary")
                    .or_else(|| item_obj.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let sib_id = item_obj
                    .get("id")
                    .or_else(|| item_obj.get("node_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        MemoryTreeNode::format_id(&short_id, turn, Some((i + 1) as u32))
                    });
                let sib_turn = item_obj
                    .get("turn")
                    .or_else(|| item_obj.get("turn_index"))
                    .and_then(|v| v.as_u64())
                    .map(|u| u as u32)
                    .unwrap_or(turn);
                let sib_started_at = item_obj
                    .get("turn_started_at")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                unexplored_siblings.push(MemoryTreeNode {
                    id: sib_id,
                    parent_id: parent_node_id.clone(),
                    topic_id: topic_id.clone(),
                    summary: sib_summary,
                    status: NodeStatus::Unexplored,
                    source_session_id: full_session_id.clone(),
                    turn_index: sib_turn,
                    turn_started_at: sib_started_at,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            }
        }
    }

    Ok(TopicEvolutionCommitPayload {
        project_slug,
        topic_id,
        topic_name,
        parent_node_id,
        parent_new_status,
        new_node,
        unexplored_siblings,
    })
}
