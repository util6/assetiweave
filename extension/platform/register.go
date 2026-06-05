package platform

import "sync"

var plugins = &pluginRegistry{}

type pluginRegistry struct {
	mu      sync.Mutex
	plugins []Plugin
}

func Register(plugin Plugin) {
	plugins.mu.Lock()
	defer plugins.mu.Unlock()
	plugins.plugins = append(plugins.plugins, plugin)
}

func RegisteredPlugins() []Plugin {
	plugins.mu.Lock()
	defer plugins.mu.Unlock()
	return append([]Plugin(nil), plugins.plugins...)
}

func ResetForTesting() {
	plugins.mu.Lock()
	defer plugins.mu.Unlock()
	plugins.plugins = nil
}
