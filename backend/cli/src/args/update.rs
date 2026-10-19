use clap::Args;

#[derive(Args, Debug)]
#[command(about = "Check for AssetIWeave CLI updates")]
pub struct UpdateArgs {
    #[arg(long, help = "only check for updates")]
    pub check: bool,

    #[arg(long, help = "replace local CLI tools after checksum verification")]
    pub yes: bool,
}
