package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdProfile(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "profile", Short: "Manage target profiles"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List target profiles",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodProfileList, map[string]any{})
		},
	})
	return cmd
}
