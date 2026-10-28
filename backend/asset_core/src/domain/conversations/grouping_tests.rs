use super::*;
use crate::backend::domain::conversations::ConversationGroupingOrigin;

#[test]
fn only_merges_exact_simple_acknowledgements() {
    assert!(should_auto_merge_acknowledgement("继续"));
    assert!(should_auto_merge_acknowledgement("OK"));
    assert!(!should_auto_merge_acknowledgement("继续解释一下原因"));
    assert!(!should_auto_merge_acknowledgement("ok?"));
    assert!(!should_auto_merge_acknowledgement("ok\nnow add tests"));
}

#[test]
fn groups_acknowledgement_turns_with_previous_question() {
    let groups = group_turn_ids_by_question(vec![
        ("t1".to_string(), "How does sync work?".to_string()),
        ("t2".to_string(), "继续".to_string()),
        ("t3".to_string(), "Now export it".to_string()),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t1", "t2"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t3"]);
}

#[test]
fn groups_interruption_recovery_and_micro_follow_up_with_previous_question() {
    let groups = group_turn_ids_by_question(vec![
        ("t1".to_string(), "Design the sync boundary".to_string()),
        ("t2".to_string(), "继续上一个问题".to_string()),
        ("t3".to_string(), "改成 Rust".to_string()),
        ("t4".to_string(), "Now export it".to_string()),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t1", "t2", "t3"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t4"]);
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
}

#[test]
fn dynamic_projection_groups_and_derives_deterministic_question_ids() {
    struct TestTurn {
        id: String,
        prompt: String,
    }
    let turns = vec![
        TestTurn {
            id: "conversation-turn-0001".to_string(),
            prompt: "What is Ticket 01?".to_string(),
        },
        TestTurn {
            id: "conversation-turn-0002".to_string(),
            prompt: "继续".to_string(),
        },
        TestTurn {
            id: "conversation-turn-0003".to_string(),
            prompt: "Now explain Ticket 02".to_string(),
        },
    ];

    let groups =
        group_turns_by_dynamic_projection("conversation-question", turns, |t| &t.prompt, |t| &t.id);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].question_id, "conversation-question-0001");
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[0].turns.len(), 2);
    assert_eq!(groups[0].turns[0].id, "conversation-turn-0001");
    assert_eq!(groups[0].turns[1].id, "conversation-turn-0002");

    assert_eq!(groups[1].question_id, "conversation-question-0003");
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
    assert_eq!(groups[1].turns.len(), 1);
    assert_eq!(groups[1].turns[0].id, "conversation-turn-0003");

    assert_eq!(
        derive_turn_id_from_question_id("conversation-question-0001"),
        Some("conversation-turn-0001".to_string())
    );
    assert_eq!(
        derive_turn_id_from_question_id("web-record-question-0001"),
        Some("web-record-turn-0001".to_string())
    );
    assert_eq!(derive_turn_id_from_question_id("invalid-question"), None);
}

#[test]
fn test_derive_question_id_from_turn() {
    assert_eq!(
        derive_question_id_from_turn("conversation-turn-abc12345"),
        "conversation-question-abc12345"
    );
    assert_eq!(
        derive_question_id_from_turn("web-record-turn-def67890"),
        "web-record-question-def67890"
    );
    assert_eq!(derive_question_id_from_turn("custom-q-1"), "custom-q-1");
}

#[test]
fn groups_subagent_turns_with_multi_sentence_prompts_under_active_question() {
    use crate::backend::domain::conversations::ConversationTurn;

    let turns = vec![
        ConversationTurn {
            id: "conversation-turn-0001".to_string(),
            session_id: "session-1".to_string(),
            external_id: "ext-1".to_string(),
            turn_index: 0,
            prompt: "Please investigate the database schema and fix the issue.".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            fingerprint: "fp1".to_string(),
            missing: false,
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            model: Some("gemini-2.5-pro".to_string()),
            process: None,
            response: Some("Starting analysis...".to_string()),
            turn_kind: None,
            agent_role: None,
            parent_turn_id: None,
        },
        ConversationTurn {
            id: "conversation-turn-0002".to_string(),
            session_id: "session-1".to_string(),
            external_id: "ext-2".to_string(),
            turn_index: 1,
            // Multi-sentence prompt with question marks and newlines that would fail simple text merging
            prompt: "Investigate backend/asset_core migrations.\nAre all columns intact?\nCheck foreign keys: `tenant_id`.".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            fingerprint: "fp2".to_string(),
            missing: false,
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            model: Some("gemini-2.5-flash".to_string()),
            process: None,
            response: Some("All tables verified.".to_string()),
            turn_kind: Some("subagent".to_string()),
            agent_role: Some("investigator".to_string()),
            parent_turn_id: Some("conversation-turn-0001".to_string()),
        },
        ConversationTurn {
            id: "conversation-turn-0003".to_string(),
            session_id: "session-1".to_string(),
            external_id: "ext-3".to_string(),
            turn_index: 2,
            // Main agent continuation turn
            prompt: "Based on subagent results, proceeding with next steps.".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            fingerprint: "fp3".to_string(),
            missing: false,
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            model: Some("gemini-2.5-pro".to_string()),
            process: None,
            response: Some("Done!".to_string()),
            turn_kind: Some("continuation".to_string()),
            agent_role: None,
            parent_turn_id: Some("conversation-turn-0001".to_string()),
        },
        ConversationTurn {
            id: "conversation-turn-0004".to_string(),
            session_id: "session-1".to_string(),
            external_id: "ext-4".to_string(),
            turn_index: 3,
            // Independent user turn
            prompt: "Now explain the final architecture.".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            fingerprint: "fp4".to_string(),
            missing: false,
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            model: Some("gemini-2.5-pro".to_string()),
            process: None,
            response: Some("Here is the architecture...".to_string()),
            turn_kind: None,
            agent_role: None,
            parent_turn_id: None,
        },
    ];

    let groups =
        group_turns_by_dynamic_projection("conversation-question", turns, |t| &t.prompt, |t| &t.id);

    assert_eq!(
        groups.len(),
        2,
        "Should group subagent and continuation turns under first question"
    );
    assert_eq!(groups[0].question_id, "conversation-question-0001");
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[0].turns.len(), 3);
    assert_eq!(groups[0].turns[0].id, "conversation-turn-0001");
    assert_eq!(groups[0].turns[1].id, "conversation-turn-0002");
    assert_eq!(groups[0].turns[2].id, "conversation-turn-0003");

    assert_eq!(groups[1].question_id, "conversation-question-0004");
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
    assert_eq!(groups[1].turns.len(), 1);
    assert_eq!(groups[1].turns[0].id, "conversation-turn-0004");
}

#[test]
fn groups_subagent_seeds_via_group_turn_ids_by_question() {
    let seeds = vec![
        ConversationTurnSeed {
            id: "t1".to_string(),
            prompt: "Build user feature".to_string(),
            turn_kind: None,
            parent_turn_id: None,
        },
        ConversationTurnSeed {
            id: "t2".to_string(),
            prompt: "Detailed subagent prompt:\n1. Check file\n2. Run tests\nAny questions?"
                .to_string(),
            turn_kind: Some("subagent".to_string()),
            parent_turn_id: Some("t1".to_string()),
        },
        ConversationTurnSeed {
            id: "t3".to_string(),
            prompt: "Second user command".to_string(),
            turn_kind: None,
            parent_turn_id: None,
        },
    ];

    let groups = group_turn_ids_by_question(seeds);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t1", "t2"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t3"]);
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
}

#[test]
fn project_question_title_prefers_user_prompt_over_subagent_delegation() {
    use crate::backend::domain::conversations::{ConversationQuestion, ConversationTurn};
    use crate::backend::store::conversations::conversation_repo::projection::project_question_title;

    let question = ConversationQuestion {
        id: "q-1".to_string(),
        session_id: "s-1".to_string(),
        title: None,
        created_at: "2026-10-28T00:00:00Z".to_string(),
        updated_at: "2026-10-28T00:00:00Z".to_string(),
    };

    let turns = vec![
        ConversationTurn {
            id: "t-sub".to_string(),
            session_id: "s-1".to_string(),
            external_id: "ext-sub".to_string(),
            turn_index: 0,
            prompt: "Delegation prompt to subagent: audit memory".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            fingerprint: "fp-sub".to_string(),
            missing: false,
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            model: None,
            process: None,
            response: None,
            turn_kind: Some("subagent".to_string()),
            agent_role: Some("auditor".to_string()),
            parent_turn_id: None,
        },
        ConversationTurn {
            id: "t-user".to_string(),
            session_id: "s-1".to_string(),
            external_id: "ext-user".to_string(),
            turn_index: 1,
            prompt: "What is the memory architecture?\nExplain in detail.".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            fingerprint: "fp-user".to_string(),
            missing: false,
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            model: None,
            process: None,
            response: None,
            turn_kind: None,
            agent_role: None,
            parent_turn_id: None,
        },
    ];

    let projected = project_question_title(question, &turns);
    assert_eq!(
        projected.title.as_deref(),
        Some("What is the memory architecture?")
    );
}

#[test]
fn adversarial_stress_test_dynamic_question_grouping_chained_subagents_and_unicode() {
    use crate::backend::domain::conversations::{ConversationQuestion, ConversationTurn};
    use crate::backend::store::conversations::conversation_repo::projection::project_question_title;

    // Phase 1: Construct 20 turns across 3 user questions
    // Q1: User turn + 12 chained subagents with complex unicode/markdown/questions + 2 continuations (15 turns)
    // Q2: User turn + 3 subagents (4 turns)
    // Q3: User turn alone (1 turn)
    let mut turns: Vec<ConversationTurn> = Vec::new();

    // Turn 0: User turn 1
    turns.push(ConversationTurn {
        id: "conversation-turn-00".to_string(),
        session_id: "sess-adv".to_string(),
        external_id: "ext-00".to_string(),
        turn_index: 0,
        prompt: "Initial user request: analyze whole codebase architecture.".to_string(),
        fingerprint: "fp-00".to_string(),
        imported_at: "2026-10-28T00:00:00Z".to_string(),
        turn_kind: None,
        agent_role: None,
        parent_turn_id: None,
        ..Default::default()
    });

    // Turns 1..=12: 12 chained subagent turns
    let tricky_prompts = vec![
        // Turn 1: code block + question mark + Chinese
        "子代理任务一：请审查 SQLite 迁移脚本。\n```rust\nfn check_migration() -> bool {\n    true\n}\n```\n是否存在冗余索引或遗漏外键约束？\n请立即排查并汇报！",
        // Turn 2: Japanese + bullet points + question marks
        "日本語タスク（サブエージェント2）：\n- 依存関係の循環をチェックしてください。\n- Cargo.toml のバージョン固定は適切ですか？\n何が问题でしょうか？",
        // Turn 3: Multi-paragraph delegation with quotes and special characters
        "Subagent 3: Inspect `AppService` facade.\nLine count > 500? \"ADR-0017\" compliance check:\n1. Is TaskRuntime used?\n2. Are locks held across await points?\nLet me know ASAP.",
        // Turn 4: Unicode symbols and math
        "Subagent 4: Calculate complexity O(N log N) vs O(N²)? Check ⚡ fast-path & ∑ metrics.",
        // Turn 5: Long markdown table and questions
        "Subagent 5: Evaluate the following matrix:\n| Adapter | Supported | Notes |\n|---|---|---|\n| Codex | Yes | JSONL |\n| Claude | Yes | Sidechain |\nAre all 5 adapters registered?",
        // Turn 6: Simple English with multiple sentences
        "Subagent 6: Run cargo clippy. Fix any unused imports. Do not touch generated files.",
        // Turn 7: Chinese with English mixed
        "Subagent 7: 检查前端 DTO 与 Rust `ConversationTurn` 字段映射是否完整？比如 `turn_kind`、`agent_role`、`parent_turn_id`。",
        // Turn 8: French / German characters
        "Subagent 8: Überprüfen Sie die Schnittstelle. Est-ce que les modèles sont synchronisés?",
        // Turn 9: Shell command script block
        "Subagent 9: Validate shell execution safety.\n```bash\ngit status --porcelain\ncargo check --workspace\n```\nDid any untracked artifacts appear?",
        // Turn 10: Deep chaining pointing to Subagent 9
        "Subagent 10 (Chained under subagent 9): Deep recursive verification of task outputs.",
        // Turn 11: Trailing punctuation and question marks
        "Subagent 11: Is everything clear??? Please verify immediately!?",
        // Turn 12: Complex multiline JSON snippet
        "Subagent 12: Ingest sample payload:\n{\n  \"action\": \"test\",\n  \"ok\": true\n}\nDoes it deserialize without panic?",
    ];

    for (idx, p) in tricky_prompts.into_iter().enumerate() {
        let turn_num = idx + 1;
        let parent_id = if turn_num == 1 {
            "conversation-turn-00".to_string()
        } else {
            // Chained to previous subagent to test hierarchical chaining
            format!("conversation-turn-{:02}", turn_num - 1)
        };
        turns.push(ConversationTurn {
            id: format!("conversation-turn-{turn_num:02}"),
            session_id: "sess-adv".to_string(),
            external_id: format!("ext-{turn_num:02}"),
            turn_index: turn_num as i64,
            prompt: p.to_string(),
            fingerprint: format!("fp-{turn_num:02}"),
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            turn_kind: Some("subagent".to_string()),
            agent_role: Some(format!("specialist-{turn_num}")),
            parent_turn_id: Some(parent_id),
            ..Default::default()
        });
    }

    // Turns 13 & 14: Main agent continuation turns
    turns.push(ConversationTurn {
        id: "conversation-turn-13".to_string(),
        session_id: "sess-adv".to_string(),
        external_id: "ext-13".to_string(),
        turn_index: 13,
        prompt: "Synthesizing subagent findings: All 12 checks completed successfully.".to_string(),
        fingerprint: "fp-13".to_string(),
        imported_at: "2026-10-28T00:00:00Z".to_string(),
        turn_kind: Some("continuation".to_string()),
        agent_role: None,
        parent_turn_id: Some("conversation-turn-00".to_string()),
        ..Default::default()
    });

    turns.push(ConversationTurn {
        id: "conversation-turn-14".to_string(),
        session_id: "sess-adv".to_string(),
        external_id: "ext-14".to_string(),
        turn_index: 14,
        prompt: "Finalizing summary for Question 1: Proceeding to next objective.".to_string(),
        fingerprint: "fp-14".to_string(),
        imported_at: "2026-10-28T00:00:00Z".to_string(),
        turn_kind: Some("main_continuation".to_string()),
        agent_role: None,
        parent_turn_id: Some("conversation-turn-00".to_string()),
        ..Default::default()
    });

    // Turn 15: Distinct user question 2
    turns.push(ConversationTurn {
        id: "conversation-turn-15".to_string(),
        session_id: "sess-adv".to_string(),
        external_id: "ext-15".to_string(),
        turn_index: 15,
        prompt: "Now implement the frontend view in React with Tailwind CSS.".to_string(),
        fingerprint: "fp-15".to_string(),
        imported_at: "2026-10-28T00:00:00Z".to_string(),
        turn_kind: None,
        agent_role: None,
        parent_turn_id: None,
        ..Default::default()
    });

    // Turns 16..=18: 3 subagents under Question 2
    for idx in 16..=18 {
        turns.push(ConversationTurn {
            id: format!("conversation-turn-{idx:02}"),
            session_id: "sess-adv".to_string(),
            external_id: format!("ext-{idx:02}"),
            turn_index: idx as i64,
            prompt: format!("Subagent {idx} delegation: Frontend component scaffolding for turn {idx}?\nCheck CSS tokens!"),
            fingerprint: format!("fp-{idx:02}"),
            imported_at: "2026-10-28T00:00:00Z".to_string(),
            turn_kind: Some("subagent".to_string()),
            agent_role: Some("frontend-builder".to_string()),
            parent_turn_id: Some("conversation-turn-15".to_string()),
            ..Default::default()
        });
    }

    // Turn 19: Distinct user question 3
    turns.push(ConversationTurn {
        id: "conversation-turn-19".to_string(),
        session_id: "sess-adv".to_string(),
        external_id: "ext-19".to_string(),
        turn_index: 19,
        prompt: "How do we run E2E verification tests?".to_string(),
        fingerprint: "fp-19".to_string(),
        imported_at: "2026-10-28T00:00:00Z".to_string(),
        turn_kind: None,
        agent_role: None,
        parent_turn_id: None,
        ..Default::default()
    });

    // Execute dynamic projection grouping
    let groups =
        group_turns_by_dynamic_projection("conversation-question", turns, |t| &t.prompt, |t| &t.id);

    // Assertions:
    // 1. Must produce EXACTLY 3 question groups
    assert_eq!(
        groups.len(),
        3,
        "Must produce exactly 3 distinct question groups"
    );

    // Group 1: 15 turns (1 user + 12 subagent + 2 continuation)
    assert_eq!(
        groups[0].turns.len(),
        15,
        "Question 1 must contain all 15 turns"
    );
    assert_eq!(groups[0].question_id, "conversation-question-00");
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    for (i, turn) in groups[0].turns.iter().enumerate() {
        assert_eq!(turn.id, format!("conversation-turn-{i:02}"));
        if i == 0 {
            assert!(!turn.is_subagent());
            assert!(!turn.is_continuation());
        } else if i <= 12 {
            assert!(turn.is_subagent(), "Turn {i} must be subagent");
        } else {
            assert!(turn.is_continuation(), "Turn {i} must be continuation");
        }
    }

    // Group 2: 4 turns (1 user + 3 subagent)
    assert_eq!(groups[1].turns.len(), 4, "Question 2 must contain 4 turns");
    assert_eq!(groups[1].question_id, "conversation-question-15");
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turns[0].id, "conversation-turn-15");
    assert!(!groups[1].turns[0].is_subagent());
    assert_eq!(groups[1].turns[1].id, "conversation-turn-16");
    assert!(groups[1].turns[1].is_subagent());
    assert_eq!(groups[1].turns[2].id, "conversation-turn-17");
    assert!(groups[1].turns[2].is_subagent());
    assert_eq!(groups[1].turns[3].id, "conversation-turn-18");
    assert!(groups[1].turns[3].is_subagent());

    // Group 3: 1 turn (1 user)
    assert_eq!(groups[2].turns.len(), 1, "Question 3 must contain 1 turn");
    assert_eq!(groups[2].question_id, "conversation-question-19");
    assert_eq!(groups[2].origin, ConversationGroupingOrigin::Imported);
    assert_eq!(groups[2].turns[0].id, "conversation-turn-19");
    assert!(!groups[2].turns[0].is_subagent());

    // 2. Question titles must select genuine user prompts, NEVER subagent delegation prompts
    let dummy_q1 = ConversationQuestion {
        id: groups[0].question_id.clone(),
        session_id: "sess-adv".to_string(),
        title: None,
        created_at: "2026-10-28T00:00:00Z".to_string(),
        updated_at: "2026-10-28T00:00:00Z".to_string(),
    };
    let projected_q1 = project_question_title(dummy_q1, &groups[0].turns);
    assert_eq!(
        projected_q1.title.as_deref(),
        Some("Initial user request: analyze whole codebase architecture.")
    );

    let dummy_q2 = ConversationQuestion {
        id: groups[1].question_id.clone(),
        session_id: "sess-adv".to_string(),
        title: None,
        created_at: "2026-10-28T00:00:00Z".to_string(),
        updated_at: "2026-10-28T00:00:00Z".to_string(),
    };
    let projected_q2 = project_question_title(dummy_q2, &groups[1].turns);
    assert_eq!(
        projected_q2.title.as_deref(),
        Some("Now implement the frontend view in React with Tailwind CSS.")
    );

    let dummy_q3 = ConversationQuestion {
        id: groups[2].question_id.clone(),
        session_id: "sess-adv".to_string(),
        title: None,
        created_at: "2026-10-28T00:00:00Z".to_string(),
        updated_at: "2026-10-28T00:00:00Z".to_string(),
    };
    let projected_q3 = project_question_title(dummy_q3, &groups[2].turns);
    assert_eq!(
        projected_q3.title.as_deref(),
        Some("How do we run E2E verification tests?")
    );
}

#[test]
fn adversarial_stress_test_group_turn_ids_seeds_hierarchical_chaining() {
    let seeds = vec![
        ConversationTurnSeed {
            id: "t0".to_string(),
            prompt: "What is the status of the release?".to_string(),
            turn_kind: None,
            parent_turn_id: None,
        },
        ConversationTurnSeed {
            id: "t1".to_string(),
            prompt: "Subagent 1: Verify git tags.\nAre tags signed?\nCheck `git tag -v`."
                .to_string(),
            turn_kind: Some("subagent".to_string()),
            parent_turn_id: Some("t0".to_string()),
        },
        ConversationTurnSeed {
            id: "t2".to_string(),
            prompt: "Subagent 2 (child of subagent 1): Check GPG keychain.\nAny expired keys?"
                .to_string(),
            turn_kind: Some("subagent".to_string()),
            parent_turn_id: Some("t1".to_string()),
        },
        ConversationTurnSeed {
            id: "t3".to_string(),
            prompt: "Subagent 3: Check CI pipeline status.\nIs build green?".to_string(),
            turn_kind: Some("agent".to_string()),
            parent_turn_id: Some("t0".to_string()),
        },
        ConversationTurnSeed {
            id: "t4".to_string(),
            prompt: "Main agent continuation: GPG and CI verified.".to_string(),
            turn_kind: Some("continuation".to_string()),
            parent_turn_id: Some("t0".to_string()),
        },
        ConversationTurnSeed {
            id: "t5".to_string(),
            prompt: "Now publish to production npm and cargo registries.".to_string(),
            turn_kind: None,
            parent_turn_id: None,
        },
        ConversationTurnSeed {
            id: "t6".to_string(),
            prompt: "Subagent under question 2: Run cargo publish --dry-run.".to_string(),
            turn_kind: Some("subagent".to_string()),
            parent_turn_id: Some("t5".to_string()),
        },
    ];

    let groups = group_turn_ids_by_question(seeds);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t0", "t1", "t2", "t3", "t4"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t5", "t6"]);
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::AutoMerged);
}
