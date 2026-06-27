package cmd

import (
	"context"
	"errors"
	"os"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/extension/platform"
	engineclient "github.com/util6/assetiweave/internal/client"
	"github.com/util6/assetiweave/internal/cmdpolicy"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/hook"
	"github.com/util6/assetiweave/internal/output"
	internalplatform "github.com/util6/assetiweave/internal/platform"
	"github.com/util6/assetiweave/internal/protocol"
	"github.com/util6/assetiweave/internal/update"
)

const rootLong = `assetiweave-cli controls AssetIWeave through the local Rust engine.

The CLI is designed for AI agents and scripts:
  - success data is written to stdout as JSON
  - errors are written to stderr as structured JSON
  - write commands support --dry-run
  - destructive commands require --yes`

const hideProfilesEnv = "ASSETIWEAVE_CLI_HIDE_PROFILES"

func Execute() int {
	f := cmdutil.NewDefault(cmdutil.SystemIO())
	ctx := context.Background()
	completionCommand := isCompletionCommandArgs(os.Args[1:])
	options, err := parseBootstrapOptions(os.Args[1:])
	if err != nil {
		return handleError(f, err)
	}
	applyBootstrapOptionsToFactory(f, options)
	root, registry := buildInternalWithOptions(ctx, f, buildOptions{
		SkipRuntime:  completionCommand,
		HideProfiles: shouldHideProfiles(),
	})
	if !completionCommand {
		setupNotices()
	}
	runErr := root.Execute()
	if registry != nil && !completionCommand {
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

type buildOptions struct {
	SkipRuntime  bool
	HideProfiles bool
}

func buildInternal(ctx context.Context, f *cmdutil.Factory) (*cobra.Command, *hook.Registry) {
	return buildInternalWithOptions(ctx, f, buildOptions{})
}

func buildInternalWithOptions(ctx context.Context, f *cmdutil.Factory, options buildOptions) (*cobra.Command, *hook.Registry) {
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
	root.PersistentFlags().StringVar(&f.EnginePath, "engine", f.EnginePath, "path to assetiweave-engine binary")
	root.PersistentFlags().StringVar(&f.PluginConfigPath, "plugin-config", f.PluginConfigPath, "path to CLI plugin config JSON")
	root.PersistentFlags().StringVar(&f.PolicyPath, "policy", f.PolicyPath, "path to Engine command policy JSON")

	root.AddCommand(newCmdVersion(f))
	root.AddCommand(newCmdOverview(f))
	root.AddCommand(newCmdTenant(f))
	root.AddCommand(newCmdSource(f))
	root.AddCommand(newCmdProfile(f))
	root.AddCommand(newCmdAsset(f))
	root.AddCommand(newCmdSkill(f))
	root.AddCommand(newCmdConversation(f))
	root.AddCommand(newCmdHarvester(f))
	root.AddCommand(newCmdSettings(f))
	root.AddCommand(newCmdApp(f))
	root.AddCommand(newCmdConfig(f))
	root.AddCommand(newCmdAPI(f))
	root.AddCommand(newCmdSchema(f))
	root.AddCommand(newCmdDoctor(f))
	root.AddCommand(newCmdUpdate(f))
	root.AddCommand(newCmdCompletion(f))
	annotateCommandTree(root)
	applyProfileVisibility(root, options.HideProfiles)
	installCobraValidation(root)
	if options.SkipRuntime {
		internalplatform.SetActiveInventory(internalplatform.BuildInventory(nil, nil, nil))
		return root, nil
	}

	plugins := platform.RegisteredPlugins()
	if len(plugins) == 0 {
		internalplatform.SetActiveInventory(internalplatform.BuildInventory(nil, nil, nil))
		return root, nil
	}
	pluginConfig, configErr := loadPluginConfig(f)
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

func setupNotices() {
	update.SetPending(nil)
	if info := update.CheckCached(protocol.CLIVersion); info != nil {
		update.SetPending(info)
	}
	output.PendingNotice = composePendingNotice
	currentVersion := protocol.CLIVersion
	go func() {
		defer func() {
			_ = recover()
		}()
		update.RefreshCache(currentVersion)
	}()
}

func composePendingNotice() map[string]any {
	notice := map[string]any{}
	if info := update.GetPending(); info != nil {
		notice["update"] = map[string]any{
			"current": info.Current,
			"latest":  info.Latest,
			"message": info.Message(),
			"command": "download the latest AssetIWeave release",
		}
	}
	if len(notice) == 0 {
		return nil
	}
	return notice
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
	internalErr := errs.NewInternalError(errs.SubtypeUnknown, "%v", err).
		WithCode("internal").
		WithCause(err)
	_ = output.WriteTypedErrorEnvelope(f.IOStreams.ErrOut, internalErr)
	return output.ExitCodeOf(internalErr)
}

func callAndPrint(cmd *cobra.Command, f *cmdutil.Factory, method string, params any) error {
	result, err := callEngine(cmd, f, method, params)
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

func callEngine(cmd *cobra.Command, f *cmdutil.Factory, method string, params any) (engineclient.CallResult, error) {
	applyBootstrapOptions(f)
	return f.Client.Call(cmd.Context(), method, params)
}

func applyBootstrapOptions(f *cmdutil.Factory) {
	if f == nil {
		return
	}
	if client, ok := f.Client.(*engineclient.EngineClient); ok {
		if f.EnginePath != "" {
			client.Path = f.EnginePath
		}
		if f.PolicyPath != "" {
			client.PolicyPath = f.PolicyPath
		}
	}
}

func requireArg(args []string, name string) (string, error) {
	if len(args) == 0 || args[0] == "" {
		return "", errs.NewValidationError(errs.SubtypeInvalidArgument, "%s is required", name).
			WithCode("validation")
	}
	return args[0], nil
}

func requireYes(yes bool, action string) error {
	if yes {
		return nil
	}
	return errs.NewConfirmationRequiredError("%s requires --yes", action).
		WithCode("confirmation_required").
		WithHint("rerun the command with --yes after confirming the operation")
}

func isCompletionCommandArgs(args []string) bool {
	for _, arg := range args {
		if arg == "completion" || arg == "__complete" || arg == "__completeNoDesc" {
			return true
		}
	}
	return false
}

func shouldHideProfiles() bool {
	return os.Getenv(hideProfilesEnv) != ""
}

func applyProfileVisibility(root *cobra.Command, hide bool) {
	if !hide {
		return
	}
	if command := findCommand(root, []string{"profile"}); command != nil {
		command.Hidden = true
	}
}
