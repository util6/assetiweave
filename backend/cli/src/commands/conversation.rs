use crate::args::conversation::{
    ConversationAdapterCommands, ConversationArgs, ConversationBlockCommands, ConversationCommands,
    ConversationDataCommands, ConversationPartCommands, ConversationPartTranslationCommands,
    ConversationQuestionCommands, ConversationSessionCommands, ConversationSourceCommands,
};
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::Result;

pub async fn handle_conversation(args: ConversationArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        ConversationCommands::Search(s) => {
            let query = s.query.unwrap_or_default();
            let res = service
                .search_conversations(
                    query, s.kinds, s.limit, s.offset, s.since, s.until, s.timeline,
                )
                .await?;
            render::render_conversation_search(&res, format);
        }
        ConversationCommands::List(l) => {
            let sessions = service
                .list_conversation_sessions(l.limit, l.offset)
                .await?;
            render::render_conversation_sessions(&sessions, format);
        }
        ConversationCommands::Session(s) => match s.command {
            ConversationSessionCommands::Get(g) => {
                let session = service.get_conversation_session(g.session_id).await?;
                render::render_json_value(&session);
            }
            ConversationSessionCommands::Outline(o) => {
                let outline = service.conversation().get_session_outline(o.id).await?;
                render::render_json_value(&outline);
            }
        },
        ConversationCommands::Sync(s) => {
            let res = service
                .sync_conversations(s.adapter, s.source, s.force)
                .await?;
            render::render_json_value(&res);
        }
        ConversationCommands::Adapter(a) => match a.command {
            ConversationAdapterCommands::List => {
                let adapters = service.list_conversation_adapters()?;
                render::render_conversation_adapters(&adapters, format);
            }
            ConversationAdapterCommands::Inspect(i) => {
                let res = service.conversation().inspect_adapter(i.adapter_id).await?;
                render::render_json_value(&res);
            }
        },
        ConversationCommands::Source(s) => match s.command {
            ConversationSourceCommands::List => {
                let res = service.conversation().list_sources().await?;
                render::render_json_value(&res);
            }
            ConversationSourceCommands::Sync(sc) => {
                let res = service
                    .conversation()
                    .sync_source(sc.source_id, sc.full)
                    .await?;
                render::render_json_value(&res);
            }
        },
        ConversationCommands::Data(d) => match d.command {
            ConversationDataCommands::Audit(a) => {
                let res = service
                    .conversation()
                    .audit_data(a.source, a.record_kind)
                    .await?;
                render::render_json_value(&res);
            }
            ConversationDataCommands::Repair(r) => {
                let res = service
                    .conversation()
                    .repair_data(r.source, r.record_kind, r.dry_run, r.resync, r.yes)
                    .await?;
                render::render_json_value(&res);
            }
            ConversationDataCommands::Rollback(rb) => {
                let res = service
                    .conversation()
                    .rollback_data(rb.backup_path, rb.dry_run, rb.yes)
                    .await?;
                render::render_json_value(&res);
            }
        },
        ConversationCommands::Question(q) => match q.command {
            ConversationQuestionCommands::List(l) => {
                let res = service
                    .conversation()
                    .list_questions(l.session_id, l.limit, l.offset)
                    .await?;
                render::render_json_value(&res);
            }
            ConversationQuestionCommands::Get(g) => {
                let res = service.conversation().get_question(g.question_id).await?;
                render::render_json_value(&res);
            }
            ConversationQuestionCommands::Merge(m) => {
                let res = service
                    .conversation()
                    .merge_question(m.question_id, m.dry_run)
                    .await?;
                render::render_json_value(&res);
            }
            ConversationQuestionCommands::Split(sp) => {
                let res = service
                    .conversation()
                    .split_question(sp.question_id, sp.before_turn, sp.dry_run)
                    .await?;
                render::render_json_value(&res);
            }
        },
        ConversationCommands::Block(b) => match b.command {
            ConversationBlockCommands::List(l) => {
                let res = service.conversation().list_blocks(l.question_id).await?;
                render::render_json_value(&res);
            }
            ConversationBlockCommands::Get(g) => {
                let res = service.conversation().get_block(g.block_id).await?;
                render::render_json_value(&res);
            }
        },
        ConversationCommands::Part(p) => match p.command {
            ConversationPartCommands::Translation(t) => match t.command {
                ConversationPartTranslationCommands::Update(u) => {
                    let res = service
                        .conversation()
                        .update_part_translation(u.part_id, u.record_kind, u.text)
                        .await?;
                    render::render_json_value(&res);
                }
            },
        },
    }
    Ok(())
}
