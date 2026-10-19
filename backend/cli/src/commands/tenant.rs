use crate::args::tenant::{TenantArgs, TenantCommands};
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::Result;

pub async fn handle_tenant(args: TenantArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        TenantCommands::List => {
            let tenants = service.list_tenants().await?;
            let active = service.active_tenant().await.ok();
            render::render_tenants(&tenants, active.as_ref().map(|t| t.id.as_str()), format);
        }
        TenantCommands::Active => {
            let active = service.active_tenant().await?;
            render::render_generic(&active, format);
        }
        TenantCommands::Switch(s) => {
            let active = service.switch_tenant(s.id).await?;
            render::render_generic(&active, format);
        }
        TenantCommands::Create(_c) => {
            println!("Tenant create called.");
        }
    }
    Ok(())
}
