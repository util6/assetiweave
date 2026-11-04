use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::agents::definition::AgentId;
use crate::backend::domain::memory::{
    MemoryTreeNode, NodeStatus, TopicEvolutionCommitPayload, TopicTree,
};
use crate::backend::infrastructure::agent_execution::{
    AgentExecutionRuntime, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
    AiExecutionPurpose, AiExecutionRequest,
};
use serde::{Deserialize, Serialize};

/// High-level action chosen by the reasoning agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MemoryAgentDecision {
    Inspect(MemoryInspectDecision),
    Commit(MemoryCommitDecision),
}

/// Agent request to inspect full Q&A turn text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryInspectDecision {
    #[serde(default)]
    pub turn_index: Option<u32>,
    #[serde(default)]
    pub turns: Vec<u32>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl MemoryInspectDecision {
    pub fn target_turn(&self) -> Option<u32> {
        self.turn_index.or_else(|| self.turns.first().copied())
    }
}

/// Agent payload committing cognitive evolution to the topic tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCommitDecision {
    #[serde(default)]
    pub evolution: Option<TopicEvolutionCommitPayload>,
    #[serde(default)]
    pub project_slug: Option<String>,
    #[serde(default)]
    pub topic_id: Option<String>,
    #[serde(default)]
    pub topic_name: Option<String>,
    #[serde(default)]
    pub parent_node_id: Option<String>,
    #[serde(default)]
    pub parent_status: Option<NodeStatus>,
    #[serde(default)]
    pub parent_new_status: Option<NodeStatus>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub status: Option<NodeStatus>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub has_signal: Option<bool>,
    #[serde(default)]
    pub new_node: Option<MemoryTreeNode>,
    #[serde(default)]
    pub unexplored_branches: Vec<SimplifiedBranchItem>,
    #[serde(default)]
    pub unexplored_siblings: Vec<SimplifiedBranchItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimplifiedBranchItem {
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub turn_index: Option<u32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

impl MemoryCommitDecision {
    pub fn into_payload(
        self,
        default_project_slug: &str,
        session_id: &str,
        short_id: &str,
        turn_index: u32,
    ) -> TopicEvolutionCommitPayload {
        if let Some(mut evo) = self.evolution {
            if evo.project_slug.is_empty() {
                evo.project_slug = default_project_slug.to_string();
            }
            return evo;
        }

        let project_slug = self
            .project_slug
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_project_slug.to_string());
        let topic_id = self
            .topic_id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "general".to_string());
        let parent_node_id = self
            .parent_node_id
            .filter(|s| !s.is_empty() && s != "null" && s != "none");
        let parent_new_status = self.parent_new_status.or(self.parent_status);
        let now = chrono::Utc::now().to_rfc3339();

        let new_node = if let Some(mut node) = self.new_node {
            if node.source_session_id.is_empty() {
                node.source_session_id = session_id.to_string();
            }
            if node.topic_id.is_empty() {
                node.topic_id = topic_id.clone();
            }
            node.parent_id = node
                .parent_id
                .filter(|s| !s.is_empty() && s != "null" && s != "none");
            node
        } else {
            let summary = match (self.title, self.detail, self.summary) {
                (Some(t), Some(d), _) if !d.is_empty() => format!("{t}: {d}"),
                (Some(t), _, _) => t,
                (None, Some(d), _) => d,
                (None, None, Some(s)) => s,
                (None, None, None) => format!("Session {short_id} turn {turn_index}"),
            };
            let status = self.status.unwrap_or(NodeStatus::Completed);
            let node_id = self
                .node_id
                .unwrap_or_else(|| MemoryTreeNode::format_id(short_id, turn_index, None));

            MemoryTreeNode {
                id: node_id,
                parent_id: parent_node_id.clone(),
                topic_id: topic_id.clone(),
                summary,
                status,
                source_session_id: session_id.to_string(),
                turn_index,
                turn_started_at: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            }
        };

        let mut siblings = Vec::new();
        for (i, branch) in self
            .unexplored_siblings
            .into_iter()
            .chain(self.unexplored_branches.into_iter())
            .enumerate()
        {
            let sub_id = branch.node_id.unwrap_or_else(|| {
                MemoryTreeNode::format_id(short_id, turn_index, Some((i + 1) as u32))
            });
            let branch_title = branch.title.or(branch.summary).unwrap_or_default();
            let summary = match branch.detail {
                Some(d) if !d.is_empty() => {
                    if branch_title.is_empty() {
                        d
                    } else {
                        format!("{}: {}", branch_title, d)
                    }
                }
                _ => branch_title,
            };
            siblings.push(MemoryTreeNode {
                id: sub_id,
                parent_id: parent_node_id.clone(),
                topic_id: topic_id.clone(),
                summary,
                status: NodeStatus::Unexplored,
                source_session_id: session_id.to_string(),
                turn_index: branch.turn_index.unwrap_or(turn_index),
                turn_started_at: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }

        TopicEvolutionCommitPayload {
            project_slug,
            topic_id,
            topic_name: self.topic_name,
            parent_node_id,
            parent_new_status,
            new_node,
            unexplored_siblings: siblings,
        }
    }
}

/// Unpeel markdown code blocks and extract valid JSON value.
pub fn unpeel_json(raw: &str) -> AppResult<serde_json::Value> {
    let trimmed = raw.trim();

    // 1. Direct parse attempt
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(val);
    }

    // 2. Unpeel markdown code block ```json ... ``` or ``` ... ```
    if let Some(start_fence) = trimmed.find("```") {
        let after_fence = &trimmed[start_fence + 3..];
        let content_start = if let Some(newline_idx) = after_fence.find('\n') {
            newline_idx + 1
        } else {
            0
        };
        let body = &after_fence[content_start..];
        if let Some(end_fence) = body.rfind("```") {
            let inside = body[..end_fence].trim();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(inside) {
                return Ok(val);
            }
        }
    }

    // 3. Scan outer curly braces '{' and '}'
    if let (Some(first_brace), Some(last_brace)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if first_brace < last_brace {
            let candidate = &trimmed[first_brace..=last_brace];
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(candidate) {
                return Ok(val);
            }
        }
    }

    Err(AppError::Validation(format!(
        "Failed to extract valid JSON from input: {}",
        trimmed.chars().take(120).collect::<String>()
    )))
}

/// Parse agent decision from raw LLM text with unpeeling.
pub fn parse_agent_decision(raw: &str) -> AppResult<MemoryAgentDecision> {
    let val = unpeel_json(raw)?;
    serde_json::from_value::<MemoryAgentDecision>(val)
        .map_err(|e| AppError::Validation(format!("Invalid MemoryAgentDecision schema: {e}")))
}

/// Construct Step 1 triage prompt.
pub fn build_step1_prompt(
    session_id: &str,
    short_id: &str,
    project_slug: &str,
    turns: &[(usize, String, String)],
    existing_tree_summary: &str,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are an expert cognitive memory extraction agent for AssetIWeave.\n");
    prompt.push_str(
        "Analyze the conversation session outline below and determine its cognitive evolution.\n\n",
    );
    prompt.push_str(&format!(
        "Session: {} (Short ID: {})\nProject: {}\n\n",
        session_id, short_id, project_slug
    ));

    prompt.push_str("=== Session Turns Outline ===\n");
    for (idx, q_preview, a_preview) in turns {
        prompt.push_str(&format!(
            "- Turn {idx}:\n  User: {q_preview}\n  Assistant: {a_preview}\n"
        ));
    }
    prompt.push('\n');

    if !existing_tree_summary.is_empty() {
        prompt.push_str("=== Existing Project Topics & Nodes ===\n");
        prompt.push_str(existing_tree_summary);
        prompt.push_str("\n\n");
    }

    prompt.push_str("=== Instructions ===\n");
    prompt.push_str("1. If you need full text of a specific turn to verify technical conclusions or critical code/decisions, output JSON:\n");
    prompt.push_str(
        "   {\"action\": \"inspect\", \"turn_index\": <number>, \"reason\": \"<explanation>\"}\n",
    );
    prompt.push_str("2. Otherwise, commit the evolution by outputting JSON:\n");
    prompt.push_str("   {\n");
    prompt.push_str("     \"action\": \"commit\",\n");
    prompt.push_str("     \"topic_id\": \"<topic_slug>\",\n");
    prompt.push_str("     \"parent_node_id\": \"<optional_parent_node_id>\",\n");
    prompt.push_str("     \"parent_status\": \"continued\" | \"completed\",\n");
    prompt.push_str("     \"title\": \"<concise conclusion title>\",\n");
    prompt.push_str("     \"detail\": \"<refined technical takeaways>\",\n");
    prompt.push_str("     \"status\": \"completed\" | \"continued\",\n");
    prompt.push_str("     \"unexplored_branches\": [\n");
    prompt.push_str("       {\"title\": \"<orthogonal open topic>\", \"detail\": \"<brief>\"}\n");
    prompt.push_str("     ]\n");
    prompt.push_str("   }\n\n");
    prompt.push_str("OUTPUT ONLY THE JSON OBJECT. No explanation, no backticks, DO NOT call any tools or functions.\n");
    prompt
}

/// Construct Step 2 commit prompt with injected turn Q&A text.
pub fn build_step2_prompt(
    step1_prompt: &str,
    target_turn: u32,
    question: &str,
    answer: &str,
) -> String {
    let mut prompt = String::new();
    prompt.push_str(step1_prompt);
    prompt.push_str("\n\n=== Injected Detail for Inspected Turn ===\n");
    prompt.push_str(&format!(
        "Turn {target_turn} Full Context:\nQuestion:\n{question}\n\nAnswer:\n{answer}\n\n"
    ));
    prompt.push_str("=== Step 2 Final Requirement ===\n");
    prompt.push_str("You have received the requested detail. You MUST now output action: \"commit\". No further inspect is permitted.\n");
    prompt.push_str("OUTPUT ONLY THE JSON OBJECT. DO NOT call any tools or functions.\n");
    prompt
}

/// Construct single-retry feedback prompt on JSON syntax error.
pub fn build_retry_prompt(original_prompt: &str, error_message: &str, raw_output: &str) -> String {
    let mut prompt = String::new();
    prompt.push_str(original_prompt);
    prompt.push_str("\n\n=== Syntax Correction Notice ===\n");
    prompt.push_str("Your previous response could not be parsed as valid JSON.\n");
    prompt.push_str(&format!("Parser Error: {error_message}\n\n"));
    prompt.push_str("Your Previous Response Was:\n");
    prompt.push_str(raw_output);
    prompt.push_str("\n\nPlease fix the JSON syntax and return ONLY the valid JSON object.\n");
    prompt
}

/// Summarize topic trees for prompt context.
pub fn format_topic_tree_summary(
    topic_trees: &std::collections::HashMap<String, TopicTree>,
) -> String {
    let mut summary = String::new();
    let mut keys: Vec<&String> = topic_trees.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(tree) = topic_trees.get(key) {
            summary.push_str(&format!("Topic [{}]: {}\n", tree.topic_id, tree.topic_name));
            let mut node_keys: Vec<&String> = tree.nodes.keys().collect();
            node_keys.sort();
            for n_key in node_keys {
                if let Some(node) = tree.nodes.get(n_key) {
                    summary.push_str(&format!(
                        "  - Node {}: [{}] {}\n",
                        node.id,
                        node.status.as_str(),
                        node.summary
                    ));
                }
            }
        }
    }
    summary
}

/// Execute the ACP inspect/commit 2-step agent loop with single-round fuse and 1-time retry.
pub async fn execute_inspect_commit_loop<F>(
    runtime: &dyn AgentExecutionRuntime,
    agent_id: &AgentId,
    model: Option<&str>,
    session_id: &str,
    short_id: &str,
    project_slug: &str,
    max_turn: u32,
    turns_outline: &[(usize, String, String)],
    existing_tree_summary: &str,
    qa_lookup: F,
) -> AppResult<TopicEvolutionCommitPayload>
where
    F: Fn(u32) -> Option<(String, String)>,
{
    let step1_prompt = build_step1_prompt(
        session_id,
        short_id,
        project_slug,
        turns_outline,
        existing_tree_summary,
    );

    let decision =
        execute_prompt_with_single_retry(runtime, agent_id, model, &step1_prompt).await?;

    match decision {
        MemoryAgentDecision::Commit(commit) => {
            Ok(commit.into_payload(project_slug, session_id, short_id, max_turn))
        }
        MemoryAgentDecision::Inspect(inspect) => {
            let target_turn = inspect.target_turn().unwrap_or(max_turn);
            let (q, a) = qa_lookup(target_turn).unwrap_or_else(|| {
                (
                    "Turn question not found".to_string(),
                    "Turn answer not found".to_string(),
                )
            });

            let step2_prompt = build_step2_prompt(&step1_prompt, target_turn, &q, &a);
            let step2_decision =
                execute_prompt_with_single_retry(runtime, agent_id, model, &step2_prompt).await?;

            match step2_decision {
                MemoryAgentDecision::Commit(commit) => {
                    Ok(commit.into_payload(project_slug, session_id, short_id, max_turn))
                }
                MemoryAgentDecision::Inspect(_) => {
                    // Strict single-round fuse: max 1 inspect round per session!
                    tracing::warn!(
                        session_id,
                        target_turn,
                        "Model returned second inspect request; strict single-round fuse tripped"
                    );
                    let fallback = MemoryCommitDecision {
                        evolution: None,
                        project_slug: Some(project_slug.to_string()),
                        topic_id: Some("general".to_string()),
                        topic_name: None,
                        parent_node_id: None,
                        parent_status: None,
                        parent_new_status: None,
                        node_id: Some(MemoryTreeNode::format_id(short_id, max_turn, None)),
                        status: Some(NodeStatus::Completed),
                        title: Some(format!("Session {short_id} turn {max_turn}")),
                        detail: Some("Completed turn analysis (inspect fuse tripped)".to_string()),
                        summary: None,
                        has_signal: Some(false),
                        new_node: None,
                        unexplored_branches: Vec::new(),
                        unexplored_siblings: Vec::new(),
                    };
                    Ok(fallback.into_payload(project_slug, session_id, short_id, max_turn))
                }
            }
        }
    }
}

async fn execute_prompt_with_single_retry(
    runtime: &dyn AgentExecutionRuntime,
    agent_id: &AgentId,
    model: Option<&str>,
    initial_prompt: &str,
) -> AppResult<MemoryAgentDecision> {
    let mut current_prompt = initial_prompt.to_string();
    let mut retried = false;

    loop {
        let req = AiExecutionRequest {
            execution_id: format!("memgen-{}", uuid::Uuid::new_v4()),
            agent_id: agent_id.clone(),
            purpose: AiExecutionPurpose::MemoryGeneration,
            session_mode: AgentSessionMode::OneShot,
            prompt: current_prompt.clone(),
            model: model.map(str::to_string),
            limits: AiExecutionLimits::default(),
            cancellation: AiExecutionCancellation::default(),
            progress: None,
            tenant_id: None,
            execution_context_key: None,
            binding: None,
            replay: false,
            restore_only: false,
            recall_tools: None,
            account: None,
        };

        let result = runtime
            .execute(req)
            .await
            .map_err(|e| AppError::External(e.to_string()))?;

        match parse_agent_decision(&result.text) {
            Ok(decision) => return Ok(decision),
            Err(parse_err) => {
                if !retried {
                    retried = true;
                    current_prompt =
                        build_retry_prompt(initial_prompt, &parse_err.to_string(), &result.text);
                } else {
                    return Err(AppError::Validation(format!(
                        "Failed to parse LLM response after 1 retry: {parse_err}"
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "reasoning_loop_tests.rs"]
mod tests;
