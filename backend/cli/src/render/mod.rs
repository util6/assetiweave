use crate::args::OutputFormat;
use crate::view_models::{OverviewViewModel, VersionViewModel};
use asset_core::domain::{CatalogAsset, Source, TargetProfileDescriptor, Tenant};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

pub fn render_ok_json<T: Serialize + ?Sized>(val: &T) {
    let envelope = serde_json::json!({
        "ok": true,
        "data": val
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&envelope).unwrap_or_default()
    );
}

pub fn render_json_value(val: &serde_json::Value) {
    render_ok_json(val);
}

pub fn render_generic<T: Serialize + ?Sized>(val: &T, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(val);
        }
        OutputFormat::Table => {
            println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
        }
    }
}

pub fn render_version(model: &VersionViewModel, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(model);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Component").fg(Color::Cyan),
                Cell::new("Version / Status").fg(Color::Cyan),
            ]);
            table.add_row(vec![
                Cell::new("CLI Native Binary"),
                Cell::new(&model.cli_version),
            ]);
            table.add_row(vec![
                Cell::new("Backend Core"),
                Cell::new(&model.engine_version),
            ]);
            table.add_row(vec![
                Cell::new("Runtime Compatibility"),
                if model.compatible {
                    Cell::new("OK (In-Process)").fg(Color::Green)
                } else {
                    Cell::new("Incompatible").fg(Color::Red)
                },
            ]);
            println!("{table}");
        }
    }
}

pub fn render_overview(model: &OverviewViewModel, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(model);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Metric").fg(Color::Cyan),
                Cell::new("Count / State").fg(Color::Cyan),
            ]);
            table.add_row(vec![
                Cell::new("Registered Sources"),
                Cell::new(model.source_count.to_string()),
            ]);
            table.add_row(vec![
                Cell::new("Indexed Assets"),
                Cell::new(model.asset_count.to_string()),
            ]);
            table.add_row(vec![
                Cell::new("Target Profiles"),
                Cell::new(model.profile_count.to_string()),
            ]);
            table.add_row(vec![
                Cell::new("Last Scan Status"),
                Cell::new(&model.last_scan_status),
            ]);
            println!("{table}");
        }
    }
}

pub fn render_skills(skills: &[CatalogAsset], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(skills);
        }
        OutputFormat::Table => {
            if skills.is_empty() {
                println!("No skills found in the catalog.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("Source").fg(Color::Cyan),
                Cell::new("Display Path").fg(Color::Cyan),
            ]);
            for item in skills {
                table.add_row(vec![
                    Cell::new(&item.asset.name),
                    Cell::new(&item.asset.id),
                    Cell::new(&item.asset.source_id),
                    Cell::new(&item.display_path),
                ]);
            }
            println!("{table}");
            println!("\nTotal: {} skills", skills.len());
        }
    }
}

pub fn render_assets(assets: &[CatalogAsset], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(assets);
        }
        OutputFormat::Table => {
            if assets.is_empty() {
                println!("No assets found.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Kind").fg(Color::Cyan),
                Cell::new("Source ID").fg(Color::Cyan),
                Cell::new("Display Path").fg(Color::Cyan),
            ]);
            for item in assets {
                table.add_row(vec![
                    Cell::new(&item.asset.name),
                    Cell::new(format!("{:?}", item.asset.kind)),
                    Cell::new(&item.asset.source_id),
                    Cell::new(&item.display_path),
                ]);
            }
            println!("{table}");
            println!("\nTotal: {} assets", assets.len());
        }
    }
}

pub fn render_sources(sources: &[Source], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(sources);
        }
        OutputFormat::Table => {
            if sources.is_empty() {
                println!("No registered sources found.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Kind").fg(Color::Cyan),
                Cell::new("Root Path").fg(Color::Cyan),
                Cell::new("Status").fg(Color::Cyan),
            ]);
            for s in sources {
                table.add_row(vec![
                    Cell::new(&s.id),
                    Cell::new(&s.name),
                    Cell::new(format!("{:?}", s.kind)),
                    Cell::new(&s.root_path),
                    Cell::new(s.last_scan_status.as_deref().unwrap_or("Never scanned")),
                ]);
            }
            println!("{table}");
            println!("\nTotal: {} sources", sources.len());
        }
    }
}

pub fn render_tenants(tenants: &[Tenant], active_id: Option<&str>, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(tenants);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Active").fg(Color::Cyan),
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
            ]);
            for t in tenants {
                let is_active = active_id.map(|id| id == t.id).unwrap_or(false);
                let mark = if is_active {
                    Cell::new("✓").fg(Color::Green)
                } else {
                    Cell::new(" ")
                };
                table.add_row(vec![mark, Cell::new(&t.id), Cell::new(&t.name)]);
            }
            println!("{table}");
        }
    }
}

pub fn render_profiles(profiles: &[TargetProfileDescriptor], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(profiles);
        }
        OutputFormat::Table => {
            if profiles.is_empty() {
                println!("No target profiles found.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Profile ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("App Kind").fg(Color::Cyan),
                Cell::new("Deployment Strategy").fg(Color::Cyan),
            ]);
            for p in profiles {
                table.add_row(vec![
                    Cell::new(&p.id),
                    Cell::new(&p.name),
                    Cell::new(format!("{:?}", p.app_kind_compat)),
                    Cell::new(format!("{:?}", p.deployment_strategy)),
                ]);
            }
            println!("{table}");
            println!("\nTotal: {} target profiles", profiles.len());
        }
    }
}

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

pub fn render_conversation_sessions(
    sessions: &[asset_core::domain::conversations::ConversationSessionListItem],
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            render_ok_json(sessions);
        }
        OutputFormat::Table => {
            if sessions.is_empty() {
                println!("No conversation sessions found.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Session ID").fg(Color::Cyan),
                Cell::new("Adapter").fg(Color::Cyan),
                Cell::new("Title").fg(Color::Cyan),
                Cell::new("Questions / Turns").fg(Color::Cyan),
                Cell::new("Updated At").fg(Color::Cyan),
            ]);
            for s in sessions {
                table.add_row(vec![
                    Cell::new(&s.session.id),
                    Cell::new(&s.session.adapter_id),
                    Cell::new(&s.session.title),
                    Cell::new(format!("{} / {}", s.question_count, s.turn_count)),
                    Cell::new(s.session.updated_at.as_deref().unwrap_or("—")),
                ]);
            }
            println!("{table}");
            println!("\nTotal: {} sessions", sessions.len());
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

pub fn render_doctor(report: &serde_json::Value, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_ok_json(report);
        }
        OutputFormat::Table => {
            let checks = report.get("checks").and_then(serde_json::Value::as_array);

            let Some(checks) = checks else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(report).unwrap_or_default()
                );
                return;
            };

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Diagnostic Check").fg(Color::Cyan),
                Cell::new("Status").fg(Color::Cyan),
                Cell::new("Message / Output").fg(Color::Cyan),
            ]);

            for c in checks {
                let name = c
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let status = c
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let message = c
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");

                let status_cell = match status {
                    "pass" => Cell::new(status).fg(Color::Green),
                    "fail" => Cell::new(status).fg(Color::Red),
                    _ => Cell::new(status).fg(Color::Yellow),
                };

                table.add_row(vec![Cell::new(name), status_cell, Cell::new(message)]);
            }
            println!("{table}");
        }
    }
}

pub fn render_harvester_templates(
    templates: &[crate::service::harvester::TemplateSummary],
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            render_ok_json(&serde_json::json!({ "templates": templates }));
        }
        OutputFormat::Table => {
            if templates.is_empty() {
                println!("No harvester templates found.");
                return;
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec![
                Cell::new("Template ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Version").fg(Color::Cyan),
                Cell::new("Installed").fg(Color::Cyan),
                Cell::new("Description").fg(Color::Cyan),
            ]);

            for t in templates {
                table.add_row(vec![
                    Cell::new(&t.id),
                    Cell::new(&t.name),
                    Cell::new(&t.version),
                    if t.installed {
                        Cell::new("YES").fg(Color::Green)
                    } else {
                        Cell::new("NO").fg(Color::DarkGrey)
                    },
                    Cell::new(&t.description),
                ]);
            }
            println!("{table}");
        }
    }
}
