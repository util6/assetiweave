use super::render_ok_json;
use crate::args::OutputFormat;
use crate::view_models::ConversationSessionViewModel;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};

pub fn render_conversation_search(
    res: &asset_core::application::ConversationSearchResult,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            render_ok_json(res);
        }
        OutputFormat::Table => {
            println!(
                "Search query: \"{}\" | Matches: {} (Backend: {})",
                res.query, res.total_count, res.backend
            );
            if res.hits.is_empty() {
                println!("No conversation hits found.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Session Title").fg(Color::Cyan),
                Cell::new("Adapter").fg(Color::Cyan),
                Cell::new("Type").fg(Color::Cyan),
                Cell::new("Snippet").fg(Color::Cyan),
            ]);
            for hit in &res.hits {
                let clean_snippet = hit.snippet.trim().replace('\n', " ");
                let truncated = if clean_snippet.chars().count() > 80 {
                    format!("{}...", clean_snippet.chars().take(80).collect::<String>())
                } else {
                    clean_snippet
                };
                table.add_row(vec![
                    Cell::new(&hit.session.session.title),
                    Cell::new(&hit.session.session.adapter_id),
                    Cell::new(format!("{:?}", hit.card_type)),
                    Cell::new(truncated),
                ]);
            }
            println!("{table}");
            println!("\nShowing {} hits", res.hits.len());
        }
    }
}

fn format_prompt_snippet(p: &str, max_len: usize) -> String {
    let single_line = p.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = single_line.chars().collect();
    if chars.len() > max_len {
        format!(
            "\"{}...\"",
            chars[..max_len.saturating_sub(3)]
                .iter()
                .collect::<String>()
        )
    } else {
        format!("\"{single_line}\"")
    }
}

pub fn render_conversation_sessions(
    sessions: &[asset_core::domain::conversations::ConversationSessionListItem],
    format: OutputFormat,
    window_expr: Option<&str>,
) {
    let now = chrono::Utc::now();
    let view_models: Vec<ConversationSessionViewModel> = sessions
        .iter()
        .map(|s| ConversationSessionViewModel::from_item(s, now))
        .collect();

    match format {
        OutputFormat::Json => {
            render_ok_json(&view_models);
        }
        OutputFormat::Table => {
            if view_models.is_empty() {
                println!("No conversation sessions found.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);

            let prompt_header = if window_expr.is_some() {
                "PROMPT (ENTRY -> LATEST IN WINDOW)"
            } else {
                "LATEST USER PROMPT"
            };
            let turns_header = if window_expr.is_some() {
                "WINDOW SPAN"
            } else {
                "TURNS"
            };

            table.set_header(vec![
                Cell::new("SHORT ID").fg(Color::Cyan),
                Cell::new("UPDATED").fg(Color::Cyan),
                Cell::new(prompt_header).fg(Color::Cyan),
                Cell::new(turns_header).fg(Color::Cyan),
                Cell::new("PROJECT").fg(Color::Cyan),
                Cell::new("ADAPTER").fg(Color::Cyan),
            ]);

            for vm in &view_models {
                let prompt_display = if window_expr.is_some() {
                    match (vm.window_start_turn_index, vm.window_end_turn_index) {
                        (Some(start), Some(end)) if start != end => {
                            let start_p = vm
                                .window_start_user_prompt
                                .as_deref()
                                .map(|p| format_prompt_snippet(p, 50))
                                .unwrap_or_else(|| "—".to_string());
                            let end_p = vm
                                .window_end_user_prompt
                                .as_deref()
                                .map(|p| format_prompt_snippet(p, 50))
                                .unwrap_or_else(|| "—".to_string());
                            format!("[T{start}] {start_p}\n[T{end}] {end_p}")
                        }
                        (Some(start), _) => {
                            let p_txt = vm
                                .window_start_user_prompt
                                .as_deref()
                                .or(vm.window_end_user_prompt.as_deref())
                                .or(vm.latest_user_prompt.as_deref())
                                .map(|p| format_prompt_snippet(p, 55))
                                .unwrap_or_else(|| "—".to_string());
                            format!("[T{start}] {p_txt}")
                        }
                        _ => vm
                            .latest_user_prompt
                            .as_deref()
                            .map(|p| format_prompt_snippet(p, 45))
                            .unwrap_or_else(|| "—".to_string()),
                    }
                } else {
                    vm.latest_user_prompt
                        .as_deref()
                        .map(|p| format_prompt_snippet(p, 45))
                        .unwrap_or_else(|| "—".to_string())
                };

                let turns_display = if window_expr.is_some() {
                    match (vm.window_start_turn_index, vm.window_end_turn_index) {
                        (Some(start), Some(end)) if start != end => {
                            let w_count = vm.window_turn_count.unwrap_or(0);
                            format!("T{start}..{end} (+{w_count})\n(total {})", vm.turn_count)
                        }
                        (Some(start), _) => {
                            format!("T{start} (1 turn)\n(total {})", vm.turn_count)
                        }
                        _ => {
                            let w_count = vm.window_turn_count.unwrap_or(0);
                            if w_count > 0 {
                                format!("+{w_count} / {} turns", vm.turn_count)
                            } else {
                                format!("0 / {} turns (inactive)", vm.turn_count)
                            }
                        }
                    }
                } else if vm.turn_count == 1 {
                    "1 turn".to_string()
                } else {
                    format!("{} turns", vm.turn_count)
                };

                table.add_row(vec![
                    Cell::new(&vm.short_id),
                    Cell::new(&vm.updated_at_human),
                    Cell::new(prompt_display),
                    Cell::new(turns_display),
                    Cell::new(&vm.project_name),
                    Cell::new(&vm.adapter_id),
                ]);
            }
            println!("{table}");
            if let Some(desc) = window_expr {
                println!("\nShowing {} active sessions {}", view_models.len(), desc);
            } else {
                println!(
                    "\nShowing latest {} sessions (sorted by recent activity)",
                    view_models.len()
                );
                println!("Hint: use --window 24h to see delta activity, or aiwc conversation outline <SHORT_ID> to inspect");
            }
        }
    }
}

pub fn render_conversation_adapters(
    adapters: &[asset_core::domain::conversations::ConversationAdapter],
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            render_ok_json(adapters);
        }
        OutputFormat::Table => {
            if adapters.is_empty() {
                println!("No conversation adapters installed.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Adapter ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Version").fg(Color::Cyan),
                Cell::new("Kind").fg(Color::Cyan),
            ]);
            for a in adapters {
                table.add_row(vec![
                    Cell::new(&a.id),
                    Cell::new(&a.name),
                    Cell::new(&a.version),
                    Cell::new(format!("{:?}", a.kind)),
                ]);
            }
            println!("{table}");
            println!("\nTotal: {} adapters", adapters.len());
        }
    }
}
