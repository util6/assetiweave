use crate::args::agent::{AgentArgs, AgentCommand, AgentInstalledCommand, AgentMarketCommand};
use crate::args::OutputFormat;
use crate::render;
use crate::service::CliService;
use anyhow::Result;

pub async fn handle_agent(args: AgentArgs, _format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        AgentCommand::Market(m) => match m.command {
            AgentMarketCommand::List(l) => {
                let res = service
                    .agent()
                    .list_market(l.query, l.protocol, l.installed_only)
                    .await?;
                render::render_json_value(&res);
            }
            AgentMarketCommand::Inspect(i) => {
                let res = service.agent().inspect_market(i.agent_id).await?;
                render::render_json_value(&res);
            }
            AgentMarketCommand::Refresh(_r) => {
                let res = service.agent().refresh_market().await?;
                render::render_json_value(&res);
            }
        },
        AgentCommand::Installed(ins) => match ins.command {
            Some(AgentInstalledCommand::Get(g)) => {
                let res = service.agent().get_installed(g.agent_id).await?;
                render::render_json_value(&res);
            }
            _ => {
                let res = service.agent().list_installed().await?;
                render::render_json_value(&res);
            }
        },
        AgentCommand::Install(i) => {
            let res = service
                .agent()
                .run_install(i.agent_id, i.distribution, "install".to_string(), i.yes)
                .await?;
            render::render_json_value(&res);
        }
        AgentCommand::Update(u) => {
            let res = service
                .agent()
                .run_install(u.agent_id, u.distribution, "update".to_string(), u.yes)
                .await?;
            render::render_json_value(&res);
        }
        AgentCommand::Reinstall(r) => {
            let res = service
                .agent()
                .run_install(r.agent_id, r.distribution, "reinstall".to_string(), r.yes)
                .await?;
            render::render_json_value(&res);
        }
        AgentCommand::Uninstall(un) => {
            let res = service.agent().uninstall(un.agent_id, un.yes).await?;
            render::render_json_value(&res);
        }
        AgentCommand::Enable(e) => {
            let res = service.agent().enable(e.agent_id).await?;
            render::render_json_value(&res);
        }
        AgentCommand::Disable(d) => {
            let res = service.agent().disable(d.agent_id).await?;
            render::render_json_value(&res);
        }
        AgentCommand::Check(c) => {
            let res = service.agent().check(c.agent_id).await?;
            render::render_json_value(&res);
        }
    }
    Ok(())
}
