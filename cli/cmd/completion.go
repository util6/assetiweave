package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
)

func newCmdCompletion(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "completion [bash|zsh|fish|powershell]",
		Short: "Generate shell completion scripts",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			switch args[0] {
			case "bash":
				return cmd.Root().GenBashCompletion(f.IOStreams.Out)
			case "zsh":
				return cmd.Root().GenZshCompletion(f.IOStreams.Out)
			case "fish":
				return cmd.Root().GenFishCompletion(f.IOStreams.Out, true)
			case "powershell":
				return cmd.Root().GenPowerShellCompletion(f.IOStreams.Out)
			default:
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "unknown completion shell: %s", args[0]).
					WithCode("validation")
			}
		},
	}
	return cmd
}
