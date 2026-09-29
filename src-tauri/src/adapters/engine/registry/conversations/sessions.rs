//! Engine 命令注册表：Conversations / Sessions

use super::super::dispatch::*;
use super::super::types::*;
use crate::backend::application::AppService;
use crate::{command, param};
use serde_json::{json, Value};

pub(super) const COMMANDS: &[CommandSpec] = &[
    command!(
        "conversation.sync",
        "conversation.sync",
        "Synchronize conversation sources",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSyncParams,
        ServiceAsync => |service, params| service.sync_conversations(params).await,
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("dry_run", "Preview without importing", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation sync")
    ),
    command!(
        "conversation.data.audit",
        "conversation.data.audit",
        "Audit conversation membership, source facts, and search index consistency",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationDataAuditParams,
        ServiceAsync => |service, params| service.audit_conversation_data(params).await,
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("record_kind", "Optional conversation record kind", ["recordKind"]),
            param!("include_resolved", "Include resolved audit records", ["includeResolved"]),
        ],
        Some("assetiweave-cli conversation data audit")
    ),
    command!(
        "conversation.data.repair",
        "conversation.data.repair",
        "Repair safe conversation data issues and rebuild conversation search",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationDataRepairParams,
        ServiceAsync => |service, params| service.repair_conversation_data(params).await,
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("record_kind", "Optional conversation record kind", ["recordKind"]),
            param!("dry_run", "Preview repairs without writing", ["dryRun"]),
            param!("yes", "Confirm applying repairs"),
            param!("resync", "Run a full source resync before applying safe repairs"),
        ],
        Some("assetiweave-cli conversation data repair")
    ),
    command!(
        "conversation.data.rollback",
        "conversation.data.rollback",
        "Restore the database from a conversation maintenance backup; restart afterward",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationDataRollbackParams,
        ServiceAsync => |service, params| service.rollback_conversation_data(params).await,
        &[
            param!("backup_path", "Verified database backup path", ["backupPath"]),
            param!("dry_run", "Preview rollback without replacing the database", ["dryRun"]),
            param!("yes", "Confirm restoring the backup"),
        ],
        Some("assetiweave-cli conversation data rollback")
    ),
    command!(
        "conversation.session.list",
        "conversation.session.list",
        "List imported conversation sessions",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionListParams,
        ServiceAsync => |service, params| service.list_conversation_sessions(params).await,
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of sessions"),
            param!("offset", "Pagination offset"),
        ],
        Some("assetiweave-cli conversation session list")
    ),
    command!(
        "conversation.command_projection.project",
        "conversation.command_projection.project",
        "Project one stored command block into display command parts",
        Read,
        Friendly,
        false,
        crate::backend::infrastructure::conversations::ConversationCommandProjectionParams,
        ServiceAsync => |service, params| service.project_conversation_command_parts(params).await,
        &[
            param!("adapter_id", "Source conversation adapter identifier", ["adapterId"]),
            param!("parts", "Stored raw command blocks to project"),
        ],
        None
    ),
    command!(
        "conversation.session.get",
        "conversation.session.get",
        "Get one conversation session with question groups",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionGetParams,
        ServiceAsync => |service, params| service.get_conversation_session(params).await,
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("roles", "Filter cards by roles or kinds", ["roles"]),
        ],
        Some("assetiweave-cli conversation session get <session-id>")
    ),
    command!(
        "conversation.session.outline",
        "conversation.session.outline",
        "Get compact conversation session outline tree with consecutive card run folding",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionOutlineParams,
        ServiceAsync => |service, params| service.get_conversation_session_outline(params).await,
        &[param!("id", "Session, question, turn, or card identifier", ["anyId"])],
        Some("assetiweave-cli conversation session outline <any-id>")
    ),
    command!(
        "conversation.session.export",
        "conversation.session.export",
        "Export one conversation session as rendered Markdown or raw JSON facts",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSessionExportParams,
        ServiceAsync => |service, params| service.export_conversation_session(params).await,
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full session",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!(
                "format",
                "Export representation: rendered Markdown or raw JSON facts",
                ["exportFormat"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation session export <session-id> --output-root <dir>")
    ),
    command!(
        "conversation.web-record.list",
        "conversation.web-record.list",
        "List imported web conversation records",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionListParams,
        ServiceAsync => |service, params| service.list_web_record_sessions(params).await,
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of web records"),
            param!("offset", "Pagination offset"),
        ],
        Some("assetiweave-cli conversation web-record list")
    ),
    command!(
        "conversation.web-record.get",
        "conversation.web-record.get",
        "Get one web conversation record with question groups",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionGetParams,
        ServiceAsync => |service, params| service.get_web_record_session(params).await,
        &[param!("session_id", "Web record identifier", ["sessionId"])],
        Some("assetiweave-cli conversation web-record get <record-id>")
    ),
    command!(
        "conversation.web-record.export",
        "conversation.web-record.export",
        "Export one web conversation record as rendered Markdown or raw JSON facts",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSessionExportParams,
        ServiceAsync => |service, params| service.export_web_record_session(params).await,
        &[
            param!("session_id", "Web record identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full record",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!(
                "format",
                "Export representation: rendered Markdown or raw JSON facts",
                ["exportFormat"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation web-record export <record-id> --output-root <dir>")
    ),
    command!(
        "conversation.question.list",
        "conversation.question.list",
        "List question groups in a conversation session",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationQuestionListParams,
        ServiceAsync => |service, params| service.list_conversation_questions(params).await,
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of questions"),
            param!("offset", "Pagination offset"),
        ],
        Some("assetiweave-cli conversation question list <session-id>")
    ),
    command!(
        "conversation.question.get",
        "conversation.question.get",
        "Get one conversation question group",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationQuestionGetParams,
        ServiceAsync => |service, params| service.get_conversation_question(params).await,
        &[param!("question_id", "Question identifier", ["questionId"])],
        Some("assetiweave-cli conversation question get <question-id>")
    ),
    command!(
        "conversation.block.list",
        "conversation.block.list",
        "List content block locators for one question without reading block content",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationBlockListParams,
        ServiceAsync => |service, params| service.list_conversation_blocks(params).await,
        &[param!("question_id", "Question identifier", ["questionId"])],
        Some("assetiweave-cli conversation block list <question-id>")
    ),
    command!(
        "conversation.block.get",
        "conversation.block.get",
        "Get exact content for one conversation block",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationBlockGetParams,
        ServiceAsync => |service, params| service.get_conversation_block(params).await,
        &[param!("block_id", "Block identifier", ["blockId"])],
        Some("assetiweave-cli conversation block get <block-id>")
    ),
    command!(
        "conversation.question.merge",
        "conversation.question.merge",
        "Merge adjacent conversation question groups",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationQuestionMergeParams,
        ServiceAsync => |service, params| service.merge_conversation_questions(params).await,
        &[
            param!("question_ids", "Adjacent question identifiers in session order", ["questionIds"]),
            param!("dry_run", "Preview without merging", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation question merge <question-id>...")
    ),
    command!(
        "conversation.question.split",
        "conversation.question.split",
        "Split a conversation question group before a turn",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationQuestionSplitParams,
        ServiceAsync => |service, params| service.split_conversation_question(params).await,
        &[
            param!("question_id", "Question identifier", ["questionId"]),
            param!("before_turn_id", "Turn identifier that starts the new question", ["beforeTurnId"]),
            param!("dry_run", "Preview without splitting", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation question split <question-id> --before-turn <turn-id>")
    ),
    command!(
        "translate_conversation_card",
        "conversation.card.translation.run",
        "Translate a conversation content card with the configured translation provider",
        Write,
        App,
        false,
        crate::backend::application::conversations::card_translation::ConversationTranslationRequest,
        ServiceAsync => |service, params| service.translate_conversation_card(params).await,
        &[
            param!("provider", "Translation provider family"),
            param!("cli", "CLI translator when provider is cli"),
            param!("model", "Optional model identifier"),
            param!("prompt", "Rendered translation prompt")
        ],
        None
    ),
    command!(
        "sync_conversations",
        "conversation.sync",
        "Synchronize conversation sources",
        Write,
        App,
        false,
        crate::backend::application::ConversationSyncParams,
        ServiceAsync => |service, params| service.sync_conversations(params).await,
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("dry_run", "Preview without importing", ["dryRun"]),
        ],
        None
    ),
    command!(
        "audit_conversation_data",
        "conversation.data.audit",
        "Audit conversation membership, source facts, and search index consistency",
        Read,
        App,
        false,
        crate::backend::application::ConversationDataAuditParams,
        ServiceAsync => |service, params| service.audit_conversation_data(params).await,
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("record_kind", "Optional conversation record kind", ["recordKind"]),
            param!("include_resolved", "Include resolved audit records", ["includeResolved"]),
        ],
        None
    ),
    command!(
        "repair_conversation_data",
        "conversation.data.repair",
        "Repair safe conversation data issues and rebuild conversation search",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationDataRepairParams,
        ServiceAsync => |service, params| service.repair_conversation_data(params).await,
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("record_kind", "Optional conversation record kind", ["recordKind"]),
            param!("dry_run", "Preview repairs without writing", ["dryRun"]),
            param!("yes", "Confirm applying repairs"),
            param!("resync", "Run a full source resync before applying safe repairs"),
        ],
        None
    ),
    command!(
        "rollback_conversation_data",
        "conversation.data.rollback",
        "Restore the database from a conversation maintenance backup; restart afterward",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationDataRollbackParams,
        ServiceAsync => |service, params| service.rollback_conversation_data(params).await,
        &[
            param!("backup_path", "Verified database backup path", ["backupPath"]),
            param!("dry_run", "Preview rollback without replacing the database", ["dryRun"]),
            param!("yes", "Confirm restoring the backup"),
        ],
        None
    ),
    command!(
        "list_conversation_sessions",
        "conversation.session.list",
        "List imported conversation sessions",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionListParams,
        ServiceAsync => |service, params| service.list_conversation_sessions(params).await,
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of sessions"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "project_conversation_command_parts",
        "conversation.command_projection.project",
        "Project one stored command block into display command parts",
        Read,
        App,
        false,
        crate::backend::infrastructure::conversations::ConversationCommandProjectionParams,
        ServiceAsync => |service, params| service.project_conversation_command_parts(params).await,
        &[
            param!("adapter_id", "Source conversation adapter identifier", ["adapterId"]),
            param!("parts", "Stored raw command blocks to project"),
        ],
        None
    ),
    command!(
        "get_conversation_session",
        "conversation.session.get",
        "Get one conversation session with question groups",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionGetParams,
        ServiceAsync => |service, params| service.get_conversation_session(params).await,
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("roles", "Filter cards by roles or kinds", ["roles"]),
        ],
        None
    ),
    command!(
        "get_conversation_session_outline",
        "conversation.session.outline",
        "Get compact conversation session outline tree with consecutive card run folding",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionOutlineParams,
        ServiceAsync => |service, params| service.get_conversation_session_outline(params).await,
        &[param!("id", "Session, question, turn, or card identifier", ["anyId"])],
        None
    ),
    command!(
        "export_conversation_session",
        "conversation.session.export",
        "Export one conversation session as rendered Markdown or raw JSON facts",
        Write,
        App,
        false,
        crate::backend::application::ConversationSessionExportParams,
        ServiceAsync => |service, params| service.export_conversation_session(params).await,
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full session",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!(
                "format",
                "Export representation: rendered Markdown or raw JSON facts",
                ["exportFormat"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        None
    ),
    command!(
        "list_web_record_sessions",
        "conversation.web-record.list",
        "List imported web conversation records",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionListParams,
        ServiceAsync => |service, params| service.list_web_record_sessions(params).await,
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of web records"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "get_web_record_session",
        "conversation.web-record.get",
        "Get one web conversation record with question groups",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionGetParams,
        ServiceAsync => |service, params| service.get_web_record_session(params).await,
        &[param!("session_id", "Web record identifier", ["sessionId"])],
        None
    ),
    command!(
        "export_web_record_session",
        "conversation.web-record.export",
        "Export one web conversation record as rendered Markdown or raw JSON facts",
        Write,
        App,
        false,
        crate::backend::application::ConversationSessionExportParams,
        ServiceAsync => |service, params| service.export_web_record_session(params).await,
        &[
            param!("session_id", "Web record identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full record",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!(
                "format",
                "Export representation: rendered Markdown or raw JSON facts",
                ["exportFormat"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        None
    ),
    command!(
        "list_conversation_questions",
        "conversation.question.list",
        "List question groups in a conversation session",
        Read,
        App,
        false,
        crate::backend::application::ConversationQuestionListParams,
        ServiceAsync => |service, params| service.list_conversation_questions(params).await,
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of questions"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "get_conversation_question",
        "conversation.question.get",
        "Get one conversation question group",
        Read,
        App,
        false,
        crate::backend::application::ConversationQuestionGetParams,
        ServiceAsync => |service, params| service.get_conversation_question(params).await,
        &[param!("question_id", "Question identifier", ["questionId"])],
        None
    ),
    command!(
        "list_conversation_blocks",
        "conversation.block.list",
        "List content block locators for one question without reading block content",
        Read,
        App,
        false,
        crate::backend::application::ConversationBlockListParams,
        ServiceAsync => |service, params| service.list_conversation_blocks(params).await,
        &[param!("question_id", "Question identifier", ["questionId"])],
        None
    ),
    command!(
        "get_conversation_block",
        "conversation.block.get",
        "Get exact content for one conversation block",
        Read,
        App,
        false,
        crate::backend::application::ConversationBlockGetParams,
        ServiceAsync => |service, params| service.get_conversation_block(params).await,
        &[param!("block_id", "Block identifier", ["blockId"])],
        None
    ),
    command!(
        "merge_conversation_questions",
        "conversation.question.merge",
        "Merge adjacent conversation question groups",
        Write,
        App,
        false,
        crate::backend::application::ConversationQuestionMergeParams,
        ServiceAsync => |service, params| service.merge_conversation_questions(params).await,
        &[
            param!("question_ids", "Adjacent question identifiers in session order", ["questionIds"]),
            param!("dry_run", "Preview without merging", ["dryRun"]),
        ],
        None
    ),
    command!(
        "split_conversation_question",
        "conversation.question.split",
        "Split a conversation question group before a turn",
        Write,
        App,
        false,
        crate::backend::application::ConversationQuestionSplitParams,
        ServiceAsync => |service, params| service.split_conversation_question(params).await,
        &[
            param!("question_id", "Question identifier", ["questionId"]),
            param!("before_turn_id", "Turn identifier that starts the new question", ["beforeTurnId"]),
            param!("dry_run", "Preview without splitting", ["dryRun"]),
        ],
        None
    ),
];
