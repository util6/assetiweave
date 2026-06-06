package protocol

const (
	Version         = 1
	ContractVersion = 2
)

// CLI release fields are replaced by the build scripts for distributable binaries.
var (
	CLIVersion     = "dev"
	CLIBuildCommit = "unknown"
	CLIBuiltAt     = "unknown"
	CLIBuildSource = "source"
)

type CLIRelease struct {
	Version string `json:"version"`
	Commit  string `json:"commit"`
	BuiltAt string `json:"built_at"`
	Source  string `json:"source"`
	Channel string `json:"channel"`
}

func CLIReleaseInfo() CLIRelease {
	version := CLIVersion
	if version == "" {
		version = "dev"
	}
	source := CLIBuildSource
	if source == "" {
		source = "source"
	}
	return CLIRelease{
		Version: version,
		Commit:  valueOrUnknown(CLIBuildCommit),
		BuiltAt: valueOrUnknown(CLIBuiltAt),
		Source:  source,
		Channel: releaseChannel(version),
	}
}

func releaseChannel(version string) string {
	if version == "" || version == "dev" {
		return "dev"
	}
	return "release"
}

func valueOrUnknown(value string) string {
	if value == "" {
		return "unknown"
	}
	return value
}

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
