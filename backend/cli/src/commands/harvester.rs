use crate::args::harvester::{HarvesterArgs, HarvesterCommand, HarvesterTemplateCommand};
use crate::args::OutputFormat;
use crate::render;
use crate::service;
use anyhow::Result;

pub async fn handle_harvester(args: HarvesterArgs, format: OutputFormat) -> Result<()> {
    match args.command {
        HarvesterCommand::Template(t) => match t.command {
            HarvesterTemplateCommand::List(l) => {
                let templates = service::harvester::HarvesterOperations::list_templates(l.root)?;
                render::render_harvester_templates(&templates, format);
            }
        },
        HarvesterCommand::List(l) => {
            let installed = service::harvester::HarvesterOperations::list_installed(l.root)?;
            render::render_harvester_templates(&installed, format);
        }
        HarvesterCommand::Install(i) => {
            let res =
                service::harvester::HarvesterOperations::install(&i.id, i.from, i.root, i.force)?;
            render::render_generic(&res, format);
        }
        HarvesterCommand::Update(u) => {
            let res = service::harvester::HarvesterOperations::update(u.id, u.from, u.root, u.all)?;
            render::render_generic(&res, format);
        }
        HarvesterCommand::Doctor(d) => {
            let res = service::harvester::HarvesterOperations::doctor(&d.id, d.root)?;
            render::render_generic(&res, format);
        }
        HarvesterCommand::Repair(r) => {
            let res =
                service::harvester::HarvesterOperations::repair(&r.id, r.root, r.dry_run, r.yes)?;
            render::render_generic(&res, format);
        }
        HarvesterCommand::Run(r) => {
            let res = service::harvester::HarvesterOperations::run(
                &r.id, r.args, r.root, r.timeout, r.yes,
            )?;
            render::render_generic(&res, format);
        }
    }
    Ok(())
}
