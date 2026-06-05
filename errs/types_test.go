package errs

import (
	"errors"
	"testing"
)

func TestConfigErrorCarriesProblemAndCause(t *testing.T) {
	cause := errors.New("bad json")
	err := NewConfigError(SubtypeInvalidPluginConfig, "plugin config failed: %v", cause).
		WithCode("invalid_plugin_config").
		WithHint("fix the plugin config file").
		WithDetails(map[string]any{"reason_code": "invalid_plugin_config"}).
		WithCause(cause)

	if err.Error() != "plugin config failed: bad json" {
		t.Fatalf("Error() = %q", err.Error())
	}
	if !errors.Is(err, cause) {
		t.Fatal("ConfigError does not unwrap its cause")
	}
	problem, ok := ProblemOf(err)
	if !ok {
		t.Fatal("ProblemOf() did not recognize ConfigError")
	}
	if problem.Category != CategoryConfig ||
		problem.Subtype != SubtypeInvalidPluginConfig ||
		problem.Code != "invalid_plugin_config" ||
		problem.Hint != "fix the plugin config file" {
		t.Fatalf("problem = %+v", problem)
	}
	if CategoryOf(err) != CategoryConfig {
		t.Fatalf("CategoryOf() = %q, want %q", CategoryOf(err), CategoryConfig)
	}
}
