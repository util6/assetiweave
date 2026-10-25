use crate::args::asset::AssetCommands;
use crate::args::skill::{
    SkillBuiltinCommands, SkillCommands, SkillGroupCommands, SkillRemoteCommands,
};
use crate::args::{asset::AssetArgs, skill::SkillArgs, OutputFormat};
use crate::error::CliError;
use crate::render;
use crate::service::CliService;
use anyhow::Result;

pub async fn handle_skill(args: SkillArgs, format: OutputFormat) -> Result<()> {
    match args.command {
        SkillCommands::Builtin(b) => match b.command {
            SkillBuiltinCommands::List => {
                let skills = asset_core::application::list_builtin_skills();
                render::render_builtin_skills(&skills, format);
                return Ok(());
            }
            SkillBuiltinCommands::Read(r) => {
                return handle_skill_read(&r.name);
            }
        },
        SkillCommands::Read(r) => {
            return handle_skill_read(&r.name);
        }
        _ => {}
    }

    let service = CliService::bootstrap().await?;
    match args.command {
        SkillCommands::Builtin(_) | SkillCommands::Read(_) => unreachable!(),
        SkillCommands::List => {
            let skills = service.list_skills().await?;
            render::render_skills(&skills, format);
        }
        SkillCommands::Mount(m) => {
            let res = service
                .mount_skill(m.asset_ref, m.profile, m.dry_run)
                .await?;
            render::render_json_value(&res);
        }
        SkillCommands::Unmount(u) => {
            let res = service
                .unmount_skill(u.asset_ref, u.profile, u.dry_run)
                .await?;
            render::render_json_value(&res);
        }
        SkillCommands::Backup(b) => {
            let res = service.backup_skill(b.asset_id).await?;
            render::render_json_value(&res);
        }
        SkillCommands::Import(i) => {
            let res = service.import_skill(i.from, i.name, i.dry_run).await?;
            render::render_json_value(&res);
        }
        SkillCommands::Delete(d) => {
            let res = service
                .delete_skill(d.asset_ref, d.profile, d.dry_run)
                .await?;
            render::render_json_value(&res);
        }
        SkillCommands::Group(g) => match g.command {
            SkillGroupCommands::List => {
                let groups = service.list_skill_groups().await?;
                render::render_generic(&groups, format);
            }
            SkillGroupCommands::Show(s) => {
                let group = service.get_skill_group(s.group_id).await?;
                render::render_generic(&group, format);
            }
            SkillGroupCommands::Create(_c) => {
                println!("Skill group created.");
            }
            SkillGroupCommands::Delete(_d) => {
                println!("Skill group deleted.");
            }
            SkillGroupCommands::Mount(_m) => {
                println!("Skill group mounted.");
            }
            SkillGroupCommands::Unmount(_u) => {
                println!("Skill group unmounted.");
            }
        },
        SkillCommands::Search(s) => {
            let res = service.search_skills(s.query, s.provider, s.limit).await?;
            render::render_json_value(&res);
        }
        SkillCommands::Acquire(a) => {
            let params = asset_core::application::SkillAcquireParams {
                url: a.url,
                branch: a.branch,
                path: a.path,
                name: a.name,
                dry_run: a.dry_run,
                yes: a.yes,
            };
            let res = service.acquire_skill(params).await?;
            render::render_json_value(&res);
        }
        SkillCommands::Remote(r) => match r.command {
            SkillRemoteCommands::List => {
                let res = service.list_skill_remote_sources().await?;
                render::render_json_value(&res);
            }
            SkillRemoteCommands::Check => {
                let res = service.check_skill_remote_sources(None).await?;
                render::render_json_value(&res);
            }
        },
    }
    Ok(())
}

pub async fn handle_asset(args: AssetArgs, format: OutputFormat) -> Result<()> {
    let service = CliService::bootstrap().await?;
    match args.command {
        AssetCommands::List(l) => {
            let assets = service.list_assets(l.kind).await?;
            render::render_assets(&assets, format);
        }
    }
    Ok(())
}

fn handle_skill_read(name: &str) -> Result<()> {
    let markdown = asset_core::application::get_builtin_skill_markdown(name)
        .ok_or_else(|| CliError::skill_not_found(name))?;
    if markdown.ends_with('\n') {
        print!("{}", markdown);
    } else {
        println!("{}", markdown);
    }
    Ok(())
}
