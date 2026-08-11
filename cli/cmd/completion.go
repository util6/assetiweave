package cmd

import (
	"fmt"
	"io"

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
			if !isCompletionShell(args[0]) {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "unknown completion shell: %s", args[0]).
					WithCode("validation")
			}
			root := cmd.Root()
			canonicalUse := root.Use
			defer func() { root.Use = canonicalUse }()
			if err := writeCompletion(root, args[0], f.IOStreams.Out); err != nil {
				return err
			}
			if _, err := fmt.Fprintln(f.IOStreams.Out); err != nil {
				return err
			}
			root.Use = "aiwc"
			return writeCompletion(root, args[0], f.IOStreams.Out)
		},
	}
	return cmd
}

func isCompletionShell(shell string) bool {
	switch shell {
	case "bash", "zsh", "fish", "powershell":
		return true
	default:
		return false
	}
}

func writeCompletion(root *cobra.Command, shell string, writer io.Writer) error {
	switch shell {
	case "bash":
		return root.GenBashCompletion(writer)
	case "zsh":
		return root.GenZshCompletion(writer)
	case "fish":
		return root.GenFishCompletion(writer, true)
	case "powershell":
		return root.GenPowerShellCompletion(writer)
	default:
		return nil
	}
}
