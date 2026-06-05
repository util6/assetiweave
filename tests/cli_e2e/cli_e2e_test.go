//go:build e2e

package cli_e2e

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/util6/assetiweave/internal/protocol"
)

type envelope struct {
	OK    bool           `json:"ok"`
	Data  any            `json:"data"`
	Meta  map[string]any `json:"meta"`
	Error map[string]any `json:"error"`
}

func TestVersionReportsCompatibleRealBinaries(t *testing.T) {
	result := runCLI(t, "version")
	if !result.OK {
		t.Fatalf("version failed: %#v", result.Error)
	}
	data, ok := result.Data.(map[string]any)
	if !ok {
		t.Fatalf("version data is not an object: %#v", result.Data)
	}
	if data["compatible"] != true {
		t.Fatalf("version compatibility = %#v", data)
	}
	if data["cli_version"] == "" || data["cli_version"] == "dev" {
		t.Fatalf("CLI version was not injected: %#v", data)
	}
	if data["cli_version"] != data["engine_version"] {
		t.Fatalf("CLI and Engine product versions drifted: %#v", data)
	}
	if data["cli_protocol_version"] != data["engine_protocol_version"] ||
		data["cli_contract_version"] != data["engine_contract_version"] {
		t.Fatalf("CLI and Engine compatibility versions drifted: %#v", data)
	}
}

func TestRealCLIExecutesGeneratedAppCommand(t *testing.T) {
	result := runCLI(t, "app", "list-profiles")
	if !result.OK {
		t.Fatalf("generated App command failed: %#v", result.Error)
	}
	if _, ok := result.Data.([]any); !ok {
		t.Fatalf("list-profiles did not return an array: %#v", result.Data)
	}
	invocation := requireInvocationMeta(t, result)
	if invocation["method"] != "list_profiles" || invocation["outcome"] != "success" {
		t.Fatalf("unexpected invocation meta: %#v", invocation)
	}
}

func TestRealCLIExposesRustTypeDerivedContract(t *testing.T) {
	result := runCLI(t, "schema", "source.add")
	data, ok := result.Data.(map[string]any)
	if !ok {
		t.Fatalf("schema data is not an object: %#v", result.Data)
	}
	paramsSchema, ok := data["params_schema"].(map[string]any)
	if !ok {
		t.Fatalf("params_schema is not an object: %#v", data)
	}
	required, ok := paramsSchema["required"].([]any)
	if !ok || !containsString(required, "kind") || !containsString(required, "include_globs") {
		t.Fatalf("source.add required fields did not come from Rust DTO: %#v", required)
	}
}

func TestRealCLINormalizesRegisteredAliasesBeforeDispatch(t *testing.T) {
	payload, err := json.Marshal(map[string]any{
		"name":         "Alias Source",
		"kind":         "local",
		"rootPath":     t.TempDir(),
		"includeGlobs": []string{"**/SKILL.md"},
		"excludeGlobs": []string{},
		"enabled":      true,
		"priority":     100,
		"dryRun":       true,
	})
	if err != nil {
		t.Fatalf("encode source.add params: %v", err)
	}
	result := runCLI(t, "api", "call", "source.add", "--json", string(payload))
	data, ok := result.Data.(map[string]any)
	if !ok || data["dry_run"] != true {
		t.Fatalf("alias-based source.add failed: %#v", result.Data)
	}
}

func TestRealCLIRejectsHighRiskRawCall(t *testing.T) {
	_, stderr, exitCode := runCLIProcess(t, "api", "call", "delete_source", "--json", `{"id":"missing","dry_run":true}`)
	if exitCode != 10 {
		t.Fatalf("exit code = %d, stderr = %s", exitCode, stderr)
	}
	var result envelope
	if err := json.Unmarshal(stderr, &result); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr)
	}
	if result.Error["type"] != "confirmation_required" {
		t.Fatalf("unexpected error envelope: %#v", result.Error)
	}
	invocation := requireInvocationMeta(t, result)
	if invocation["error_type"] != "confirmation_required" {
		t.Fatalf("unexpected invocation meta: %#v", invocation)
	}
}

func TestRealCLIEnforcesCommandPolicyBeforeDispatch(t *testing.T) {
	policyPath := filepath.Join(t.TempDir(), "policy.json")
	if err := os.WriteFile(policyPath, []byte(`{"version":1,"deny":["source.*"]}`), 0o600); err != nil {
		t.Fatalf("write policy: %v", err)
	}
	_, stderr, exitCode := runCLIProcessWithEnv(
		t,
		[]string{"ASSETIWEAVE_POLICY_PATH=" + policyPath},
		"api", "call", "delete_source", "--json", `{"id":"missing"}`, "--yes",
	)
	if exitCode != 6 {
		t.Fatalf("exit code = %d, stderr = %s", exitCode, stderr)
	}
	var result envelope
	if err := json.Unmarshal(stderr, &result); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr)
	}
	if result.Error["type"] != "command_denied" {
		t.Fatalf("unexpected error envelope: %#v", result.Error)
	}
	invocation := requireInvocationMeta(t, result)
	if invocation["canonical_method"] != "source.remove" || invocation["error_type"] != "command_denied" {
		t.Fatalf("unexpected invocation meta: %#v", invocation)
	}
}

func TestRealCLIRejectsUnknownParamsBeforeDispatch(t *testing.T) {
	_, stderr, exitCode := runCLIProcess(t, "api", "call", "profile.list", "--json", `{"typo":true}`)
	if exitCode != 2 {
		t.Fatalf("exit code = %d, stderr = %s", exitCode, stderr)
	}
	var result envelope
	if err := json.Unmarshal(stderr, &result); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr)
	}
	if result.Error["type"] != "validation" || result.Error["code"] != "invalid_params" {
		t.Fatalf("unexpected error envelope: %#v", result.Error)
	}
}

func TestRealCLIDiagnosticsBypassInvalidPolicy(t *testing.T) {
	policyPath := filepath.Join(t.TempDir(), "policy.json")
	if err := os.WriteFile(policyPath, []byte(`{`), 0o600); err != nil {
		t.Fatalf("write invalid policy: %v", err)
	}
	extraEnv := []string{"ASSETIWEAVE_POLICY_PATH=" + policyPath}
	_, stderr, exitCode := runCLIProcessWithEnv(t, extraEnv, "profile", "list")
	if exitCode != 6 {
		t.Fatalf("invalid policy exit code = %d, stderr = %s", exitCode, stderr)
	}
	result := runCLIWithEnv(t, extraEnv, "version")
	data, ok := result.Data.(map[string]any)
	if !ok || data["compatible"] != true {
		t.Fatalf("version diagnostics failed under invalid policy: %#v", result)
	}
}

func TestRealEngineRejectsMismatchedProtocolWithMeta(t *testing.T) {
	body, err := json.Marshal(map[string]any{
		"id":               "e2e",
		"method":           "profile.list",
		"params":           map[string]any{},
		"protocol_version": protocol.Version + 1,
		"contract_version": protocol.ContractVersion,
	})
	if err != nil {
		t.Fatalf("encode mismatched request: %v", err)
	}
	cmd := exec.Command(enginePath(t))
	cmd.Stdin = bytes.NewReader(body)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if runErr := cmd.Run(); runErr != nil {
		t.Fatalf("Engine process failed: %v\n%s", runErr, stderr.String())
	}

	var result envelope
	if err := json.Unmarshal(stdout.Bytes(), &result); err != nil {
		t.Fatalf("Engine stdout is not JSON: %v\n%s", err, stdout.String())
	}
	if result.OK || result.Error["type"] != "engine_incompatible" {
		t.Fatalf("unexpected Engine mismatch response: %#v", result)
	}
	if result.Meta["protocol_version"] != float64(protocol.Version) ||
		result.Meta["contract_version"] != float64(protocol.ContractVersion) {
		t.Fatalf("Engine response meta is incomplete: %#v", result.Meta)
	}
	invocation := requireInvocationMeta(t, result)
	if invocation["method"] != "profile.list" || invocation["error_type"] != "engine_incompatible" {
		t.Fatalf("unexpected invocation meta: %#v", invocation)
	}
}

func runCLI(t *testing.T, args ...string) envelope {
	t.Helper()
	return runCLIWithEnv(t, nil, args...)
}

func runCLIWithEnv(t *testing.T, extraEnv []string, args ...string) envelope {
	t.Helper()
	stdout, stderr, exitCode := runCLIProcessWithEnv(t, extraEnv, args...)
	if exitCode != 0 {
		t.Fatalf("CLI exit code = %d\nstdout: %s\nstderr: %s", exitCode, stdout, stderr)
	}
	var result envelope
	if err := json.Unmarshal(stdout, &result); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout)
	}
	return result
}

func runCLIProcess(t *testing.T, args ...string) ([]byte, []byte, int) {
	t.Helper()
	return runCLIProcessWithEnv(t, nil, args...)
}

func runCLIProcessWithEnv(t *testing.T, extraEnv []string, args ...string) ([]byte, []byte, int) {
	t.Helper()
	cmd := exec.Command(cliPath(t), args...)
	home := t.TempDir()
	cmd.Env = isolatedEnv(append([]string{
		"ASSETIWEAVE_ENGINE=" + enginePath(t),
		"ASSETIWEAVE_DB_PATH=" + filepath.Join(t.TempDir(), "app.db"),
		"HOME=" + home,
		"USERPROFILE=" + home,
	}, extraEnv...)...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	if err == nil {
		return stdout.Bytes(), stderr.Bytes(), 0
	}
	if exitErr, ok := err.(*exec.ExitError); ok {
		return stdout.Bytes(), stderr.Bytes(), exitErr.ExitCode()
	}
	t.Fatalf("CLI process failed: %v", err)
	return nil, nil, -1
}

func isolatedEnv(overrides ...string) []string {
	keys := map[string]bool{"ASSETIWEAVE_POLICY_PATH": true}
	for _, override := range overrides {
		key, _, _ := strings.Cut(override, "=")
		keys[key] = true
	}
	env := make([]string, 0, len(os.Environ())+len(overrides))
	for _, entry := range os.Environ() {
		key, _, _ := strings.Cut(entry, "=")
		if !keys[key] {
			env = append(env, entry)
		}
	}
	return append(env, overrides...)
}

func requireInvocationMeta(t *testing.T, result envelope) map[string]any {
	t.Helper()
	invocation, ok := result.Meta["invocation"].(map[string]any)
	if !ok {
		t.Fatalf("response missing invocation meta: %#v", result.Meta)
	}
	return invocation
}

func containsString(values []any, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func cliPath(t *testing.T) string {
	t.Helper()
	if path := os.Getenv("ASSETIWEAVE_CLI"); path != "" {
		return path
	}
	return filepath.Join(workspaceRoot(t), "target", buildProfile(), executableName("assetiweave-cli"))
}

func enginePath(t *testing.T) string {
	t.Helper()
	if path := os.Getenv("ASSETIWEAVE_ENGINE"); path != "" {
		return path
	}
	return filepath.Join(workspaceRoot(t), "target", buildProfile(), executableName("assetiweave-engine"))
}

func workspaceRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("failed to resolve E2E test path")
	}
	return filepath.Clean(filepath.Join(filepath.Dir(file), "..", ".."))
}

func executableName(name string) string {
	if runtime.GOOS == "windows" {
		return name + ".exe"
	}
	return name
}

func buildProfile() string {
	if profile := os.Getenv("ASSETIWEAVE_E2E_PROFILE"); profile != "" {
		return profile
	}
	return "debug"
}
