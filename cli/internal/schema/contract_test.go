package schema

import (
	"testing"

	"github.com/util6/assetiweave/internal/protocol"
)

func TestEmbeddedContractHasOneCommandPerMethod(t *testing.T) {
	contract := MustContract()
	if contract.ProtocolVersion != protocol.Version {
		t.Fatalf("protocol version = %d, want %d", contract.ProtocolVersion, protocol.Version)
	}
	if contract.ContractVersion != protocol.ContractVersion {
		t.Fatalf("contract version = %d, want %d", contract.ContractVersion, protocol.ContractVersion)
	}
	if len(contract.Methods) != len(contract.Commands) {
		t.Fatalf("methods = %d, commands = %d", len(contract.Methods), len(contract.Commands))
	}

	seen := map[string]bool{}
	for _, command := range contract.Commands {
		if seen[command.Method] {
			t.Fatalf("duplicate command method %q", command.Method)
		}
		seen[command.Method] = true
	}
}

func TestEmbeddedContractExposesHighRiskConfirmation(t *testing.T) {
	command, ok := Lookup("delete_source")
	if !ok {
		t.Fatal("delete_source contract not found")
	}
	if command.Risk != "high-risk-write" || !command.ConfirmationRequired {
		t.Fatalf("unexpected delete_source risk contract: %+v", command)
	}
	property, ok := command.ParamsSchema.Properties["yes"]
	if !ok || property.BaseType() != "boolean" {
		t.Fatalf("delete_source yes property = %+v", property)
	}
}

func TestEmbeddedContractUsesRustRequestTypeSchema(t *testing.T) {
	sourceAdd, ok := Lookup("source.add")
	if !ok {
		t.Fatal("source.add contract not found")
	}
	for _, field := range []string{"name", "kind", "root_path", "include_globs", "exclude_globs", "enabled", "priority"} {
		if !sourceAdd.ParamsSchema.IsRequired(field) {
			t.Fatalf("source.add omitted required Rust request field %q", field)
		}
	}

	setMount, ok := Lookup("set_asset_mount")
	if !ok {
		t.Fatal("set_asset_mount contract not found")
	}
	strategies := setMount.ParamsSchema.Properties["strategy"].Enum
	if !contains(strategies, "render") || !contains(strategies, "config_merge") {
		t.Fatalf("deployment strategy enum drifted from Rust type: %#v", strategies)
	}

	createProfile, ok := Lookup("create_profile")
	if !ok {
		t.Fatal("create_profile contract not found")
	}
	input := createProfile.ParamsSchema.Properties["input"]
	if _, ok := input.Properties["target_paths"]; !ok {
		t.Fatalf("nested Rust request schema was not preserved: %#v", input.Properties)
	}
}

func contains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}
