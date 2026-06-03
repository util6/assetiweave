package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdDoctor(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "doctor",
		Short: "Run local CLI and engine diagnostics",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodDoctorRun, map[string]any{})
		},
	}
}
