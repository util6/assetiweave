package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdSource(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "source", Short: "Manage asset sources"}
	cmd.AddCommand(newCmdSourceList(f))
	cmd.AddCommand(newCmdSourceAdd(f))
	cmd.AddCommand(newCmdSourceRemove(f))
	cmd.AddCommand(newCmdSourceScan(f))
	return cmd
}

func newCmdSourceList(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List sources",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSourceList, map[string]any{})
		},
	}
}

func newCmdSourceAdd(f *cmdutil.Factory) *cobra.Command {
	var name, path, kind string
	var enabled bool
	var dryRun bool
	var priority int
	cmd := &cobra.Command{
		Use:   "add",
		Short: "Add a local source",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{
				"name":            name,
				"kind":            "local",
				"root_path":       path,
				"scanner_kind":    kind,
				"source_origin":   "local_folder",
				"include_globs":   []string{"**/SKILL.md"},
				"exclude_globs":   []string{"**/.git/**", "**/node_modules/**", "**/target/**", "**/dist/**"},
				"default_kind":    "skill",
				"enabled":         enabled,
				"dry_run":         dryRun,
				"priority":        priority,
				"repo_root":       nil,
				"scan_root":       "",
				"origin_app_kind": nil,
			}
			return callAndPrint(cmd, f, schema.MethodSourceAdd, params)
		},
	}
	cmd.Flags().StringVar(&name, "name", "", "source display name")
	cmd.Flags().StringVar(&path, "path", "", "source root directory")
	cmd.Flags().StringVar(&kind, "scanner-kind", "skill", "scanner kind")
	cmd.Flags().BoolVar(&enabled, "enabled", true, "enable source")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without adding the source")
	cmd.Flags().IntVar(&priority, "priority", 100, "source priority")
	_ = cmd.MarkFlagRequired("name")
	_ = cmd.MarkFlagRequired("path")
	return cmd
}

func newCmdSourceRemove(f *cmdutil.Factory) *cobra.Command {
	var dryRun, yes bool
	cmd := &cobra.Command{
		Use:   "remove <source-id>",
		Short: "Remove a source registration",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !dryRun {
				if err := requireYes(yes, "source remove"); err != nil {
					return err
				}
			}
			return callAndPrint(cmd, f, schema.MethodSourceRemove, map[string]any{"id": args[0], "dry_run": dryRun, "yes": yes})
		},
	}
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without changing state")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm source removal")
	return cmd
}

func newCmdSourceScan(f *cmdutil.Factory) *cobra.Command {
	var kind string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "scan",
		Short: "Scan sources",
		RunE: func(cmd *cobra.Command, args []string) error {
			var value any
			if kind != "" {
				value = kind
			}
			return callAndPrint(cmd, f, schema.MethodSourceScan, map[string]any{"kind": value, "dry_run": dryRun})
		},
	}
	cmd.Flags().StringVar(&kind, "kind", "", "asset kind filter, e.g. skill")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "return current assets without scanning")
	return cmd
}
