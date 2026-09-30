package client

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
)

func TestEngineClientCancellationInterruptsEngineBeforeForcedTermination(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("signal forwarding fixture requires a POSIX shell")
	}

	tempDir := t.TempDir()
	markerPath := filepath.Join(tempDir, "marker")
	enginePath := filepath.Join(tempDir, "fake-engine")
	script := "#!/bin/sh\n" +
		"trap 'printf interrupted > \"" + markerPath + "\"; exit 130' INT TERM\n" +
		"printf started > \"" + markerPath + "\"\n" +
		"while :; do sleep 0.05; done\n"
	if err := os.WriteFile(enginePath, []byte(script), 0o700); err != nil {
		t.Fatalf("write fake Engine: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() {
		_, err := NewEngineClient(enginePath).Call(ctx, "profile.list", map[string]any{})
		done <- err
	}()

	deadline := time.Now().Add(2 * time.Second)
	for {
		marker, err := os.ReadFile(markerPath)
		if err == nil && string(marker) == "started" {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("fake Engine did not start")
		}
		time.Sleep(10 * time.Millisecond)
	}
	cancel()

	select {
	case err := <-done:
		if err == nil {
			t.Fatal("Call() error = nil after cancellation")
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Call() did not return after cancellation")
	}

	marker, err := os.ReadFile(markerPath)
	if err != nil {
		t.Fatalf("read cancellation marker: %v", err)
	}
	if string(marker) != "interrupted" {
		t.Fatalf("marker = %q, want graceful interrupt", marker)
	}
}

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
			"contract_version": 3,
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
			"contract_version": 3,
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

func TestEngineClientCommandEnvironmentIncludesPolicyOverride(t *testing.T) {
	client := NewEngineClient("/bin/echo")
	client.PolicyPath = "/tmp/policy.json"

	env := client.commandEnv()

	if env["ASSETIWEAVE_POLICY_PATH"] != "/tmp/policy.json" {
		t.Fatalf("ASSETIWEAVE_POLICY_PATH = %q, want policy override", env["ASSETIWEAVE_POLICY_PATH"])
	}
}

func TestDecodeVersionResponseAllowsMismatchedProtocolForDiagnostics(t *testing.T) {
	result, err := decodeResponseForMethod("system.version", []byte(`{
		"ok": true,
		"data": {
			"engine_version": "99.0.0",
			"protocol_version": 99,
			"contract_version": 3
		},
		"meta": {
			"protocol_version": 99,
			"contract_version": 3,
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

func TestDecodeResponsePromotesConfirmationAndPreservesAgentProtocol(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": false,
		"meta": {
			"protocol_version": 1,
			"contract_version": 3,
			"engine_version": "0.1.1"
		},
		"error": {
			"type": "confirmation_required",
			"code": "confirmation_required",
			"message": "confirmation required"
		}
	}`))
	problem := assertTypedProblem(t, err, errs.CategoryConfirmation, errs.SubtypeConfirmationRequired)
	if output.ExitCodeOf(err) != output.ExitConfirmationRequired ||
		problem.WireType != "confirmation_required" ||
		problem.Meta == nil {
		t.Fatalf("problem = %+v", problem)
	}
}

func TestDecodeResponsePromotesCommandDenialAndPreservesAgentProtocol(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": false,
		"meta": {
			"protocol_version": 1,
			"contract_version": 3,
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
	problem := assertTypedProblem(t, err, errs.CategoryPolicy, errs.SubtypeCommandDenied)
	if output.ExitCodeOf(err) != output.ExitPolicy ||
		problem.WireType != "command_denied" {
		t.Fatalf("problem = %+v", problem)
	}
	meta, ok := problem.Meta.(*protocol.EngineMeta)
	if !ok || meta.Invocation == nil || meta.Invocation.ErrorType != "command_denied" {
		t.Fatalf("policy error invocation meta was not preserved: %#v", problem.Meta)
	}
}

func TestDecodeResponsePromotesValidationByErrorCode(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": false,
		"meta": {
			"protocol_version": 1,
			"contract_version": 3,
			"engine_version": "0.1.1"
		},
		"error": {
			"type": "validation",
			"code": "invalid_params",
			"message": "invalid params",
			"hint": "inspect schema",
			"details": {"method": "profile.list"}
		}
	}`))
	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeInvalidParams)
	if problem.WireType != "validation" ||
		problem.Code != "invalid_params" ||
		problem.Hint != "inspect schema" ||
		problem.Details == nil {
		t.Fatalf("problem = %+v", problem)
	}
}

func TestDecodeResponsePromotesAppBusinessError(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": false,
		"meta": {
			"protocol_version": 1,
			"contract_version": 3,
			"engine_version": "0.1.1"
		},
		"error": {
			"type": "not_found",
			"code": "not_found",
			"message": "profile not found"
		}
	}`))
	problem := assertTypedProblem(t, err, errs.CategoryEngine, errs.SubtypeNotFound)
	if output.ExitCodeOf(err) != output.ExitEngine ||
		problem.WireType != "not_found" {
		t.Fatalf("problem = %+v", problem)
	}
}

func TestDecodeResponsePromotesUnknownEngineErrorWithoutChangingWireType(t *testing.T) {
	_, err := decodeResponse([]byte(`{
		"ok": false,
		"meta": {
			"protocol_version": 1,
			"contract_version": 3,
			"engine_version": "0.1.1"
		},
		"error": {
			"type": "future_error",
			"code": "future_code",
			"message": "future failure"
		}
	}`))
	problem := assertTypedProblem(t, err, errs.CategoryEngine, errs.SubtypeEngineReturnedError)
	if problem.WireType != "future_error" || problem.Code != "future_code" {
		t.Fatalf("problem = %+v", problem)
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

func TestFindWorkspaceEngineFromWorkspaceRootAndSubdirectory(t *testing.T) {
	wsDir := t.TempDir()
	pkgJSON := filepath.Join(wsDir, "package.json")
	if err := os.WriteFile(pkgJSON, []byte(`{"name":"assetiweave"}`), 0o600); err != nil {
		t.Fatalf("write package.json: %v", err)
	}

	targetDir := filepath.Join(wsDir, "target", "debug")
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		t.Fatalf("mkdir target/debug: %v", err)
	}
	engineFile := filepath.Join(targetDir, executableName("assetiweave-engine"))
	if err := os.WriteFile(engineFile, []byte("binary"), 0o755); err != nil {
		t.Fatalf("write fake engine: %v", err)
	}

	// 1. From workspace root
	found := findWorkspaceEngineFrom(wsDir)
	if found != engineFile {
		t.Fatalf("findWorkspaceEngineFrom(root) = %q, want %q", found, engineFile)
	}

	// 2. From nested subdirectory
	subDir := filepath.Join(wsDir, "cli", "internal", "client")
	if err := os.MkdirAll(subDir, 0o755); err != nil {
		t.Fatalf("mkdir subDir: %v", err)
	}
	foundSub := findWorkspaceEngineFrom(subDir)
	if foundSub != engineFile {
		t.Fatalf("findWorkspaceEngineFrom(subDir) = %q, want %q", foundSub, engineFile)
	}

	// 3. Non-workspace dir returns empty
	otherDir := t.TempDir()
	if foundOther := findWorkspaceEngineFrom(otherDir); foundOther != "" {
		t.Fatalf("findWorkspaceEngineFrom(otherDir) = %q, want empty", foundOther)
	}
}

func TestResolvePathPrioritizesWorkspaceEngineOverLookPath(t *testing.T) {
	wsDir := t.TempDir()
	pkgJSON := filepath.Join(wsDir, "package.json")
	if err := os.WriteFile(pkgJSON, []byte(`{"name":"assetiweave"}`), 0o600); err != nil {
		t.Fatalf("write package.json: %v", err)
	}

	targetDir := filepath.Join(wsDir, "target", "debug")
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		t.Fatalf("mkdir target/debug: %v", err)
	}
	workspaceEngine := filepath.Join(targetDir, executableName("assetiweave-engine"))
	if err := os.WriteFile(workspaceEngine, []byte("workspace-engine"), 0o755); err != nil {
		t.Fatalf("write fake engine: %v", err)
	}

	fakeBinDir := t.TempDir()
	globalEngine := filepath.Join(fakeBinDir, executableName("assetiweave-engine"))
	if err := os.WriteFile(globalEngine, []byte("global-engine"), 0o755); err != nil {
		t.Fatalf("write fake global engine: %v", err)
	}

	originalWD, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	if err := os.Chdir(wsDir); err != nil {
		t.Fatalf("chdir wsDir: %v", err)
	}
	defer func() { _ = os.Chdir(originalWD) }()

	t.Setenv("PATH", fakeBinDir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Setenv("ASSETIWEAVE_ENGINE", "")

	client := &EngineClient{}
	resolved, err := client.resolvePath()
	if err != nil {
		t.Fatalf("resolvePath() error = %v", err)
	}
	realResolved, err := filepath.EvalSymlinks(resolved)
	if err != nil {
		t.Fatalf("EvalSymlinks(resolved): %v", err)
	}
	realWorkspaceEngine, err := filepath.EvalSymlinks(workspaceEngine)
	if err != nil {
		t.Fatalf("EvalSymlinks(workspaceEngine): %v", err)
	}
	if realResolved != realWorkspaceEngine {
		t.Fatalf("resolvePath() = %q, want workspace engine %q (not global %q)", realResolved, realWorkspaceEngine, globalEngine)
	}
}

func TestCallTranslatesMigrationMissingError(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("mock shell script requires POSIX shell")
	}

	tempDir := t.TempDir()
	fakeEngine := filepath.Join(tempDir, "fake-outdated-engine")
	script := "#!/bin/sh\n" +
		"echo 'failed to initialize Engine AppRuntime: migration 202609260001 was previously applied but is missing in the resolved migrations' >&2\n" +
		"exit 1\n"
	if err := os.WriteFile(fakeEngine, []byte(script), 0o755); err != nil {
		t.Fatalf("write fake engine script: %v", err)
	}

	client := NewEngineClient(fakeEngine)
	_, err := client.Call(context.Background(), "profile.list", map[string]any{})
	if err == nil {
		t.Fatal("Call() error = nil, want migration error")
	}

	problem := assertTypedProblem(t, err, errs.CategoryEngine, errs.SubtypeEngineProcess)
	if problem.Code != "engine_migration_outdated" {
		t.Fatalf("problem.Code = %q, want 'engine_migration_outdated'", problem.Code)
	}
	if !strings.Contains(problem.Hint, "cargo build -p assetiweave --bin assetiweave-engine or pnpm cli:install") {
		t.Fatalf("problem.Hint = %q, want hint to mention cargo build and pnpm cli:install", problem.Hint)
	}
}

func TestResolvePathDevelopmentModeRequiresWorkspaceEngineAndRefusesGlobalFallback(t *testing.T) {
	wsDir := t.TempDir()
	pkgJSON := filepath.Join(wsDir, "package.json")
	if err := os.WriteFile(pkgJSON, []byte(`{"name":"assetiweave"}`), 0o600); err != nil {
		t.Fatalf("write package.json: %v", err)
	}

	fakeBinDir := t.TempDir()
	globalEngine := filepath.Join(fakeBinDir, executableName("assetiweave-engine"))
	if err := os.WriteFile(globalEngine, []byte("global-engine"), 0o755); err != nil {
		t.Fatalf("write fake global engine: %v", err)
	}

	originalWD, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	if err := os.Chdir(wsDir); err != nil {
		t.Fatalf("chdir wsDir: %v", err)
	}
	defer func() { _ = os.Chdir(originalWD) }()

	t.Setenv("PATH", fakeBinDir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Setenv("ASSETIWEAVE_ENGINE", "")
	t.Setenv("ASSETIWEAVE_ENV", "development")

	client := &EngineClient{}

	// Case 1: In development mode, without target/debug/assetiweave-engine, must NOT fallback to globalEngine!
	_, err = client.resolvePath()
	if err == nil {
		t.Fatal("resolvePath() succeeded in development mode without workspace engine; want error refusing global fallback")
	}
	if !strings.Contains(err.Error(), "development engine not found in target/debug/assetiweave-engine") {
		t.Fatalf("resolvePath() error = %v, want error indicating development engine missing", err)
	}

	// Case 2: When target/debug/assetiweave-engine exists in workspace, returns it
	targetDir := filepath.Join(wsDir, "target", "debug")
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		t.Fatalf("mkdir target/debug: %v", err)
	}
	wsEngine := filepath.Join(targetDir, executableName("assetiweave-engine"))
	if err := os.WriteFile(wsEngine, []byte("ws-engine"), 0o755); err != nil {
		t.Fatalf("write ws engine: %v", err)
	}

	resolved, err := client.resolvePath()
	if err != nil {
		t.Fatalf("resolvePath() unexpected error: %v", err)
	}
	realResolved, _ := filepath.EvalSymlinks(resolved)
	realWSEngine, _ := filepath.EvalSymlinks(wsEngine)
	if realResolved != realWSEngine {
		t.Fatalf("resolvePath() = %q, want workspace engine %q", realResolved, realWSEngine)
	}
}

func TestResolvePathInstalledModeIgnoresWorkspaceEngine(t *testing.T) {
	wsDir := t.TempDir()
	pkgJSON := filepath.Join(wsDir, "package.json")
	if err := os.WriteFile(pkgJSON, []byte(`{"name":"assetiweave"}`), 0o600); err != nil {
		t.Fatalf("write package.json: %v", err)
	}
	targetDir := filepath.Join(wsDir, "target", "debug")
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		t.Fatalf("mkdir target/debug: %v", err)
	}
	wsEngine := filepath.Join(targetDir, executableName("assetiweave-engine"))
	if err := os.WriteFile(wsEngine, []byte("ws-engine"), 0o755); err != nil {
		t.Fatalf("write ws engine: %v", err)
	}

	fakeBinDir := t.TempDir()
	globalEngine := filepath.Join(fakeBinDir, executableName("assetiweave-engine"))
	if err := os.WriteFile(globalEngine, []byte("global-engine"), 0o755); err != nil {
		t.Fatalf("write fake global engine: %v", err)
	}

	originalWD, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	if err := os.Chdir(wsDir); err != nil {
		t.Fatalf("chdir wsDir: %v", err)
	}
	defer func() { _ = os.Chdir(originalWD) }()

	t.Setenv("PATH", fakeBinDir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Setenv("ASSETIWEAVE_ENGINE", "")
	t.Setenv("ASSETIWEAVE_ENV", "production")

	client := &EngineClient{}
	resolved, err := client.resolvePath()
	if err != nil {
		t.Fatalf("resolvePath() error = %v", err)
	}
	realResolved, _ := filepath.EvalSymlinks(resolved)
	realGlobal, _ := filepath.EvalSymlinks(globalEngine)
	if realResolved != realGlobal {
		t.Fatalf("resolvePath() in production mode = %q, want installed engine %q (not workspace %q)", realResolved, realGlobal, wsEngine)
	}
}

func TestFindWorkspaceCLIFindsWorkspaceBinaries(t *testing.T) {
	wsDir := t.TempDir()
	pkgJSON := filepath.Join(wsDir, "package.json")
	if err := os.WriteFile(pkgJSON, []byte(`{"name":"assetiweave"}`), 0o600); err != nil {
		t.Fatalf("write package.json: %v", err)
	}
	targetDir := filepath.Join(wsDir, "target", "debug")
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		t.Fatalf("mkdir target/debug: %v", err)
	}
	cliFile := filepath.Join(targetDir, executableName("aiwc"))
	if err := os.WriteFile(cliFile, []byte("fake-cli"), 0o755); err != nil {
		t.Fatalf("write fake cli: %v", err)
	}

	found := findWorkspaceCLIFrom(wsDir)
	if found != cliFile {
		t.Fatalf("findWorkspaceCLIFrom(wsDir) = %q, want %q", found, cliFile)
	}
}


