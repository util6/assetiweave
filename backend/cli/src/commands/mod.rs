mod agent;
mod conversation;
mod harvester;
mod memory;
mod skill;
mod source;
mod system;
mod tenant;

use crate::args::{CliArgs, Commands};
use crate::service::CliService;
use anyhow::Result;

pub async fn dispatch(cli: CliArgs) -> Result<()> {
    match cli.command {
        Some(Commands::Version(args)) => system::handle_version(args, cli.format).await?,
        Some(Commands::Overview) => system::handle_overview(cli.format).await?,
        Some(Commands::Skill(args)) => skill::handle_skill(args, cli.format).await?,
        Some(Commands::Asset(args)) => skill::handle_asset(args, cli.format).await?,
        Some(Commands::Source(args)) => source::handle_source(args, cli.format).await?,
        Some(Commands::Tenant(args)) => tenant::handle_tenant(args, cli.format).await?,
        Some(Commands::Profile(args)) => system::handle_profile(args, cli.format).await?,
        Some(Commands::Conversation(args)) => {
            conversation::handle_conversation(args, cli.format).await?
        }
        Some(Commands::Memory(args)) => memory::handle_memory(args, cli.format).await?,
        Some(Commands::Agent(args)) => agent::handle_agent(args, cli.format).await?,
        Some(Commands::Harvester(args)) => harvester::handle_harvester(args, cli.format).await?,
        Some(Commands::Settings(args)) => system::handle_settings(args, cli.format).await?,
        Some(Commands::Doctor(args)) => system::handle_doctor(args, cli.format).await?,
        Some(Commands::Config(args)) => system::handle_config(args, cli.format).await?,
        Some(Commands::Update(args)) => system::handle_update(args, cli.format).await?,
        Some(Commands::Completion(args)) => system::handle_completion(args).await?,
        Some(Commands::Schema(args)) => system::handle_schema(args, cli.format).await?,
        Some(Commands::Api(args)) => system::handle_api(args, cli.format).await?,
        None => {
            let service = CliService::bootstrap().await?;
            let model = service.get_overview().await?;
            crate::render::render_overview(&model, cli.format);
        }
    }
    Ok(())
}
