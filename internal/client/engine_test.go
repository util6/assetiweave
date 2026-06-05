package client

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
)

func TestEncodeRequestIncludesCompatibilityVersions(t *testing.T) {
	body, err := encodeRequest("profile.list", map[string]any{})
	if err != nil {
		t.Fatalf("encodeRequest() error = %v", err)
	}

	var request map[string]any
	if err := json.Unmarshal(body, &request); err != nil {
		t.Fatalf("request is not JSON: %v", err)
	}
	if request["protocol_version"] != float64(protocol.Version) {
		t.Fatalf("protocol_version = %#v, want %d", request["protocol_version"], protocol.Version)
	}
	if request["contract_version"] != float64(protocol.ContractVersion) {
		t.Fatalf("contract_version = %#v, want %d", request["contract_version"], protocol.ContractVersion)
	}
}

func TestDecodeResponseAcceptsMatchingCompatibilityMeta(t *testing.T) {
	result, err := decodeResponse([]byte(`{
		"ok": true,
		"data": {"profiles": []},
		"meta": {
			"protocol_version": 1,
			"contract_version": 2,
			"engine_version": "0.1.1",
			"invocation": {
				"method": "profile.list",
				"canonical_method": "profile.list",
				"risk": "read",
				"exposure": "friendly",
				"outcome": "success",
				"hooks": ["runtime.timing"],
				"duration_ms": 2
			}
		}
	}`))
	if err != nil {
		t.Fatalf("decodeResponse() error = %v", err)
	}
	if string(result.Data) != `{"profiles": []}` {
		t.Fatalf("data = %s", result.Data)
	}
	if result.Meta == nil || result.Meta.Invocation == nil ||
		result.Meta.Invocation.Method != "profile.list" ||
		len(result.Meta.Invocation.Hooks) != 1 ||
		result.Meta.Invocation.Hooks[0] != "runtime.timing" {
		t.Fatalf("invocation meta was not preserved: %+v", result.Meta)
	}
}

func TestDecodeResponseRejectsMissingCompatibilityMeta(t *testing.T) {
	_, err := decodeResponse([]byte(`{"ok": true, "data": {}}`))
	assertEngineIncompatible(t, err)
}

func TestDecodeResponseRejectsMismatchedProtocol(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": true,
		"data": {},
		"meta": {
			"protocol_version": 99,
			"contract_version": 2,
			"engine_version": "99.0.0"
		}
	}`))
	problem := assertEngineIncompatible(t, err)
	meta, ok := problem.Meta.(*protocol.EngineMeta)
	if !ok {
		t.Fatalf("problem meta = %#v, want *protocol.EngineMeta", problem.Meta)
	}
	if meta.ProtocolVersion != 99 || meta.EngineVersion != "99.0.0" {
		t.Fatalf("meta = %+v", meta)
	}
}

func TestDecodeResponseRejectsInvalidJSONWithTypedEngineError(t *testing.T) {
	_, err := decodeResponse([]byte(`{`))
	assertTypedProblem(t, err, errs.CategoryEngine, errs.SubtypeEngineProtocol)
}

func TestCallRejectsUnencodableParamsWithTypedValidationError(t *testing.T) {
	client := &EngineClient{Path: "/not-used-after-encode-fails"}

	_, err := client.Call(context.Background(), "profile.list", map[string]any{"bad": make(chan int)})

	assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeInvalidArgument)
}

func TestDecodeVersionResponseAllowsMismatchedProtocolForDiagnostics(t *testing.T) {
	result, err := decodeResponseForMethod("system.version", []byte(`{
		"ok": true,
		"data": {
			"engine_version": "99.0.0",
			"protocol_version": 99,
			"contract_version": 2
		},
		"meta": {
			"protocol_version": 99,
			"contract_version": 2,
			"engine_version": "99.0.0"
		}
	}`))
	if err != nil {
		t.Fatalf("decodeResponseForMethod() error = %v", err)
	}
	if string(result.Data) == "" {
		t.Fatal("version diagnostics data is empty")
	}
}

func TestDecodeResponseMapsConfirmationToAgentProtocolExitCode(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": false,
		"meta": {
			"protocol_version": 1,
			"contract_version": 2,
			"engine_version": "0.1.1"
		},
		"error": {
			"type": "confirmation_required",
			"code": "confirmation_required",
			"message": "confirmation required"
		}
	}`))
	assertExitCode(t, err, output.ExitConfirmationRequired)
}

func TestDecodeResponseMapsCommandDenialToPolicyExitCode(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": false,
		"meta": {
			"protocol_version": 1,
			"contract_version": 2,
			"engine_version": "0.1.1",
			"invocation": {
				"method": "delete_source",
				"outcome": "error",
				"error_type": "command_denied",
				"duration_ms": 0
			}
		},
		"error": {
			"type": "command_denied",
			"code": "command_denied",
			"message": "command denied"
		}
	}`))
	exitErr := assertExitCode(t, err, output.ExitPolicy)
	meta, ok := exitErr.Meta.(*protocol.EngineMeta)
	if !ok || meta.Invocation == nil || meta.Invocation.ErrorType != "command_denied" {
		t.Fatalf("policy error invocation meta was not preserved: %#v", exitErr.Meta)
	}
}

func assertEngineIncompatible(t *testing.T, err error) *errs.Problem {
	t.Helper()
	if err == nil {
		t.Fatal("error = nil, want engine_incompatible")
	}
	problem := assertTypedProblem(t, err, errs.CategoryEngine, errs.SubtypeEngineIncompatible)
	if problem.Code != "engine_incompatible" {
		t.Fatalf("problem code = %q, want engine_incompatible", problem.Code)
	}
	return problem
}

func assertExitCode(t *testing.T, err error, want int) *output.ExitError {
	t.Helper()
	if err == nil {
		t.Fatalf("error = nil, want exit code %d", want)
	}
	var exitErr *output.ExitError
	if !errors.As(err, &exitErr) {
		t.Fatalf("error type = %T, want *output.ExitError", err)
	}
	if exitErr.Code != want {
		t.Fatalf("exit code = %d, want %d", exitErr.Code, want)
	}
	return exitErr
}

func assertTypedProblem(t *testing.T, err error, category errs.Category, subtype errs.Subtype) *errs.Problem {
	t.Helper()
	if err == nil {
		t.Fatalf("error = nil, want %s.%s", category, subtype)
	}
	problem, ok := errs.ProblemOf(err)
	if !ok {
		t.Fatalf("error type = %T, want typed problem", err)
	}
	if problem.Category != category || problem.Subtype != subtype {
		t.Fatalf("problem = %+v, want %s.%s", problem, category, subtype)
	}
	return problem
}
