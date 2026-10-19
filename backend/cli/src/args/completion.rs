use clap::Args;
use clap_complete::Shell;

#[derive(Args, Debug)]
#[command(about = "Generate shell completion scripts")]
pub struct CompletionArgs {
    #[arg(value_enum, help = "target shell for completion")]
    pub shell: Shell,
}
