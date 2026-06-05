package platform

import (
	"encoding/json"
	"testing"
)

func TestPluginConfigTypedAccessorsAndRawCopy(t *testing.T) {
	config := NewPluginConfig(map[string]json.RawMessage{
		"endpoint": json.RawMessage(`"https://example.com"`),
		"enabled":  json.RawMessage(`true`),
		"retries":  json.RawMessage(`3`),
	})

	if value, ok := config.String("endpoint"); !ok || value != "https://example.com" {
		t.Fatalf("endpoint = %q/%v, want configured string", value, ok)
	}
	if value, ok := config.Bool("enabled"); !ok || !value {
		t.Fatalf("enabled = %v/%v, want true", value, ok)
	}
	if value, ok := config.Int("retries"); !ok || value != 3 {
		t.Fatalf("retries = %d/%v, want 3", value, ok)
	}
	raw, ok := config.Raw("endpoint")
	if !ok {
		t.Fatal("Raw(endpoint) missing")
	}
	raw[0] = '{'
	rawAgain, _ := config.Raw("endpoint")
	if string(rawAgain) != `"https://example.com"` {
		t.Fatalf("Raw returned mutable backing slice: %s", rawAgain)
	}
}
