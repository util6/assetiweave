package cmd

import (
	"encoding/json"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdAgent(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "agent", Short: "Manage Agent Market installations and runtime bindings"}
	cmd.AddCommand(newCmdAgentMarket(f))
	cmd.AddCommand(newCmdAgentInstalled(f))
	cmd.AddCommand(newCmdAgentInstall(f))
	cmd.AddCommand(newCmdAgentUpdate(f))
	cmd.AddCommand(newCmdAgentReinstall(f))
	cmd.AddCommand(newCmdAgentUninstall(f))
	cmd.AddCommand(newCmdAgentToggle(f, true))
	cmd.AddCommand(newCmdAgentToggle(f, false))
	cmd.AddCommand(newCmdAgentCheck(f))
	return cmd
}

func newCmdAgentMarket(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "market", Short: "Browse curated Agent Market items"}
	cmd.AddCommand(newCmdAgentMarketList(f))
	cmd.AddCommand(newCmdAgentMarketInspect(f))
	cmd.AddCommand(newCmdAgentMarketRefresh(f))
	return cmd
}

func newCmdAgentMarketRefresh(f *cmdutil.Factory) *cobra.Command {
	var yes bool
	cmd := &cobra.Command{
		Use:   "refresh",
		Short: "Refresh the controlled curated Agent Market catalog",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := requireYes(yes, "Agent Market refresh"); err != nil {
				return err
			}
			return callAndPrint(cmd, f, schema.MethodAgentMarketRefresh, map[string]any{"yes": true})
		},
	}
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm catalog network access and cache write")
	return cmd
}

func newCmdAgentMarketList(f *cmdutil.Factory) *cobra.Command {
	var query, protocol string
	var installedOnly, includeIncompatible bool
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List curated Agent Market items",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{
				"installedOnly":       installedOnly,
				"includeIncompatible": includeIncompatible,
			}
			if query != "" {
				params["query"] = query
			}
			if protocol != "" {
				params["protocol"] = protocol
			}
			return callAndPrint(cmd, f, schema.MethodAgentMarketList, params)
		},
	}
	cmd.Flags().StringVar(&query, "query", "", "search Agent id, name or description")
	cmd.Flags().StringVar(&protocol, "protocol", "", "protocol filter: acp or native")
	cmd.Flags().BoolVar(&installedOnly, "installed-only", false, "only list installed Agents")
	cmd.Flags().BoolVar(&includeIncompatible, "include-incompatible", true, "legacy compatibility flag; Agent versions are observational")
	return cmd
}

func newCmdAgentMarketInspect(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "inspect <agent-id>",
		Short: "Inspect one curated Agent Market item",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodAgentMarketInspect, map[string]any{"agentId": args[0]})
		},
	}
}

func newCmdAgentInstalled(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "installed",
		Short: "List current Agent installations",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodAgentInstalledList, map[string]any{})
		},
	}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List current Agent installations",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodAgentInstalledList, map[string]any{})
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "get <agent-id>",
		Short: "Get one current Agent installation",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodAgentInstalledGet, map[string]any{"agentId": args[0]})
		},
	})
	return cmd
}

type agentInstallPreview struct {
	CatalogVersion string `json:"catalogVersion"`
	TargetVersion  string `json:"targetVersion"`
	Selected       struct {
		DistributionID string `json:"distributionId"`
	} `json:"selectedDistribution"`
	PreviewToken string   `json:"previewToken"`
	Conflicts    []string `json:"conflicts"`
}

func newCmdAgentInstall(f *cmdutil.Factory) *cobra.Command {
	return newCmdAgentLifecycle(f, "install", "install <agent-id>", "Preview and synchronously install one Agent")
}

func newCmdAgentUpdate(f *cmdutil.Factory) *cobra.Command {
	return newCmdAgentLifecycle(f, "update", "update <agent-id>", "Preview and synchronously update one Agent")
}

func newCmdAgentReinstall(f *cmdutil.Factory) *cobra.Command {
	return newCmdAgentLifecycle(f, "reinstall", "reinstall <agent-id>", "Preview and synchronously reinstall one Agent")
}

func newCmdAgentLifecycle(f *cmdutil.Factory, defaultAction, use, short string) *cobra.Command {
	var distribution string
	var yes bool
	cmd := &cobra.Command{
		Use:   use,
		Short: short,
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			previewParams := map[string]any{"agentId": args[0], "action": defaultAction}
			if distribution != "" {
				previewParams["distributionId"] = distribution
			}
			previewResult, err := callEngine(cmd, f, schema.MethodAgentInstallPreview, previewParams)
			if err != nil {
				return err
			}
			var preview agentInstallPreview
			if err := json.Unmarshal(previewResult.Data, &preview); err != nil {
				return errs.NewEngineError(errs.SubtypeEngineProtocol, "invalid Agent installation preview: %v", err).WithCode("agent_preview_invalid").WithCause(err)
			}
			if !yes {
				output.WriteSuccess(f.IOStreams.Out, previewResult.Data)
				return nil
			}
			if len(preview.Conflicts) > 0 {
				return errs.NewEngineError(errs.SubtypeConflict, "Agent installation has conflicts: %v", preview.Conflicts).WithCode("agent_conflict")
			}
			return callAndPrint(cmd, f, schema.MethodAgentInstallRun, map[string]any{
				"agentId":        args[0],
				"action":         defaultAction,
				"catalogVersion": preview.CatalogVersion,
				"agentVersion":   preview.TargetVersion,
				"distributionId": preview.Selected.DistributionID,
				"previewToken":   preview.PreviewToken,
				"yes":            true,
			})
		},
	}
	cmd.Flags().StringVar(&distribution, "distribution", "", "distribution id from `agent market inspect`")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm installation and local runtime writes")
	cmd.AddCommand(newCmdAgentInstallPreview(f, defaultAction))
	return cmd
}

func newCmdAgentInstallPreview(f *cmdutil.Factory, defaultAction string) *cobra.Command {
	var distribution, action string
	cmd := &cobra.Command{
		Use:   "preview <agent-id>",
		Short: "Preview an Agent installation or update",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{"agentId": args[0], "action": action}
			if distribution != "" {
				params["distributionId"] = distribution
			}
			return callAndPrint(cmd, f, schema.MethodAgentInstallPreview, params)
		},
	}
	cmd.Flags().StringVar(&distribution, "distribution", "", "distribution id from `agent market inspect`")
	cmd.Flags().StringVar(&action, "action", defaultAction, "lifecycle action: install, update or reinstall")
	return cmd
}

type agentUninstallPreview struct {
	PreviewToken          string   `json:"previewToken"`
	CapabilityAssignments []string `json:"capabilityAssignments"`
	Conflicts             []string `json:"conflicts"`
}

func newCmdAgentUninstall(f *cmdutil.Factory) *cobra.Command {
	var yes bool
	var clearAssignments []string
	cmd := &cobra.Command{
		Use:   "uninstall <agent-id>",
		Short: "Preview and synchronously uninstall or unbind one Agent",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			previewResult, err := callEngine(cmd, f, schema.MethodAgentUninstallPreview, map[string]any{"agentId": args[0]})
			if err != nil {
				return err
			}
			var preview agentUninstallPreview
			if err := json.Unmarshal(previewResult.Data, &preview); err != nil {
				return errs.NewEngineError(errs.SubtypeEngineProtocol, "invalid Agent uninstall preview: %v", err).WithCode("agent_preview_invalid").WithCause(err)
			}
			if !yes {
				output.WriteSuccess(f.IOStreams.Out, previewResult.Data)
				return nil
			}
			return callAndPrint(cmd, f, schema.MethodAgentUninstallRun, map[string]any{
				"agentId":                    args[0],
				"clearCapabilityAssignments": clearAssignments,
				"previewToken":               preview.PreviewToken,
				"yes":                        true,
			})
		},
	}
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm uninstall and managed runtime cleanup")
	cmd.Flags().StringSliceVar(&clearAssignments, "clear-assignment", nil, "capability assignment to clear before uninstall; repeatable")
	cmd.AddCommand(newCmdAgentUninstallPreview(f))
	return cmd
}

func newCmdAgentUninstallPreview(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "preview <agent-id>",
		Short: "Preview Agent references and cleanup scope",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodAgentUninstallPreview, map[string]any{"agentId": args[0]})
		},
	}
}

func newCmdAgentToggle(f *cmdutil.Factory, enabled bool) *cobra.Command {
	name := "disable"
	method := schema.MethodAgentDisable
	if enabled {
		name = "enable"
		method = schema.MethodAgentEnable
	}
	return &cobra.Command{
		Use:   name + " <agent-id>",
		Short: name + " one installed Agent",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, method, map[string]any{"agentId": args[0]})
		},
	}
}

func newCmdAgentCheck(f *cmdutil.Factory) *cobra.Command {
	var protocol bool
	cmd := &cobra.Command{
		Use:   "check <agent-id>",
		Short: "Check one installed Agent runtime or protocol",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			mode := "installation"
			if protocol {
				mode = "connection"
			}
			method := schema.MethodAgentRuntimeCheck
			params := map[string]any{"agentId": args[0]}
			if protocol {
				method = schema.MethodAgentConnectionCheck
				params["mode"] = mode
			}
			return callAndPrint(cmd, f, method, params)
		},
	}
	cmd.Flags().BoolVar(&protocol, "protocol", false, "run the bounded Agent protocol check")
	return cmd
}
