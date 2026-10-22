use crate::args::conversation::{
    ConversationAdapterCommands, ConversationArgs, ConversationCardCommands, ConversationCommands,
    ConversationDataCommands, ConversationSourceCommands,
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
            let (since, until, window_desc) = if !l.time.is_empty() {
                let time_strs: Vec<&str> = l.time.iter().map(String::as_str).collect();
                let (s, u, desc) = asset_core::application::resolve_unified_time(&time_strs)?;
                (s, u, desc)
            } else {
                let desc = if let Some(r) = l.range.as_deref() {
                    Some(format!("in range {r}"))
                } else if let (Some(s), Some(u)) = (l.since.as_deref(), l.until.as_deref()) {
                    Some(format!("between {s} and {u}"))
                } else if let Some(s) = l.since.as_deref() {
                    if s.contains("..") || s.contains(" - ") || s.contains(" to ") {
                        Some(format!("in range {s}"))
                    } else {
                        Some(format!("since {s}"))
                    }
                } else if let Some(u) = l.until.as_deref() {
                    Some(format!("until {u}"))
                } else {
                    l.window.as_deref().map(|w| format!("in past {w}"))
                };
                (l.since.clone(), l.until.clone(), desc)
            };
            let mut list_args = l;
            if since.is_some() || until.is_some() {
                list_args.since = since;
                list_args.until = until;
            }
            let sessions = service.list_conversation_sessions(&list_args).await?;
            render::render_conversation_sessions(&sessions, format, window_desc.as_deref());
        }
        ConversationCommands::Outline(o) => {
            let outline = service.conversation().get_outline(o.id).await?;
            render::render_json_value(&outline);
        }
        ConversationCommands::Card(c) => match c.command {
            ConversationCardCommands::Get(g) => {
                let card = service.conversation().get_card(g.card_id).await?;
                if g.raw {
                    if card.content.ends_with('\n') {
                        print!("{}", card.content);
                    } else {
                        println!("{}", card.content);
                    }
                } else {
                    render::render_ok_json(&card);
                }
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
    }
    Ok(())
}
