package internalplatform

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sort"

	"github.com/util6/assetiweave/extension/platform"
)

type PluginConfigStore struct {
	plugins map[string]platform.PluginConfig
}

func NewPluginConfigStore(configs map[string]platform.PluginConfig) *PluginConfigStore {
	store := &PluginConfigStore{plugins: make(map[string]platform.PluginConfig, len(configs))}
	for name, config := range configs {
		store.plugins[name] = platform.NewPluginConfigFromValues(rawConfigValues(config))
	}
	return store
}

func EmptyPluginConfigStore() *PluginConfigStore {
	return &PluginConfigStore{plugins: map[string]platform.PluginConfig{}}
}

func LoadPluginConfig(path string) (*PluginConfigStore, error) {
	if path == "" {
		return EmptyPluginConfigStore(), nil
	}
	bytes, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return EmptyPluginConfigStore(), nil
		}
		return nil, fmt.Errorf("read plugin config %q: %w", path, err)
	}
	var document struct {
		Plugins map[string]map[string]json.RawMessage `json:"plugins"`
	}
	if err := json.Unmarshal(bytes, &document); err != nil {
		return nil, fmt.Errorf("parse plugin config %q: %w", path, err)
	}
	configs := make(map[string]platform.PluginConfig, len(document.Plugins))
	for name, values := range document.Plugins {
		configs[name] = platform.NewPluginConfig(values)
	}
	return &PluginConfigStore{plugins: configs}, nil
}

func (s *PluginConfigStore) ForPlugin(name string) platform.PluginConfig {
	if s == nil {
		return platform.PluginConfig{}
	}
	config, ok := s.plugins[name]
	if !ok {
		return platform.PluginConfig{}
	}
	return platform.NewPluginConfigFromValues(rawConfigValues(config))
}

func (s *PluginConfigStore) Keys(name string) []string {
	if s == nil {
		return nil
	}
	config, ok := s.plugins[name]
	if !ok {
		return nil
	}
	keys := config.Keys()
	sort.Strings(keys)
	return keys
}

func rawConfigValues(config platform.PluginConfig) map[string]any {
	values := map[string]any{}
	for _, key := range config.Keys() {
		raw, ok := config.Raw(key)
		if !ok {
			continue
		}
		var decoded any
		if json.Unmarshal(raw, &decoded) == nil {
			values[key] = decoded
		}
	}
	return values
}
