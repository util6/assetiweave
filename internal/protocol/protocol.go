package protocol

const (
	Version         = 1
	ContractVersion = 2
)

// CLIVersion is replaced by the build scripts for distributable binaries.
var CLIVersion = "dev"

type EngineMeta struct {
	ProtocolVersion int             `json:"protocol_version"`
	ContractVersion int             `json:"contract_version"`
	EngineVersion   string          `json:"engine_version"`
	Invocation      *InvocationMeta `json:"invocation,omitempty"`
}

type InvocationMeta struct {
	Method          string   `json:"method"`
	CanonicalMethod string   `json:"canonical_method,omitempty"`
	Risk            string   `json:"risk,omitempty"`
	Exposure        string   `json:"exposure,omitempty"`
	Outcome         string   `json:"outcome"`
	ErrorType       string   `json:"error_type,omitempty"`
	Hooks           []string `json:"hooks,omitempty"`
	DurationMS      uint64   `json:"duration_ms"`
}

type EngineVersion struct {
	Product         string   `json:"product"`
	EngineVersion   string   `json:"engine_version"`
	ProtocolVersion int      `json:"protocol_version"`
	ContractVersion int      `json:"contract_version"`
	Capabilities    []string `json:"capabilities"`
}

func Compatible(protocolVersion, contractVersion int) bool {
	return protocolVersion == Version && contractVersion == ContractVersion
}
