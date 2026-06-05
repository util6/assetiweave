package cmd

import (
	"os"
	"path/filepath"

	internalplatform "github.com/util6/assetiweave/internal/platform"
)

const pluginConfigEnv = "ASSETIWEAVE_CLI_PLUGIN_CONFIG"

func loadPluginConfig() (*internalplatform.PluginConfigStore, error) {
	return internalplatform.LoadPluginConfig(pluginConfigPath())
}

func pluginConfigPath() string {
	if path := os.Getenv(pluginConfigEnv); path != "" {
		return path
	}
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return ""
	}
	return filepath.Join(home, ".assetiweave-cli", "plugins.json")
}
