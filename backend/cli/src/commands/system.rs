use crate::args::api::{ApiArgs, ApiCommand};
use crate::args::config::{ConfigArgs, ConfigCommand, ConfigPluginsCommand};
use crate::args::doctor::DoctorArgs;
use crate::args::profile::{ProfileArgs, ProfileCommands};
use crate::args::schema::SchemaArgs;
use crate::args::settings::{SettingsArgs, SettingsCommand};
use crate::args::update::UpdateArgs;
use crate::args::{completion::CompletionArgs, OutputFormat, VersionArgs};
use crate::render;
use crate::service::CliService;
use anyhow::Result;

pub async fn handle_version(args: VersionArgs, format: OutputFormat) -> Result<()> {
    if args.check_updates {
        let service = CliService::bootstrap().await?;
        let update_info = service.system().check_update(true, false);
        render::render_generic(&update_info, format);
    } else {
        let model = CliService::get_version_static();
        render::render_version(&model, format);
    }
    Ok(())
}

pub async fn handle_overview(format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    let model = service.get_overview().await?;
    render::render_overview(&model, format);
    Ok(())
}

pub async fn handle_profile(args: ProfileArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        ProfileCommands::List => {
            let profiles = service.list_profiles()?;
            render::render_profiles(&profiles, format);
        }
    }
    Ok(())
}

pub async fn handle_settings(args: SettingsArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        SettingsCommand::Show => {
            let res = service.system().get_settings().await?;
            render::render_generic(&res, format);
        }
        SettingsCommand::Save(s) => {
            let res = service.system().save_settings(&s.json).await?;
            render::render_generic(&res, format);
        }
    }
    Ok(())
}

pub async fn handle_doctor(_args: DoctorArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    let report = service.system().run_doctor().await?;
    render::render_doctor(&report, format);
    Ok(())
}

pub async fn handle_config(args: ConfigArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        ConfigCommand::Plugins(p) => match p.command {
            ConfigPluginsCommand::Show => {
                let res = service.system().config_plugins_show();
                render::render_generic(&res, format);
            }
        },
    }
    Ok(())
}

pub async fn handle_update(args: UpdateArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    let res = service.system().check_update(args.check, args.yes);
    render::render_generic(&res, format);
    Ok(())
}

pub async fn handle_completion(args: CompletionArgs) -> Result<()> {
    let service = CliService::bootstrap().await?;
    service.system().generate_completion(args.shell);
    Ok(())
}

pub async fn handle_schema(args: SchemaArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    let res = match args.method {
        Some(m) => service.system().schema_get(&m)?,
        None => service.system().schema_list()?,
    };
    render::render_generic(&res, format);
    Ok(())
}

pub async fn handle_api(args: ApiArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        ApiCommand::Call(c) => {
            let res = service
                .system()
                .api_call(&c.method, c.json.as_deref(), c.yes)
                .await?;
            render::render_generic(&res, format);
        }
    }
    Ok(())
}
