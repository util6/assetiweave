package cmd

import (
	"encoding/json"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdSettings(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "settings", Short: "Inspect application settings"}
	cmd.AddCommand(newCmdSettingsShow(f))
	cmd.AddCommand(newCmdSettingsSave(f))
	return cmd
}

func newCmdSettingsShow(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "show",
		Short: "Show application settings and managed paths",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSettingsGet, map[string]any{})
		},
	}
}

func newCmdSettingsSave(f *cmdutil.Factory) *cobra.Command {
	var jsonValue string
	cmd := &cobra.Command{
		Use:   "save",
		Short: "Save application settings JSON",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			var settings map[string]any
			if err := json.Unmarshal([]byte(jsonValue), &settings); err != nil {
				return errs.NewValidationError(errs.SubtypeInvalidJSON, "--json: %v", err).
					WithCode("invalid_json").
					WithHint("pass a JSON object, for example --json '{\"density\":\"compact\"}'").
					WithCause(err)
			}
			if settings == nil {
				return errs.NewValidationError(errs.SubtypeInvalidJSON, "--json must be a JSON object").
					WithCode("invalid_json")
			}
			return callAndPrint(cmd, f, schema.MethodSettingsSave, map[string]any{"settings": settings})
		},
	}
	cmd.Flags().StringVar(&jsonValue, "json", "", "application settings JSON object")
	_ = cmd.MarkFlagRequired("json")
	return cmd
}
