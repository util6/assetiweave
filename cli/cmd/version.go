package cmd

import (
	"encoding/json"
	"os"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
	"github.com/util6/assetiweave/internal/schema"
	"github.com/util6/assetiweave/internal/update"
)

type versionReport struct {
	CLIVersion            string              `json:"cli_version"`
	CLIRelease            protocol.CLIRelease `json:"cli_release"`
	EngineVersion         string              `json:"engine_version"`
	CLIProtocolVersion    int                 `json:"cli_protocol_version"`
	EngineProtocolVersion int                 `json:"engine_protocol_version"`
	CLIContractVersion    int                 `json:"cli_contract_version"`
	EngineContractVersion int                 `json:"engine_contract_version"`
	Compatible            bool                `json:"compatible"`
	Capabilities          []string            `json:"capabilities"`
	Update                *update.Report      `json:"update,omitempty"`
}

func newCmdVersion(f *cmdutil.Factory) *cobra.Command {
	var checkUpdates bool
	cmd := &cobra.Command{
		Use:   "version",
		Short: "Show CLI and Engine compatibility versions",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			result, err := callEngine(cmd, f, schema.MethodSystemVersion, map[string]any{})
			if err != nil {
				return err
			}
			var engine protocol.EngineVersion
			if err := json.Unmarshal(result.Data, &engine); err != nil {
				return errs.NewEngineError(errs.SubtypeEngineProtocol, "system.version returned invalid data: %v", err).
					WithCode("engine_protocol").
					WithHint("install a matching AssetIWeave CLI and Engine release").
					WithMeta(result.Meta).
					WithCause(err)
			}
			if engine.EngineVersion == "" || engine.ProtocolVersion == 0 || engine.ContractVersion == 0 {
				return errs.NewEngineError(errs.SubtypeEngineProtocol, "system.version returned incomplete compatibility data").
					WithCode("engine_protocol").
					WithHint("install a matching AssetIWeave CLI and Engine release").
					WithMeta(result.Meta)
			}
			report := versionReport{
				CLIVersion:            protocol.CLIVersion,
				CLIRelease:            protocol.CLIReleaseInfo(),
				EngineVersion:         engine.EngineVersion,
				CLIProtocolVersion:    protocol.Version,
				EngineProtocolVersion: engine.ProtocolVersion,
				CLIContractVersion:    protocol.ContractVersion,
				EngineContractVersion: engine.ContractVersion,
				Compatible:            protocol.Compatible(engine.ProtocolVersion, engine.ContractVersion),
				Capabilities:          engine.Capabilities,
			}
			if checkUpdates {
				report.Update = ptr(update.CheckAndCache(protocol.CLIVersion, os.Getenv(update.ManifestURLEnv)))
			}
			if result.Meta == nil {
				output.WriteSuccess(f.IOStreams.Out, report)
			} else {
				output.WriteSuccessWithMeta(f.IOStreams.Out, report, result.Meta)
			}
			return nil
		},
	}
	cmd.Flags().BoolVar(&checkUpdates, "check-updates", false, "check the remote AssetIWeave updater manifest")
	return cmd
}

func ptr[T any](value T) *T {
	return &value
}
