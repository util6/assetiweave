package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"

	"github.com/util6/assetiweave/extension/platform"
)

func TestConfigPluginsShowOutputsInstalledPluginInventory(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := platform.NewPlugin("audit", "0.1.0").
		Observer(platform.Before, "before", platform.All(), func(context.Context, platform.Invocation) {}).
		Restrict(&platform.Rule{Name: "read", MaxRisk: platform.RiskRead}).
		MustBuild()
	platform.Register(plugin)
	factory := testPluginFactory(&recordingClient{})
	root, _ := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"config", "plugins", "show"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}

	var envelope struct {
		OK   bool `json:"ok"`
		Data struct {
			Plugins []struct {
				Name      string `json:"name"`
				Version   string `json:"version"`
				Observers []struct {
					Name string `json:"name"`
				} `json:"observers"`
				Rules []struct {
					Name    string `json:"name"`
					MaxRisk string `json:"max_risk"`
				} `json:"rules"`
			} `json:"plugins"`
		} `json:"data"`
	}
	if err := json.Unmarshal(factory.IOStreams.Out.(*bytes.Buffer).Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v", err)
	}
	if !envelope.OK || len(envelope.Data.Plugins) != 1 {
		t.Fatalf("envelope = %+v", envelope)
	}
	got := envelope.Data.Plugins[0]
	if got.Name != "audit" || got.Version != "0.1.0" || len(got.Observers) != 1 || len(got.Rules) != 1 {
		t.Fatalf("plugin inventory = %+v", got)
	}
}
