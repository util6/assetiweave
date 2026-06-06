package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdOverview(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "overview",
		Short: "Show AssetIWeave catalog overview",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodOverviewGet, map[string]any{})
		},
	}
}
