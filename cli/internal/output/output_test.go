package output

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/util6/assetiweave/errs"
)

func TestWriteTypedErrorEnvelopePreservesLegacyEnvelopeShape(t *testing.T) {
	typed := errs.NewConfigError(errs.SubtypeInvalidPluginConfig, "invalid plugin config").
		WithCode("invalid_plugin_config").
		WithHint("fix the plugin config file").
		WithDetails(map[string]any{"reason_code": "invalid_plugin_config"})
	var stderr bytes.Buffer

	if ok := WriteTypedErrorEnvelope(&stderr, typed); !ok {
		t.Fatal("WriteTypedErrorEnvelope() = false")
	}

	var envelope ErrorEnvelope
	if err := json.Unmarshal(stderr.Bytes(), &envelope); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr.String())
	}
	if envelope.OK || envelope.Error == nil {
		t.Fatalf("envelope = %+v", envelope)
	}
	if envelope.Error.Type != "config" ||
		envelope.Error.Subtype != "invalid_plugin_config" ||
		envelope.Error.Code != "invalid_plugin_config" ||
		envelope.Error.Message != "invalid plugin config" ||
		envelope.Error.Hint != "fix the plugin config file" {
		t.Fatalf("error detail = %+v", envelope.Error)
	}
	details, ok := envelope.Error.Details.(map[string]any)
	if !ok || details["reason_code"] != "invalid_plugin_config" {
		t.Fatalf("details = %#v", envelope.Error.Details)
	}
}

func TestWriteSuccessInjectsPendingNotice(t *testing.T) {
	previous := PendingNotice
	PendingNotice = func() map[string]any {
		return map[string]any{"update": map[string]any{"latest": "0.2.0"}}
	}
	t.Cleanup(func() { PendingNotice = previous })
	var stdout bytes.Buffer

	WriteSuccess(&stdout, map[string]any{"value": true})

	var envelope map[string]any
	if err := json.Unmarshal(stdout.Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout.String())
	}
	notice, ok := envelope["_notice"].(map[string]any)
	if !ok {
		t.Fatalf("envelope missing _notice: %#v", envelope)
	}
	update, ok := notice["update"].(map[string]any)
	if !ok || update["latest"] != "0.2.0" {
		t.Fatalf("update notice = %#v", notice["update"])
	}
}

func TestWriteTypedErrorEnvelopePreservesMeta(t *testing.T) {
	typed := errs.NewEngineError(errs.SubtypeEngineIncompatible, "incompatible engine").
		WithCode("engine_incompatible").
		WithMeta(map[string]any{"engine_version": "99.0.0"})
	var stderr bytes.Buffer

	if ok := WriteTypedErrorEnvelope(&stderr, typed); !ok {
		t.Fatal("WriteTypedErrorEnvelope() = false")
	}

	var envelope ErrorEnvelope
	if err := json.Unmarshal(stderr.Bytes(), &envelope); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr.String())
	}
	if envelope.Meta == nil {
		t.Fatalf("meta was not preserved: %+v", envelope)
	}
	meta := envelope.Meta.(map[string]any)
	if meta["engine_version"] != "99.0.0" {
		t.Fatalf("meta = %#v", envelope.Meta)
	}
}

func TestWriteTypedErrorEnvelopeInjectsPendingNotice(t *testing.T) {
	previous := PendingNotice
	PendingNotice = func() map[string]any {
		return map[string]any{"update": map[string]any{"latest": "0.2.0"}}
	}
	t.Cleanup(func() { PendingNotice = previous })
	typed := errs.NewValidationError(errs.SubtypeInvalidArgument, "invalid argument")
	var stderr bytes.Buffer

	if ok := WriteTypedErrorEnvelope(&stderr, typed); !ok {
		t.Fatal("WriteTypedErrorEnvelope() = false")
	}

	var envelope map[string]any
	if err := json.Unmarshal(stderr.Bytes(), &envelope); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr.String())
	}
	notice, ok := envelope["_notice"].(map[string]any)
	if !ok {
		t.Fatalf("envelope missing _notice: %#v", envelope)
	}
	if _, ok := notice["update"]; !ok {
		t.Fatalf("notice missing update: %#v", notice)
	}
}

func TestWriteTypedErrorEnvelopeUsesExplicitWireType(t *testing.T) {
	typed := errs.NewPolicyError(errs.SubtypeCommandDenied, "command denied").
		WithCode("command_denied").
		WithWireType("command_denied")
	var stderr bytes.Buffer

	if ok := WriteTypedErrorEnvelope(&stderr, typed); !ok {
		t.Fatal("WriteTypedErrorEnvelope() = false")
	}

	var envelope ErrorEnvelope
	if err := json.Unmarshal(stderr.Bytes(), &envelope); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr.String())
	}
	if envelope.Error.Type != "command_denied" ||
		envelope.Error.Subtype != "command_denied" {
		t.Fatalf("error detail = %+v", envelope.Error)
	}
}

func TestExitCodeOfTypedError(t *testing.T) {
	err := errs.NewConfigError(errs.SubtypeInvalidConfig, "invalid config")

	if got := ExitCodeOf(err); got != ExitValidation {
		t.Fatalf("ExitCodeOf() = %d, want %d", got, ExitValidation)
	}
}

func TestExitCodeOfTypedEngineError(t *testing.T) {
	err := errs.NewEngineError(errs.SubtypeEngineProtocol, "invalid engine response")

	if got := ExitCodeOf(err); got != ExitEngine {
		t.Fatalf("ExitCodeOf() = %d, want %d", got, ExitEngine)
	}
}

func TestWriteTypedErrorEnvelopeReturnsFalseWhenEncodingFails(t *testing.T) {
	typed := errs.NewConfigError(errs.SubtypeInvalidConfig, "invalid config").
		WithDetails(map[string]any{"bad": make(chan int)})
	var stderr bytes.Buffer

	if ok := WriteTypedErrorEnvelope(&stderr, typed); ok {
		t.Fatal("WriteTypedErrorEnvelope() = true, want false")
	}
	if stderr.Len() != 0 {
		t.Fatalf("stderr = %q, want empty so caller can fall back", stderr.String())
	}
}
