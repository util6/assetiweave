package cmd

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/util6/assetiweave/internal/schema"
)

const agentInstallPreviewFixture = `{
  "catalogVersion": "2026.08.1",
  "targetVersion": "1.2.3",
  "selectedDistribution": {"distributionId": "system"},
  "previewToken": "preview-token",
  "conflicts": []
}`

func TestAgentInstallWithoutYesOnlyPrintsPreview(t *testing.T) {
	client := &recordingClient{data: json.RawMessage(agentInstallPreviewFixture)}
	root := Build(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"agent", "install", "fixture-agent"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodAgentInstallPreview {
		t.Fatalf("method = %q, want %q", client.method, schema.MethodAgentInstallPreview)
	}
}

func TestAgentInstallYesCallsConfirmedEngineRun(t *testing.T) {
	client := &recordingClient{data: json.RawMessage(agentInstallPreviewFixture)}
	root := Build(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"agent", "install", "fixture-agent", "--yes"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodAgentInstallRun {
		t.Fatalf("method = %q, want %q", client.method, schema.MethodAgentInstallRun)
	}
	params, ok := client.params.(map[string]any)
	if !ok || params["yes"] != true {
		t.Fatalf("params = %#v, want explicit Engine confirmation", client.params)
	}
}

func TestAgentCheckDefaultsToBoundedRuntimeMethod(t *testing.T) {
	client := &recordingClient{}
	root := Build(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"agent", "check", "fixture-agent"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodAgentRuntimeCheck {
		t.Fatalf("method = %q, want %q", client.method, schema.MethodAgentRuntimeCheck)
	}
}
