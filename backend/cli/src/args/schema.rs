use clap::Args;

#[derive(Args, Debug)]
#[command(about = "Inspect engine method schemas")]
pub struct SchemaArgs {
    #[arg(help = "optional method name to inspect")]
    pub method: Option<String>,
}
