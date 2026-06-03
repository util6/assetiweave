package cmd

import (
	"context"
	"errors"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
)

const rootLong = `assetiweave-cli controls AssetIWeave through the local Rust engine.

The CLI is designed for AI agents and scripts:
  - success data is written to stdout as JSON
  - errors are written to stderr as structured JSON
  - write commands support --dry-run
  - destructive commands require --yes`

func Execute() int {
	f := cmdutil.NewDefault(cmdutil.SystemIO())
	root := Build(context.Background(), f)
	if err := root.Execute(); err != nil {
		return handleError(f, err)
	}
	return 0
}

func Build(ctx context.Context, f *cmdutil.Factory) *cobra.Command {
	root := &cobra.Command{
		Use:           "assetiweave-cli",
		Short:         "AssetIWeave CLI for AI-agent workflows",
		Long:          rootLong,
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	root.SetContext(ctx)
	root.SetIn(f.IOStreams.In)
	root.SetOut(f.IOStreams.Out)
	root.SetErr(f.IOStreams.ErrOut)

	root.AddCommand(newCmdOverview(f))
	root.AddCommand(newCmdSource(f))
	root.AddCommand(newCmdProfile(f))
	root.AddCommand(newCmdAsset(f))
	root.AddCommand(newCmdSkill(f))
	root.AddCommand(newCmdAPI(f))
	root.AddCommand(newCmdSchema(f))
	root.AddCommand(newCmdDoctor(f))
	root.AddCommand(newCmdCompletion(f))

	return root
}

func handleError(f *cmdutil.Factory, err error) int {
	var exitErr *output.ExitError
	if errors.As(err, &exitErr) {
		output.WriteErrorEnvelope(f.IOStreams.ErrOut, exitErr)
		return exitErr.Code
	}
	output.WriteErrorEnvelope(f.IOStreams.ErrOut, output.Errorf(output.ExitInternal, "internal", "%v", err))
	return 1
}

func callAndPrint(cmd *cobra.Command, f *cmdutil.Factory, method string, params any) error {
	data, err := f.Client.Call(cmd.Context(), method, params)
	if err != nil {
		return err
	}
	output.WriteSuccess(f.IOStreams.Out, data)
	return nil
}

func requireArg(args []string, name string) (string, error) {
	if len(args) == 0 || args[0] == "" {
		return "", output.Errorf(output.ExitValidation, "validation", "%s is required", name)
	}
	return args[0], nil
}

func requireYes(yes bool, action string) error {
	if yes {
		return nil
	}
	return output.ErrWithHint(output.ExitValidation, "confirmation_required", fmt.Sprintf("%s requires --yes", action), "rerun the command with --yes after confirming the operation")
}

func isCompletionCommand() bool {
	for _, arg := range os.Args {
		if arg == "completion" || arg == "__complete" || arg == "__completeNoDesc" {
			return true
		}
	}
	return false
}
