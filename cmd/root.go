package cmd

import (
	"context"
	"errors"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdpolicy"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/hook"
	"github.com/util6/assetiweave/internal/output"
	internalplatform "github.com/util6/assetiweave/internal/platform"
)

const rootLong = `assetiweave-cli controls AssetIWeave through the local Rust engine.

The CLI is designed for AI agents and scripts:
  - success data is written to stdout as JSON
  - errors are written to stderr as structured JSON
  - write commands support --dry-run
  - destructive commands require --yes`

func Execute() int {
	f := cmdutil.NewDefault(cmdutil.SystemIO())
	ctx := context.Background()
	root, registry := buildInternal(ctx, f)
	runErr := root.Execute()
	if registry != nil && !isCompletionCommand() {
		_ = hook.Emit(ctx, registry, platform.Shutdown, runErr, f.IOStreams.ErrOut)
	}
	if runErr != nil {
		return handleError(f, runErr)
	}
	return 0
}

func Build(ctx context.Context, f *cmdutil.Factory) *cobra.Command {
	root, _ := buildInternal(ctx, f)
	return root
}

func buildInternal(ctx context.Context, f *cmdutil.Factory) (*cobra.Command, *hook.Registry) {
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

	root.AddCommand(newCmdVersion(f))
	root.AddCommand(newCmdOverview(f))
	root.AddCommand(newCmdSource(f))
	root.AddCommand(newCmdProfile(f))
	root.AddCommand(newCmdAsset(f))
	root.AddCommand(newCmdSkill(f))
	root.AddCommand(newCmdConversation(f))
	root.AddCommand(newCmdApp(f))
	root.AddCommand(newCmdConfig(f))
	root.AddCommand(newCmdAPI(f))
	root.AddCommand(newCmdSchema(f))
	root.AddCommand(newCmdDoctor(f))
	root.AddCommand(newCmdCompletion(f))
	annotateCommandTree(root)

	plugins := platform.RegisteredPlugins()
	if len(plugins) == 0 {
		internalplatform.SetActiveInventory(internalplatform.BuildInventory(nil, nil, nil))
		return root, nil
	}
	pluginConfig, configErr := loadPluginConfig()
	if configErr != nil {
		internalplatform.SetActiveInventory(nil)
		installPluginConfigGuard(root, configErr)
		return root, nil
	}
	installResult, err := internalplatform.InstallAllWithOptions(
		plugins,
		f.IOStreams.ErrOut,
		internalplatform.WithPluginConfig(pluginConfig),
	)
	if err != nil {
		internalplatform.SetActiveInventory(nil)
		installPluginInstallGuard(root, err)
		return root, nil
	}
	rules, source, err := cmdpolicy.ResolvePluginRules(installResult.PluginRules)
	if err != nil {
		internalplatform.SetActiveInventory(nil)
		installPluginPolicyGuard(root, err)
		return root, nil
	}
	if len(rules) > 0 {
		ruleName := ""
		if len(rules) == 1 {
			ruleName = rules[0].Name
		}
		engine := cmdpolicy.New(rules)
		cmdpolicy.Apply(root, engine.EvaluateAll(root), source, ruleName)
	}
	registry := installResult.Registry
	hook.Install(root, registry, f.IOStreams.ErrOut)
	if err := hook.Emit(ctx, registry, platform.Startup, nil, f.IOStreams.ErrOut); err != nil {
		internalplatform.SetActiveInventory(nil)
		installPluginLifecycleGuard(root, err)
		return root, nil
	}
	internalplatform.SetActiveInventory(internalplatform.BuildInventory(installResult.Plugins, registry, installResult.PluginRules))
	return root, registry
}

func handleError(f *cmdutil.Factory, err error) int {
	if output.WriteTypedErrorEnvelope(f.IOStreams.ErrOut, err) {
		return output.ExitCodeOf(err)
	}
	var exitErr *output.ExitError
	if errors.As(err, &exitErr) {
		output.WriteErrorEnvelope(f.IOStreams.ErrOut, exitErr)
		return exitErr.Code
	}
	output.WriteErrorEnvelope(f.IOStreams.ErrOut, output.Errorf(output.ExitInternal, "internal", "%v", err))
	return 1
}

func callAndPrint(cmd *cobra.Command, f *cmdutil.Factory, method string, params any) error {
	result, err := f.Client.Call(cmd.Context(), method, params)
	if err != nil {
		return err
	}
	if result.Meta == nil {
		output.WriteSuccess(f.IOStreams.Out, result.Data)
	} else {
		output.WriteSuccessWithMeta(f.IOStreams.Out, result.Data, result.Meta)
	}
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
	return output.ErrWithHint(output.ExitConfirmationRequired, "confirmation_required", fmt.Sprintf("%s requires --yes", action), "rerun the command with --yes after confirming the operation")
}

func isCompletionCommand() bool {
	for _, arg := range os.Args {
		if arg == "completion" || arg == "__complete" || arg == "__completeNoDesc" {
			return true
		}
	}
	return false
}
