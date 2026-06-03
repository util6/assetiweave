package cmd

import (
	"encoding/json"
	"io"
	"os"
	"strings"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
)

func newCmdAPI(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "api", Short: "Call engine methods directly"}
	cmd.AddCommand(newCmdAPICall(f))
	return cmd
}

func newCmdAPICall(f *cmdutil.Factory) *cobra.Command {
	var jsonArg string
	cmd := &cobra.Command{
		Use:   "call <method>",
		Short: "Call an engine method with JSON params",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			params, err := readJSONParams(f, jsonArg)
			if err != nil {
				return err
			}
			return callAndPrint(cmd, f, args[0], params)
		},
	}
	cmd.Flags().StringVar(&jsonArg, "json", "", "JSON params, @file path, or - for stdin")
	return cmd
}

func readJSONParams(f *cmdutil.Factory, value string) (any, error) {
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
		return nil, output.Errorf(output.ExitValidation, "validation", "failed to read JSON params: %v", err)
	}

	var raw json.RawMessage
	if err := json.Unmarshal(bytes, &raw); err != nil {
		return nil, output.ErrWithHint(output.ExitValidation, "invalid_json", err.Error(), "pass a valid JSON object, @file, or - for stdin")
	}
	return raw, nil
}
