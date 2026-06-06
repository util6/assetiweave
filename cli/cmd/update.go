package cmd

import (
	"os"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
	"github.com/util6/assetiweave/internal/selfupdate"
	"github.com/util6/assetiweave/internal/update"
)

func newCmdUpdate(f *cmdutil.Factory) *cobra.Command {
	var checkOnly bool
	var yes bool
	cmd := &cobra.Command{
		Use:   "update",
		Short: "Check for AssetIWeave CLI updates",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			previousNotice := output.PendingNotice
			output.PendingNotice = nil
			defer func() { output.PendingNotice = previousNotice }()

			result := selfupdate.Check(selfupdate.Options{
				CurrentVersion: protocol.CLIVersion,
				ManifestURL:    os.Getenv(update.ManifestURLEnv),
			})
			if checkOnly || result.Action != "update_available" {
				output.WriteSuccess(f.IOStreams.Out, result)
				return nil
			}
			if err := requireYes(yes, "update"); err != nil {
				return err
			}
			applied, err := selfupdate.Apply(cmd.Context(), result, selfupdate.ApplyOptions{})
			if err != nil {
				return errs.NewInternalError(errs.SubtypeUpdateFailed, "failed to update AssetIWeave CLI tools: %v", err).
					WithCode("update_failed").
					WithHint("rerun `assetiweave-cli update --check` and verify the release assets").
					WithCause(err)
			}
			output.WriteSuccess(f.IOStreams.Out, applied)
			return nil
		},
	}
	cmd.Flags().BoolVar(&checkOnly, "check", false, "only check for updates")
	cmd.Flags().BoolVar(&yes, "yes", false, "replace local CLI tools after checksum verification")
	return cmd
}
