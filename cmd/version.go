package cmd

import (
	"encoding/json"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
	"github.com/util6/assetiweave/internal/schema"
)

type versionReport struct {
	CLIVersion            string   `json:"cli_version"`
	EngineVersion         string   `json:"engine_version"`
	CLIProtocolVersion    int      `json:"cli_protocol_version"`
	EngineProtocolVersion int      `json:"engine_protocol_version"`
	CLIContractVersion    int      `json:"cli_contract_version"`
	EngineContractVersion int      `json:"engine_contract_version"`
	Compatible            bool     `json:"compatible"`
	Capabilities          []string `json:"capabilities"`
}

func newCmdVersion(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Show CLI and Engine compatibility versions",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			result, err := f.Client.Call(cmd.Context(), schema.MethodSystemVersion, map[string]any{})
			if err != nil {
				return err
			}
			var engine protocol.EngineVersion
			if err := json.Unmarshal(result.Data, &engine); err != nil {
				return output.ErrWithHint(output.ExitEngine, "engine_protocol", "system.version returned invalid data: "+err.Error(), "install a matching AssetIWeave CLI and Engine release")
			}
			if engine.EngineVersion == "" || engine.ProtocolVersion == 0 || engine.ContractVersion == 0 {
				return output.ErrWithHint(output.ExitEngine, "engine_protocol", "system.version returned incomplete compatibility data", "install a matching AssetIWeave CLI and Engine release")
			}
			report := versionReport{
				CLIVersion:            protocol.CLIVersion,
				EngineVersion:         engine.EngineVersion,
				CLIProtocolVersion:    protocol.Version,
				EngineProtocolVersion: engine.ProtocolVersion,
				CLIContractVersion:    protocol.ContractVersion,
				EngineContractVersion: engine.ContractVersion,
				Compatible:            protocol.Compatible(engine.ProtocolVersion, engine.ContractVersion),
				Capabilities:          engine.Capabilities,
			}
			if result.Meta == nil {
				output.WriteSuccess(f.IOStreams.Out, report)
			} else {
				output.WriteSuccessWithMeta(f.IOStreams.Out, report, result.Meta)
			}
			return nil
		},
	}
}
