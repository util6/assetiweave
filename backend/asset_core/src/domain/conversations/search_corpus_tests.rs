use super::*;
use crate::backend::domain::{ConversationContentCardDescriptor, ConversationPartRole};

fn part(kind: ConversationPartKind) -> ConversationPart {
    ConversationPart {
        id: format!("part-{}", kind.as_str()),
        turn_id: "turn-1".to_string(),
        part_index: 0,
        role: ConversationPartRole::Tool,
        kind,
        text: None,
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: None,
        translated_text: None,
        signal: None,
        tool_name: None,
        file_path: None,
    }
}

fn turn_facts<'a>(
    prompt: &'a str,
    response: Option<&'a str>,
    parts: &'a [ConversationPart],
) -> CorpusTurnFacts<'a> {
    CorpusTurnFacts {
        turn_id: "turn-1",
        prompt,
        response,
        parts,
    }
}

fn texts_for(documents: &[SearchCorpusDocument], signal: SearchCorpusSignal) -> Vec<String> {
    documents
        .iter()
        .filter(|document| document.signal == signal)
        .map(|document| document.text.clone())
        .collect()
}

#[test]
fn extracts_prompt_and_answer() {
    let policy = HighSignalCorpusPolicy;
    let documents = policy.extract(&turn_facts(
        "  如何挂载 Skill？  ",
        Some(" 在资产页挂载 "),
        &[],
    ));

    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::Prompt),
        vec!["如何挂载 Skill？".to_string()]
    );
    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::Answer),
        vec!["在资产页挂载".to_string()]
    );
}

#[test]
fn omits_blank_prompt_and_answer() {
    let policy = HighSignalCorpusPolicy;
    let documents = policy.extract(&turn_facts("   ", Some("   "), &[]));

    assert!(documents.is_empty(), "空白信号不应产生索引行");
}

#[test]
fn excludes_tool_output_and_reasoning_bodies() {
    let policy = HighSignalCorpusPolicy;
    let mut command = part(ConversationPartKind::Command);
    command.command = Some("cargo build --release 2>&1".to_string());
    command.text = Some("Compiling 431 crates...".to_string());
    let mut result = part(ConversationPartKind::Tool);
    result.text = Some("diff --git a/x b/x\n@@ -1 +1 @@".to_string());
    let mut reasoning = part(ConversationPartKind::Text);
    reasoning.text = Some("先看看索引有多大".to_string());
    let parts = vec![command, result, reasoning];

    let documents = policy.extract(&turn_facts("提问", Some("回答"), &parts));

    assert_eq!(documents.len(), 2, "只有提问与回答允许进入语料");
    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::CommandLabel).len(),
        0
    );
    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::ChangedFile).len(),
        0
    );
}

#[test]
fn collects_changed_file_paths_from_mutation_parts_only() {
    let policy = HighSignalCorpusPolicy;
    let mut edited = part(ConversationPartKind::Tool);
    edited.file_path = Some("~/project/src/lib.rs".to_string());
    edited.command_label = Some("Edit".to_string());
    let mut written = part(ConversationPartKind::Tool);
    written.file_path = Some("~/project/README.md".to_string());
    written.command_label = Some("apply_patch".to_string());
    // 只读访问不应被当成"变更文件"。
    let mut viewed = part(ConversationPartKind::Tool);
    viewed.file_path = Some("~/project/secret.env".to_string());
    viewed.command_label = Some("Read".to_string());
    let parts = vec![edited, written, viewed];

    let documents = policy.extract(&turn_facts("提问", None, &parts));

    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::ChangedFile),
        vec!["~/project/src/lib.rs\n~/project/README.md".to_string()]
    );
}

#[test]
fn collects_explicit_file_change_parts() {
    let policy = HighSignalCorpusPolicy;
    let mut change = part(ConversationPartKind::FileChange);
    change.file_path = Some("~/project/src/main.rs".to_string());
    let parts = vec![change];

    let documents = policy.extract(&turn_facts("提问", None, &parts));

    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::ChangedFile),
        vec!["~/project/src/main.rs".to_string()]
    );
}

#[test]
fn collects_command_labels_without_raw_commands() {
    let policy = HighSignalCorpusPolicy;
    let mut build = part(ConversationPartKind::Command);
    build.command = Some("pnpm build".to_string());
    build.command_label = Some("编译构建".to_string());
    let mut test = part(ConversationPartKind::Command);
    test.command = Some("pnpm test".to_string());
    test.command_label = Some("单元测试".to_string());
    let mut duplicated = part(ConversationPartKind::Command);
    duplicated.command_label = Some("编译构建".to_string());
    let parts = vec![build, test, duplicated];

    let documents = policy.extract(&turn_facts("提问", None, &parts));

    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::CommandLabel),
        vec!["编译构建\n单元测试".to_string()]
    );
    assert!(!documents
        .iter()
        .any(|document| document.text.contains("pnpm")));
}

#[test]
fn omits_command_signal_when_no_semantic_label_exists() {
    let policy = HighSignalCorpusPolicy;
    let mut raw = part(ConversationPartKind::Command);
    raw.command = Some("ls -la".to_string());
    raw.text = Some("total 24".to_string());
    let parts = vec![raw];

    let documents = policy.extract(&turn_facts("提问", Some("回答"), &parts));

    // 拿不到可靠语义时宁可缺席，也不能把原始命令或终端输出当替代品入库。
    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::CommandLabel).len(),
        0
    );
}

#[test]
fn document_anchor_is_stable_per_signal() {
    let policy = HighSignalCorpusPolicy;
    let documents = policy.extract(&turn_facts("提问", Some("回答"), &[]));

    assert_eq!(documents[0].anchor_id(), "turn-1:prompt");
    assert_eq!(documents[1].anchor_id(), "turn-1:answer");
}

#[test]
fn policy_identity_is_versioned() {
    let policy = HighSignalCorpusPolicy;

    assert_eq!(policy.version(), SEARCH_CORPUS_POLICY_VERSION);
    assert_eq!(policy.identity(), "high-signal@1");
    assert_eq!(search_corpus_policy_identity(), "high-signal@1");
    assert_eq!(current_search_corpus_policy().identity(), "high-signal@1");
}

#[test]
fn content_card_kind_suffix_drives_file_change_detection() {
    let policy = HighSignalCorpusPolicy;
    let mut carded = part(ConversationPartKind::Tool);
    carded.file_path = Some("~/project/src/app.tsx".to_string());
    carded.content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "claude-code.file-change".to_string(),
        renderer: None,
        semantic_role: None,
    });
    let parts = vec![carded];

    let documents = policy.extract(&turn_facts("提问", None, &parts));

    assert_eq!(
        texts_for(&documents, SearchCorpusSignal::ChangedFile),
        vec!["~/project/src/app.tsx".to_string()]
    );
}
