package internalplatform

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/util6/assetiweave/extension/platform"
)

func TestLoadPluginConfigReadsPerPluginObjects(t *testing.T) {
	path := filepath.Join(t.TempDir(), "plugins.json")
	if err := os.WriteFile(path, []byte(`{
		"plugins": {
			"audit": {
				"endpoint": "https://example.com",
				"enabled": true
			}
		}
	}`), 0o600); err != nil {
		t.Fatalf("write config: %v", err)
	}

	store, err := LoadPluginConfig(path)

	if err != nil {
		t.Fatalf("LoadPluginConfig() error = %v", err)
	}
	config := store.ForPlugin("audit")
	if value, ok := config.String("endpoint"); !ok || value != "https://example.com" {
		t.Fatalf("endpoint = %q/%v, want configured endpoint", value, ok)
	}
	if keys := store.Keys("audit"); len(keys) != 2 || keys[0] != "enabled" || keys[1] != "endpoint" {
		t.Fatalf("keys = %v, want sorted plugin config keys", keys)
	}
}

func TestInstallAllInjectsPluginConfigAndInventoryRecordsKeys(t *testing.T) {
	store := NewPluginConfigStore(map[string]platform.PluginConfig{
		"audit": platform.NewPluginConfigFromValues(map[string]any{
			"endpoint": "https://example.com",
			"enabled":  true,
		}),
	})
	plugin := &configReadingPlugin{name: "audit"}

	result, err := InstallAllWithOptions([]platform.Plugin{plugin}, nil, WithPluginConfig(store))

	if err != nil {
		t.Fatalf("InstallAll() error = %v", err)
	}
	if plugin.endpoint != "https://example.com" {
		t.Fatalf("plugin endpoint = %q, want injected config", plugin.endpoint)
	}
	inventory := BuildInventory(result.Plugins, result.Registry, result.PluginRules)
	if keys := inventory.Plugins[0].ConfigKeys; len(keys) != 2 || keys[0] != "enabled" || keys[1] != "endpoint" {
		t.Fatalf("inventory config keys = %v", keys)
	}
}

type configReadingPlugin struct {
	name     string
	endpoint string
}

func (p *configReadingPlugin) Name() string    { return p.name }
func (p *configReadingPlugin) Version() string { return "0.1.0" }
func (p *configReadingPlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{FailurePolicy: platform.FailClosed}
}
func (p *configReadingPlugin) Install(registrar platform.Registrar) error {
	p.endpoint, _ = registrar.Config().String("endpoint")
	return nil
}
