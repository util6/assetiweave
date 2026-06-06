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

func TestConfirmationAndInternalConstructorsLockCategory(t *testing.T) {
	confirmation := NewConfirmationRequiredError("confirmation required").
		WithCode("confirmation_required").
		WithHint("pass --yes")
	if problem, ok := ProblemOf(confirmation); !ok ||
		problem.Category != CategoryConfirmation ||
		problem.Subtype != SubtypeConfirmationRequired ||
		problem.Hint != "pass --yes" {
		t.Fatalf("confirmation problem = %+v", problem)
	}

	cause := errors.New("plain failure")
	internal := NewInternalError(SubtypeUnknown, "internal failure: %v", cause).
		WithCode("internal").
		WithCause(cause)
	if problem, ok := ProblemOf(internal); !ok ||
		problem.Category != CategoryInternal ||
		problem.Subtype != SubtypeUnknown {
		t.Fatalf("internal problem = %+v", problem)
	}
	if !errors.Is(internal, cause) {
		t.Fatal("InternalError does not unwrap its cause")
	}
}
