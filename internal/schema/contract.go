package schema

import (
	_ "embed"
	"encoding/json"
	"fmt"
	"sync"

	"github.com/util6/assetiweave/internal/protocol"
)

//go:embed contract.json
var embeddedContract []byte

type Contract struct {
	ProtocolVersion int           `json:"protocol_version"`
	ContractVersion int           `json:"contract_version"`
	EngineVersion   string        `json:"engine_version"`
	Methods         []string      `json:"methods"`
	Commands        []CommandSpec `json:"commands"`
}

type CommandSpec struct {
	CanonicalMethod      string       `json:"canonical_method"`
	CLI                  *string      `json:"cli"`
	ConfirmationRequired bool         `json:"confirmation_required"`
	ContractVersion      int          `json:"contract_version"`
	Deprecated           bool         `json:"deprecated"`
	Description          string       `json:"description"`
	Exposure             string       `json:"exposure"`
	Method               string       `json:"method"`
	ParamsSchema         ObjectSchema `json:"params_schema"`
	Risk                 string       `json:"risk"`
	Since                string       `json:"since"`
	SupportsDryRun       bool         `json:"supports_dry_run"`
}

type ObjectSchema struct {
	AdditionalProperties bool                      `json:"additionalProperties"`
	Properties           map[string]PropertySchema `json:"properties"`
	Required             []string                  `json:"required"`
	Type                 string                    `json:"type"`
}

type PropertySchema struct {
	AdditionalProperties json.RawMessage           `json:"additionalProperties,omitempty"`
	Aliases              []string                  `json:"aliases,omitempty"`
	Default              json.RawMessage           `json:"default,omitempty"`
	Description          string                    `json:"description"`
	Enum                 []string                  `json:"enum,omitempty"`
	Format               string                    `json:"format,omitempty"`
	Items                *PropertySchema           `json:"items,omitempty"`
	Minimum              *float64                  `json:"minimum,omitempty"`
	Properties           map[string]PropertySchema `json:"properties,omitempty"`
	Required             []string                  `json:"required,omitempty"`
	Type                 json.RawMessage           `json:"type"`
}

func (p PropertySchema) BaseType() string {
	var single string
	if json.Unmarshal(p.Type, &single) == nil {
		return single
	}
	var choices []string
	if json.Unmarshal(p.Type, &choices) == nil {
		for _, choice := range choices {
			if choice != "null" {
				return choice
			}
		}
	}
	return ""
}

func (s ObjectSchema) IsRequired(name string) bool {
	for _, required := range s.Required {
		if required == name {
			return true
		}
	}
	return false
}

var (
	contractOnce  sync.Once
	contractValue *Contract
	contractErr   error
)

func LoadContract() (*Contract, error) {
	contractOnce.Do(func() {
		var contract Contract
		if err := json.Unmarshal(embeddedContract, &contract); err != nil {
			contractErr = fmt.Errorf("parse embedded command contract: %w", err)
			return
		}
		if contract.ProtocolVersion != protocol.Version {
			contractErr = fmt.Errorf("unsupported engine protocol version: %d", contract.ProtocolVersion)
			return
		}
		if contract.ContractVersion != protocol.ContractVersion {
			contractErr = fmt.Errorf("unsupported command contract version: %d", contract.ContractVersion)
			return
		}
		if len(contract.Methods) != len(contract.Commands) {
			contractErr = fmt.Errorf("command contract method count does not match command count")
			return
		}
		seen := make(map[string]bool, len(contract.Commands))
		for _, command := range contract.Commands {
			if command.Method == "" {
				contractErr = fmt.Errorf("command contract contains an empty method")
				return
			}
			if seen[command.Method] {
				contractErr = fmt.Errorf("command contract contains duplicate method %q", command.Method)
				return
			}
			seen[command.Method] = true
		}
		contractValue = &contract
	})
	return contractValue, contractErr
}

func MustContract() *Contract {
	contract, err := LoadContract()
	if err != nil {
		panic(err)
	}
	return contract
}

func Lookup(method string) (CommandSpec, bool) {
	for _, command := range MustContract().Commands {
		if command.Method == method {
			return command, true
		}
	}
	return CommandSpec{}, false
}

func AppCommands() []CommandSpec {
	commands := make([]CommandSpec, 0)
	for _, command := range MustContract().Commands {
		if command.Exposure == "app" {
			commands = append(commands, command)
		}
	}
	return commands
}
