package cmd

import (
	"os"
	"path/filepath"

	"github.com/util6/assetiweave/internal/cmdutil"
	internalplatform "github.com/util6/assetiweave/internal/platform"
)

const pluginConfigEnv = "ASSETIWEAVE_CLI_PLUGIN_CONFIG"

func loadPluginConfig(f *cmdutil.Factory) (*internalplatform.PluginConfigStore, error) {
	return internalplatform.LoadPluginConfig(pluginConfigPath(f))
}

func pluginConfigPath(f *cmdutil.Factory) string {
	if f != nil && f.PluginConfigPath != "" {
		return f.PluginConfigPath
	}
	if path := os.Getenv(pluginConfigEnv); path != "" {
		return path
	}
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return ""
	}
	return filepath.Join(home, ".assetiweave-cli", "plugins.json")
}
