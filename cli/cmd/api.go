package cmd

import (
	"encoding/json"
	"io"
	"os"
	"strings"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
)

func newCmdAPI(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "api", Short: "Call engine methods directly"}
	cmd.AddCommand(newCmdAPICall(f))
	return cmd
}

func newCmdAPICall(f *cmdutil.Factory) *cobra.Command {
	var jsonArg string
	var yes bool
	cmd := &cobra.Command{
		Use:   "call <method>",
		Short: "Call an engine method with JSON params",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			params, err := readJSONParams(f, jsonArg)
			if err != nil {
				return err
			}
			if yes {
				params["yes"] = true
			}
			return callAndPrint(cmd, f, args[0], params)
		},
	}
	cmd.Flags().StringVar(&jsonArg, "json", "", "JSON params, @file path, or - for stdin")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm a high-risk engine method")
	return cmd
}

func readJSONParams(f *cmdutil.Factory, value string) (map[string]any, error) {
	value = strings.TrimSpace(value)
	if value == "" {
		return map[string]any{}, nil
	}

	var bytes []byte
	var err error
	switch {
	case value == "-":
		bytes, err = io.ReadAll(f.IOStreams.In)
	case strings.HasPrefix(value, "@"):
		bytes, err = os.ReadFile(strings.TrimPrefix(value, "@"))
	default:
		bytes = []byte(value)
	}
	if err != nil {
		return nil, errs.NewValidationError(errs.SubtypeInvalidArgument, "failed to read JSON params: %v", err).
			WithCode("validation").
			WithCause(err)
	}

	var params map[string]any
	if err := json.Unmarshal(bytes, &params); err != nil {
		return nil, errs.NewValidationError(errs.SubtypeInvalidJSON, err.Error()).
			WithCode("invalid_json").
			WithHint("pass a valid JSON object, @file, or - for stdin").
			WithCause(err)
	}
	if params == nil {
		return nil, errs.NewValidationError(errs.SubtypeInvalidJSON, "engine params must be a JSON object").
			WithCode("invalid_json").
			WithHint("pass a valid JSON object, @file, or - for stdin")
	}
	return params, nil
}
