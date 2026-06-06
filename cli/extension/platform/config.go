package platform

import (
	"encoding/json"
	"sort"
)

type PluginConfig struct {
	values map[string]json.RawMessage
}

func NewPluginConfig(values map[string]json.RawMessage) PluginConfig {
	cloned := make(map[string]json.RawMessage, len(values))
	for key, value := range values {
		cloned[key] = append(json.RawMessage(nil), value...)
	}
	return PluginConfig{values: cloned}
}

func NewPluginConfigFromValues(values map[string]any) PluginConfig {
	raw := make(map[string]json.RawMessage, len(values))
	for key, value := range values {
		encoded, err := json.Marshal(value)
		if err == nil {
			raw[key] = encoded
		}
	}
	return NewPluginConfig(raw)
}

func (c PluginConfig) Raw(key string) (json.RawMessage, bool) {
	value, ok := c.values[key]
	if !ok {
		return nil, false
	}
	return append(json.RawMessage(nil), value...), true
}

func (c PluginConfig) String(key string) (string, bool) {
	var value string
	if !c.decode(key, &value) {
		return "", false
	}
	return value, true
}

func (c PluginConfig) Bool(key string) (bool, bool) {
	var value bool
	if !c.decode(key, &value) {
		return false, false
	}
	return value, true
}

func (c PluginConfig) Int(key string) (int, bool) {
	var value int
	if !c.decode(key, &value) {
		return 0, false
	}
	return value, true
}

func (c PluginConfig) Keys() []string {
	keys := make([]string, 0, len(c.values))
	for key := range c.values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func (c PluginConfig) decode(key string, target any) bool {
	raw, ok := c.values[key]
	if !ok {
		return false
	}
	return json.Unmarshal(raw, target) == nil
}
