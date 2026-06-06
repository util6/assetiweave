package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	engineschema "github.com/util6/assetiweave/internal/schema"
)

func newCmdSchema(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "schema [method]",
		Short: "Inspect engine method schemas",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return callAndPrint(cmd, f, engineschema.MethodSchemaList, map[string]any{})
			}
			return callAndPrint(cmd, f, engineschema.MethodSchemaGet, map[string]any{"method": args[0]})
		},
	}
}
