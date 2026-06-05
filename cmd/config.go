package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	internalplatform "github.com/util6/assetiweave/internal/platform"
)

func newCmdConfig(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "config", Short: "Inspect local CLI configuration"}
	cmd.AddCommand(newCmdConfigPlugins(f))
	return cmd
}

func newCmdConfigPlugins(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "plugins", Short: "Inspect CLI plugins"}
	cmd.AddCommand(&cobra.Command{
		Use:   "show",
		Short: "Show installed plugin inventory",
		Args:  cobra.NoArgs,
		RunE: func(*cobra.Command, []string) error {
			inventory := internalplatform.GetActiveInventory()
			if inventory == nil {
				inventory = internalplatform.BuildInventory(nil, nil, nil)
			}
			output.WriteSuccess(f.IOStreams.Out, inventory)
			return nil
		},
	})
	return cmd
}
