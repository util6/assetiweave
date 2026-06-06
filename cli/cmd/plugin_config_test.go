package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/extension/platform"
)

func TestBuildInternalLoadsPluginConfigFromEnvironmentPath(t *testing.T) {
	path := filepath.Join(t.TempDir(), "plugins.json")
	if err := os.WriteFile(path, []byte(`{
		"plugins": {
			"configured": {
				"endpoint": "https://example.com",
				"token": "secret"
			}
		}
	}`), 0o600); err != nil {
		t.Fatalf("write plugin config: %v", err)
	}
	t.Setenv("ASSETIWEAVE_CLI_PLUGIN_CONFIG", path)
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &cliConfigReadingPlugin{}
	platform.Register(plugin)
	factory := testPluginFactory(&recordingClient{})

	buildInternal(context.Background(), factory)

	if plugin.endpoint != "https://example.com" {
		t.Fatalf("plugin endpoint = %q, want config from env path", plugin.endpoint)
	}
}

func TestBuildInternalLoadsPluginConfigFromFactoryPathBeforeCommandParse(t *testing.T) {
	envPath := filepath.Join(t.TempDir(), "env-plugins.json")
	if err := os.WriteFile(envPath, []byte(`{
		"plugins": {
			"configured": {
				"endpoint": "https://env.example.com"
			}
		}
	}`), 0o600); err != nil {
		t.Fatalf("write env plugin config: %v", err)
	}
	flagPath := filepath.Join(t.TempDir(), "flag-plugins.json")
	if err := os.WriteFile(flagPath, []byte(`{
		"plugins": {
			"configured": {
				"endpoint": "https://flag.example.com"
			}
		}
	}`), 0o600); err != nil {
		t.Fatalf("write flag plugin config: %v", err)
	}
	t.Setenv("ASSETIWEAVE_CLI_PLUGIN_CONFIG", envPath)
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &cliConfigReadingPlugin{}
	platform.Register(plugin)
	factory := testPluginFactory(&recordingClient{})
	factory.PluginConfigPath = flagPath

	buildInternal(context.Background(), factory)

	if plugin.endpoint != "https://flag.example.com" {
		t.Fatalf("plugin endpoint = %q, want config from factory path", plugin.endpoint)
	}
}

func TestConfigPluginsShowIncludesConfigKeysOnly(t *testing.T) {
	path := filepath.Join(t.TempDir(), "plugins.json")
	if err := os.WriteFile(path, []byte(`{
		"plugins": {
			"configured": {
				"endpoint": "https://example.com",
				"token": "secret"
			}
		}
	}`), 0o600); err != nil {
		t.Fatalf("write plugin config: %v", err)
	}
	t.Setenv("ASSETIWEAVE_CLI_PLUGIN_CONFIG", path)
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	platform.Register(&cliConfigReadingPlugin{})
	factory := testPluginFactory(&recordingClient{})
	root, _ := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"config", "plugins", "show"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}

	var envelope struct {
		Data struct {
			Plugins []struct {
				Name       string   `json:"name"`
				ConfigKeys []string `json:"config_keys"`
			} `json:"plugins"`
		} `json:"data"`
	}
	if err := json.Unmarshal(factory.IOStreams.Out.(*bytes.Buffer).Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v", err)
	}
	if len(envelope.Data.Plugins) != 1 || envelope.Data.Plugins[0].Name != "configured" {
		t.Fatalf("plugins = %+v", envelope.Data.Plugins)
	}
	keys := envelope.Data.Plugins[0].ConfigKeys
	if len(keys) != 2 || keys[0] != "endpoint" || keys[1] != "token" {
		t.Fatalf("config keys = %v, want keys only", keys)
	}
	if strings.Contains(factory.IOStreams.Out.(*bytes.Buffer).String(), "secret") {
		t.Fatalf("plugin inventory leaked config value: %s", factory.IOStreams.Out.(*bytes.Buffer).String())
	}
}

func TestInvalidPluginConfigDoesNotBlockWhenNoPluginsAreRegistered(t *testing.T) {
	path := filepath.Join(t.TempDir(), "plugins.json")
	if err := os.WriteFile(path, []byte(`{`), 0o600); err != nil {
		t.Fatalf("write plugin config: %v", err)
	}
	t.Setenv("ASSETIWEAVE_CLI_PLUGIN_CONFIG", path)
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	client := &recordingClient{}
	factory := testPluginFactory(client)
	root, registry := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if registry != nil {
		t.Fatal("registry = non-nil with no registered plugins")
	}
	if client.method != "overview.get" {
		t.Fatalf("engine method = %q, want overview.get", client.method)
	}
}

func TestInvalidPluginConfigBlocksRegisteredPlugins(t *testing.T) {
	path := filepath.Join(t.TempDir(), "plugins.json")
	if err := os.WriteFile(path, []byte(`{`), 0o600); err != nil {
		t.Fatalf("write plugin config: %v", err)
	}
	t.Setenv("ASSETIWEAVE_CLI_PLUGIN_CONFIG", path)
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &installTrackingPlugin{}
	platform.Register(plugin)
	client := &recordingClient{}
	factory := testPluginFactory(client)
	root, registry := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	err := root.Execute()

	if registry != nil {
		t.Fatal("registry = non-nil after invalid plugin config")
	}
	if plugin.installed {
		t.Fatal("plugin install ran after invalid plugin config")
	}
	if client.method != "" {
		t.Fatalf("engine method = %q, want no call", client.method)
	}
	assertTypedConfigError(t, err, errs.SubtypeInvalidPluginConfig)
}

func TestCompletionBootstrapBypassesInvalidPluginConfigWithRegisteredPlugins(t *testing.T) {
	path := filepath.Join(t.TempDir(), "plugins.json")
	if err := os.WriteFile(path, []byte(`{`), 0o600); err != nil {
		t.Fatalf("write plugin config: %v", err)
	}
	t.Setenv("ASSETIWEAVE_CLI_PLUGIN_CONFIG", path)
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &installTrackingPlugin{}
	platform.Register(plugin)
	factory := testPluginFactory(&recordingClient{})

	root, registry := buildInternalWithOptions(context.Background(), factory, buildOptions{SkipRuntime: true})
	root.SetArgs([]string{"completion", "bash"})

	if err := root.Execute(); err != nil {
		t.Fatalf("completion failed under invalid plugin config: %v", err)
	}
	if registry != nil {
		t.Fatal("registry = non-nil for completion bootstrap")
	}
	if plugin.installed {
		t.Fatal("plugin install ran for completion bootstrap")
	}
	if factory.IOStreams.Out.(*bytes.Buffer).Len() == 0 {
		t.Fatal("completion output is empty")
	}
}

type cliConfigReadingPlugin struct {
	endpoint string
}

func (p *cliConfigReadingPlugin) Name() string    { return "configured" }
func (p *cliConfigReadingPlugin) Version() string { return "0.1.0" }
func (p *cliConfigReadingPlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{FailurePolicy: platform.FailClosed}
}
func (p *cliConfigReadingPlugin) Install(registrar platform.Registrar) error {
	p.endpoint, _ = registrar.Config().String("endpoint")
	return nil
}

type installTrackingPlugin struct {
	installed bool
}

func (p *installTrackingPlugin) Name() string    { return "tracked" }
func (p *installTrackingPlugin) Version() string { return "0.1.0" }
func (p *installTrackingPlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{FailurePolicy: platform.FailClosed}
}
func (p *installTrackingPlugin) Install(platform.Registrar) error {
	p.installed = true
	return nil
}

func assertTypedConfigError(t *testing.T, err error, want errs.Subtype) {
	t.Helper()
	var configErr *errs.ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("error = %#v, want *errs.ConfigError", err)
	}
	problem, ok := errs.ProblemOf(err)
	if !ok {
		t.Fatalf("ProblemOf(%#v) = false", err)
	}
	if problem.Category != errs.CategoryConfig || problem.Subtype != want {
		t.Fatalf("problem = %+v, want config.%s", problem, want)
	}
}
