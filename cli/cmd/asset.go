package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdAsset(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "asset", Short: "Inspect indexed assets"}
	cmd.AddCommand(newCmdAssetList(f))
	return cmd
}

func newCmdAssetList(f *cmdutil.Factory) *cobra.Command {
	var kind string
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List indexed assets",
		RunE: func(cmd *cobra.Command, args []string) error {
			var value any
			if kind != "" {
				value = kind
			}
			return callAndPrint(cmd, f, schema.MethodAssetList, map[string]any{"kind": value})
		},
	}
	cmd.Flags().StringVar(&kind, "kind", "", "asset kind filter, e.g. skill")
	return cmd
}
