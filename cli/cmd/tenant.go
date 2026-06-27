package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdTenant(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "tenant", Short: "Manage local tenants"}
	cmd.AddCommand(newCmdTenantList(f))
	cmd.AddCommand(newCmdTenantActive(f))
	cmd.AddCommand(newCmdTenantCreate(f))
	cmd.AddCommand(newCmdTenantSwitch(f))
	return cmd
}

func newCmdTenantList(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List tenants",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTenantList, map[string]any{})
		},
	}
}

func newCmdTenantActive(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "active",
		Short: "Show the active tenant",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTenantActive, map[string]any{})
		},
	}
}

func newCmdTenantCreate(f *cmdutil.Factory) *cobra.Command {
	var slug string
	var setActive bool
	cmd := &cobra.Command{
		Use:   "create <name>",
		Short: "Create a local tenant",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{
				"name":       args[0],
				"slug":       nil,
				"set_active": setActive,
			}
			if slug != "" {
				params["slug"] = slug
			}
			return callAndPrint(cmd, f, schema.MethodTenantCreate, params)
		},
	}
	cmd.Flags().StringVar(&slug, "slug", "", "tenant stable slug")
	cmd.Flags().BoolVar(&setActive, "set-active", true, "switch to the created tenant")
	return cmd
}

func newCmdTenantSwitch(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "switch <tenant-id>",
		Short: "Switch the active tenant",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTenantSwitch, map[string]any{"id": args[0]})
		},
	}
}
