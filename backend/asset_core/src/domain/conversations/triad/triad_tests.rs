use super::*;
use crate::backend::domain::projection::{
    project_content_nodes_for_part, project_conversation_content_card,
};
use crate::backend::domain::{
    derive_conversation_part_id, ConversationCardKindDefinition, ConversationContentCardDescriptor,
    ConversationPart, ConversationPartKind, ConversationPartRole, NormalizedConversationPart,
};

fn sample_card_kinds() -> Vec<ConversationCardKindDefinition> {
    vec![
        ConversationCardKindDefinition {
            id: "codex.command".to_string(),
            semantic_role: None,
            label: "Command".to_string(),
            default_renderer: "command".to_string(),
            allowed_renderers: vec!["command".to_string()],
            icon_hint: None,
        },
        ConversationCardKindDefinition {
            id: "codex.result".to_string(),
            semantic_role: None,
            label: "Result".to_string(),
            default_renderer: "result".to_string(),
            allowed_renderers: vec!["result".to_string()],
            icon_hint: None,
        },
        ConversationCardKindDefinition {
            id: "codex.reasoning".to_string(),
            semantic_role: None,
            label: "Reasoning".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: None,
        },
        ConversationCardKindDefinition {
            id: "codex.answer".to_string(),
            semantic_role: None,
            label: "Answer".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: None,
        },
        ConversationCardKindDefinition {
            id: "codex.file-change".to_string(),
            semantic_role: None,
            label: "File Change".to_string(),
            default_renderer: "diff".to_string(),
            allowed_renderers: vec!["diff".to_string()],
            icon_hint: None,
        },
    ]
}

#[test]
fn test_split_drops_duplicate_user_parts() {
    let raw_parts = vec![
        NormalizedConversationPart {
            role: ConversationPartRole::User,
            kind: ConversationPartKind::Text,
            text: Some("重复用户提问".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codebuddy.user".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("这是最终回答".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.answer".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
    ];

    let triad = split_normalized_turn("codex", "原始提问", &raw_parts);
    assert_eq!(triad.prompt, "原始提问");
    assert_eq!(triad.response.as_deref(), Some("这是最终回答"));
    assert!(triad.parts.is_empty(), "重复 user 零件不应保留在 parts 中");
}

#[test]
fn test_split_aggregates_commands_and_hoists_common_cwd() {
    let raw_parts = vec![
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("vfox --version".to_string()),
            cwd: Some("/workspace/project".to_string()),
            status: Some("completed".to_string()),
            exit_code: Some(0),
            command_label: Some("检查版本".to_string()),
            source_execution_id: Some("exec-1".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.command".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: Some(r#"{"signal":"ambient"}"#.to_string()),
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: Some("测试失败摘要".to_string()),
            language: None,
            command: Some("cargo test".to_string()),
            cwd: Some("/workspace/project".to_string()),
            status: Some("failed".to_string()),
            exit_code: Some(101),
            command_label: Some("运行测试".to_string()),
            source_execution_id: Some("exec-2".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.command".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
    ];

    let triad = split_normalized_turn("codex", "跑测试", &raw_parts);
    let process = triad.process.expect("应产出 process");
    assert_eq!(process.cwd.as_deref(), Some("/workspace/project"));
    assert_eq!(process.items.len(), 2);

    // 第一个成功命令：exit_code 0 和 status completed 被省略，cwd 与顶层一致被省略
    let item1 = &process.items[0];
    assert_eq!(item1.seq, 0);
    assert_eq!(item1.content.as_deref(), Some("vfox --version"));
    assert_eq!(item1.label.as_deref(), Some("检查版本"));
    assert_eq!(item1.signal.as_deref(), Some("ambient"));
    assert_eq!(item1.cwd, None);
    assert_eq!(item1.exit_code, None);
    assert_eq!(item1.status, None);

    // 第二个失败命令：保留 exit_code 101 与 status failed
    let item2 = &process.items[1];
    assert_eq!(item2.seq, 1);
    assert_eq!(item2.content.as_deref(), Some("cargo test"));
    assert_eq!(item2.label.as_deref(), Some("运行测试"));
    assert_eq!(item2.exit_code, Some(101));
    assert_eq!(item2.status.as_deref(), Some("failed"));
    assert_eq!(item2.summary.as_deref(), Some("测试失败摘要"));
}

#[test]
fn test_split_absorbs_empty_result_and_drops_it() {
    let raw_parts = vec![
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("cargo build".to_string()),
            cwd: Some("/app".to_string()),
            status: Some("completed".to_string()),
            exit_code: Some(0),
            command_label: Some("编译".to_string()),
            source_execution_id: Some("call_abc123".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.command".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        // 空壳 .result 携带非零 exit_code 1
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Tool,
            text: Some("   ".to_string()), // 空白视为 None
            language: None,
            command: None,
            cwd: None,
            status: Some("failed".to_string()),
            exit_code: Some(1),
            command_label: None,
            source_execution_id: Some("call_abc123".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.result".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
    ];

    let triad = split_normalized_turn("codex", "构建", &raw_parts);
    assert!(triad.parts.is_empty(), "空壳 .result 必须被彻底丢弃");

    let process = triad.process.expect("必须有 process");
    assert_eq!(process.items.len(), 1);
    let cmd = &process.items[0];
    assert_eq!(
        cmd.exit_code,
        Some(1),
        "空壳 result 的非零 exit_code 应回填至对应命令"
    );
    assert_eq!(cmd.status.as_deref(), Some("failed"), "失败 status 应回填");
}

#[test]
fn test_split_links_non_empty_result_with_cmd_seq_and_purges_forbidden_attrs() {
    let raw_parts = vec![
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("npm test".to_string()),
            cwd: Some("/app".to_string()),
            status: Some("completed".to_string()),
            exit_code: Some(0),
            command_label: Some("单元测试".to_string()),
            source_execution_id: Some("exec_456".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.command".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        // 带报错正文的非空 .result
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Tool,
            text: Some("Error: 3 tests failed".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: Some("failed".to_string()),
            exit_code: Some(1),
            command_label: None,
            source_execution_id: Some("exec_456".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.result".to_string(),
                renderer: Some("result_renderer".to_string()),
                semantic_role: Some("status".to_string()),
            }),
            metadata_json: Some(r#"{"exec_id":"exec_456","payload_policy_version":1}"#.to_string()),
        },
    ];

    let triad = split_normalized_turn("codex", "测试", &raw_parts);
    assert_eq!(triad.parts.len(), 1, "非空 .result 应保留在 parts 中");
    let result_part = &triad.parts[0];
    assert_eq!(result_part.part_index, 1);
    assert_eq!(
        result_part.content.as_deref(),
        Some("Error: 3 tests failed")
    );

    let attrs_str = result_part
        .attrs_json
        .as_deref()
        .expect("应生成 attrs_json");
    let attrs: serde_json::Value = serde_json::from_str(attrs_str).unwrap();

    // 断言包含 cmd_seq 且指向命令 seq 0
    assert_eq!(attrs.get("cmd_seq").and_then(|v| v.as_i64()), Some(0));
    assert_eq!(attrs.get("exit_code").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(attrs.get("status").and_then(|v| v.as_str()), Some("failed"));

    // 断言禁止出现的键均不存在
    assert!(attrs.get("exec_id").is_none());
    assert!(attrs.get("call_id").is_none());
    assert!(attrs.get("renderer").is_none());
    assert!(attrs.get("semantic_role").is_none());
    assert!(attrs.get("payload_policy_version").is_none());
}

#[test]
fn test_split_separates_intermediate_reasoning_and_final_response() {
    let raw_parts = vec![
        // 中途思考播报 1（出现在命令之前）
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("我正在检查环境配置...".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.answer".to_string(), // 旧误标为 .answer
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        // 命令
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("git status".to_string()),
            cwd: Some("/repo".to_string()),
            status: Some("completed".to_string()),
            exit_code: Some(0),
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.command".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        // 中途思考播报 2（夹在命令之后但并非末尾）
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("我发现代码树很干净，准备输出分析...".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.reasoning".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        // 文件变更
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::FileChange,
            text: Some("diff --git a/foo b/foo".to_string()),
            language: Some("diff".to_string()),
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.file-change".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        // 轮次末尾唯一最终回答
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("全部检查已完成，一切正常。".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.answer".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
    ];

    let triad = split_normalized_turn("codex", "检查仓库", &raw_parts);
    assert_eq!(
        triad.response.as_deref(),
        Some("全部检查已完成，一切正常。")
    );
    let process = triad.process.expect("必须有 process");
    assert_eq!(
        process.response_seq,
        Some(4),
        "末尾回答序号记入 process.response_seq"
    );

    // 中间 parts 包含：原 part 0（纠偏为 reasoning）、原 part 2（reasoning）、原 part 3（file-change）
    assert_eq!(triad.parts.len(), 3);
    assert_eq!(triad.parts[0].part_index, 0);
    assert_eq!(
        triad.parts[0].kind, "codex.reasoning",
        "中途伪 answer 归位为 reasoning"
    );
    assert_eq!(
        triad.parts[0].content.as_deref(),
        Some("我正在检查环境配置...")
    );

    assert_eq!(triad.parts[1].part_index, 2);
    assert_eq!(triad.parts[1].kind, "codex.reasoning");

    assert_eq!(triad.parts[2].part_index, 3);
    assert_eq!(triad.parts[2].kind, "codex.file-change");
}

#[test]
fn test_roundtrip_merge_and_content_node_projection_equivalence() {
    let prefix = "conversation-part";
    let turn_id = "turn-test-123";
    let adapter_id = "codex";
    let card_kinds = sample_card_kinds();

    // 构造一个交错的完整轮次：思考 -> 命令 0 -> 报错结果 1 -> 文件变更 2 -> 最终回答 3
    let raw_parts = vec![
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("开始执行任务".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.reasoning".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("cargo test --lib".to_string()),
            cwd: Some("/repo".to_string()),
            status: Some("failed".to_string()),
            exit_code: Some(101),
            command_label: Some("测试".to_string()),
            source_execution_id: Some("call-cmd-1".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.command".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Tool,
            text: Some("FAILED: 1 test panicked".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: Some("failed".to_string()),
            exit_code: Some(101),
            command_label: None,
            source_execution_id: Some("call-cmd-1".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.result".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::FileChange,
            text: Some("+ fixed code".to_string()),
            language: Some("diff".to_string()),
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.file-change".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("任务已完成。".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.answer".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
    ];

    // 1. 执行拆分
    let triad = split_normalized_turn(adapter_id, "运行测试", &raw_parts);

    // 2. 执行读取向归并还原
    let restored_parts = merge_turn_triad(
        adapter_id,
        prefix,
        turn_id,
        &triad.prompt,
        triad.process.as_ref(),
        triad.response.as_deref(),
        &triad.parts,
    );

    assert_eq!(restored_parts.len(), 5, "应完整还原 5 个原始零件");

    // 3. 逐个验证还原后的序号、ID 及配对
    for (expected_idx, part) in restored_parts.iter().enumerate() {
        assert_eq!(part.part_index as usize, expected_idx);
        let expected_id = derive_conversation_part_id(turn_id, expected_idx as i64);
        assert_eq!(part.id, expected_id, "8 位短 ID / part_id 必须精确一致");
    }

    // 验证命令与结果的 source_execution_id 关联
    let cmd_part = &restored_parts[1];
    let result_part = &restored_parts[2];
    assert!(cmd_part.source_execution_id.is_some());
    assert_eq!(
        cmd_part.source_execution_id, result_part.source_execution_id,
        "命令与报错结果必须拥有相同的配对 source_execution_id 供前端折叠"
    );

    // 4. 投影为 ConversationContentNode
    let mut all_nodes = Vec::new();
    for part in &restored_parts {
        let card = project_conversation_content_card(part, adapter_id, &card_kinds)
            .expect("投影卡片不应报错")
            .expect("卡片必须存在");
        let candidate = card.into();
        let nodes = project_content_nodes_for_part("q-1", 0, part, &[candidate]);
        all_nodes.extend(nodes);
    }

    assert_eq!(all_nodes.len(), 5);
    assert_eq!(all_nodes[0].node_type, "codex.reasoning");
    assert_eq!(all_nodes[1].node_type, "codex.command");
    assert_eq!(all_nodes[2].node_type, "codex.result");
    assert_eq!(all_nodes[3].node_type, "codex.file-change");
    assert_eq!(all_nodes[4].node_type, "codex.answer");

    assert_eq!(
        all_nodes[1].source_execution_id,
        all_nodes[2].source_execution_id
    );
    assert!(all_nodes[1].source_execution_id.is_some());
}

#[test]
fn test_subagent_triad_roundtrip_with_commands_cards_and_response() {
    let adapter_id = "codex";
    let prefix = "codex";
    let turn_id = "subagent-turn-001";
    let card_kinds = sample_card_kinds();

    let subagent_prompt =
        "Subagent delegation: investigate benchmark bottlenecks across worker threads";

    let raw_parts = vec![
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Analyzing thread allocation and memory locks...".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.reasoning".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("cargo test -p asset_core --release".to_string()),
            cwd: Some("/workspace".to_string()),
            status: Some("success".to_string()),
            exit_code: Some(0),
            command_label: Some("Execute release tests".to_string()),
            source_execution_id: Some("subagent-exec-1".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.command".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Text,
            text: Some("All 45 tests passed without contention.".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: Some("success".to_string()),
            exit_code: Some(0),
            command_label: None,
            source_execution_id: Some("subagent-exec-1".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.result".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::FileChange,
            text: Some("Diff summary: thread pool lock contention reduced by 40%".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.file-change".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Subagent report: Bottlenecks resolved and verified.".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "codex.answer".to_string(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
        },
    ];

    // 1. Split turn triad
    let triad = split_normalized_turn(adapter_id, subagent_prompt, &raw_parts);
    assert_eq!(
        triad.prompt, subagent_prompt,
        "Subagent delegation prompt preserved"
    );
    assert_eq!(
        triad.response.as_deref(),
        Some("Subagent report: Bottlenecks resolved and verified."),
        "Subagent final answer preserved as response"
    );
    assert!(triad.process.is_some(), "Process items preserved");

    // 2. Merge turn triad
    let restored_parts = merge_turn_triad(
        adapter_id,
        prefix,
        turn_id,
        &triad.prompt,
        triad.process.as_ref(),
        triad.response.as_deref(),
        &triad.parts,
    );
    assert_eq!(
        restored_parts.len(),
        5,
        "All 5 subagent parts faithfully restored"
    );

    // Check indices and IDs
    for (i, p) in restored_parts.iter().enumerate() {
        assert_eq!(p.part_index as usize, i);
        assert_eq!(p.turn_id, turn_id);
    }

    // Check command and result pairing
    let cmd = &restored_parts[1];
    let res = &restored_parts[2];
    assert_eq!(
        cmd.command.as_deref(),
        Some("cargo test -p asset_core --release")
    );
    assert_eq!(cmd.source_execution_id, res.source_execution_id);

    // 3. Project content cards
    let mut all_nodes = Vec::new();
    for part in &restored_parts {
        let card = project_conversation_content_card(part, adapter_id, &card_kinds)
            .expect("Card projection succeeds")
            .expect("Card exists");
        let candidate = card.into();
        let nodes = project_content_nodes_for_part("q-subagent", 0, part, &[candidate]);
        all_nodes.extend(nodes);
    }
    assert_eq!(all_nodes.len(), 5);
    assert_eq!(all_nodes[0].node_type, "codex.reasoning");
    assert_eq!(all_nodes[1].node_type, "codex.command");
    assert_eq!(all_nodes[2].node_type, "codex.result");
    assert_eq!(all_nodes[3].node_type, "codex.file-change");
    assert_eq!(all_nodes[4].node_type, "codex.answer");
}
