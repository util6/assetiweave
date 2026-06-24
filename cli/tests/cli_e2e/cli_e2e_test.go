//go:build e2e

package cli_e2e

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/util6/assetiweave/internal/protocol"
)

type envelope struct {
	OK     bool           `json:"ok"`
	Data   any            `json:"data"`
	Meta   map[string]any `json:"meta"`
	Error  map[string]any `json:"error"`
	Notice map[string]any `json:"_notice"`
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
	release, ok := data["cli_release"].(map[string]any)
	if !ok {
		t.Fatalf("version missing cli_release diagnostics: %#v", data)
	}
	if release["version"] != data["cli_version"] ||
		release["source"] != "script" ||
		release["channel"] != "release" ||
		release["commit"] == "" ||
		release["built_at"] == "" {
		t.Fatalf("CLI release diagnostics are incomplete: %#v", release)
	}
}

func TestGlobalEngineFlagOverridesInvalidEnvironment(t *testing.T) {
	result := runCLIWithEnv(t, []string{"ASSETIWEAVE_ENGINE=" + filepath.Join(t.TempDir(), "missing-engine")}, "--engine", enginePath(t), "version")
	if !result.OK {
		t.Fatalf("version with --engine failed: %#v", result.Error)
	}
	data, ok := result.Data.(map[string]any)
	if !ok || data["compatible"] != true {
		t.Fatalf("version compatibility = %#v", result.Data)
	}
}

func TestGlobalPolicyFlagOverridesInvalidEnvironment(t *testing.T) {
	dir := t.TempDir()
	badPolicy := filepath.Join(dir, "bad-policy.json")
	if err := os.WriteFile(badPolicy, []byte(`{`), 0o600); err != nil {
		t.Fatalf("write bad policy: %v", err)
	}
	goodPolicy := filepath.Join(dir, "good-policy.json")
	if err := os.WriteFile(goodPolicy, []byte(`{"version":1}`), 0o600); err != nil {
		t.Fatalf("write good policy: %v", err)
	}

	result := runCLIWithEnv(t, []string{"ASSETIWEAVE_POLICY_PATH=" + badPolicy}, "--policy", goodPolicy, "profile", "list")

	if !result.OK {
		t.Fatalf("profile list with --policy failed: %#v", result.Error)
	}
}

func TestVersionCheckUpdatesReadsRemoteManifest(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"99.0.0"}`))
	}))
	t.Cleanup(server.Close)

	result := runCLIWithEnv(t, []string{"ASSETIWEAVE_UPDATE_MANIFEST_URL=" + server.URL}, "version", "--check-updates")

	data, ok := result.Data.(map[string]any)
	if !ok {
		t.Fatalf("version data is not an object: %#v", result.Data)
	}
	update, ok := data["update"].(map[string]any)
	if !ok ||
		update["checked"] != true ||
		update["available"] != true ||
		update["latest"] != "99.0.0" {
		t.Fatalf("update diagnostics = %#v", update)
	}
}

func TestCachedUpdateNoticeIncludedInSuccessEnvelope(t *testing.T) {
	statePath := filepath.Join(t.TempDir(), "update-state.json")
	state := `{"latest_version":"99.0.0","checked_at":` + strconv.FormatInt(time.Now().Unix(), 10) + `}`
	if err := os.WriteFile(statePath, []byte(state), 0o600); err != nil {
		t.Fatalf("write update state: %v", err)
	}

	result := runCLIWithEnv(t, []string{
		"ASSETIWEAVE_UPDATE_STATE_PATH=" + statePath,
		"ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER=",
		"CI=",
	}, "profile", "list")

	updateNotice, ok := result.Notice["update"].(map[string]any)
	if !ok ||
		updateNotice["latest"] != "99.0.0" ||
		updateNotice["current"] == "" ||
		updateNotice["message"] == "" ||
		updateNotice["command"] == "" {
		t.Fatalf("update notice = %#v", result.Notice)
	}
}

func TestUpdateCheckReadsRemoteManifest(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"99.0.0"}`))
	}))
	t.Cleanup(server.Close)

	result := runCLIWithEnv(t, []string{
		"ASSETIWEAVE_UPDATE_MANIFEST_URL=" + server.URL,
		"ASSETIWEAVE_UPDATE_STATE_PATH=" + filepath.Join(t.TempDir(), "update-state.json"),
	}, "update", "--check")

	data, ok := result.Data.(map[string]any)
	if !ok {
		t.Fatalf("update data is not an object: %#v", result.Data)
	}
	if data["checked"] != true ||
		data["update_available"] != true ||
		data["latest"] != "99.0.0" ||
		data["action"] != "app_update_required" ||
		data["package_url"] != nil ||
		data["checksum_url"] != nil {
		t.Fatalf("update check result = %#v", data)
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

func TestRealCLISettingsShortcuts(t *testing.T) {
	show := runCLI(t, "settings", "show")
	showData, ok := show.Data.(map[string]any)
	if !ok ||
		showData["config_path"] == "" ||
		showData["conversation_adapter_dir"] == "" {
		t.Fatalf("settings show data = %#v", show.Data)
	}
	if _, ok := showData["settings"].(map[string]any); !ok {
		t.Fatalf("settings show did not return an object: %#v", showData["settings"])
	}

	save := runCLI(t, "settings", "save", "--json", `{"density":"compact"}`)
	saveData, ok := save.Data.(map[string]any)
	if !ok {
		t.Fatalf("settings save data = %#v", save.Data)
	}
	settings, ok := saveData["settings"].(map[string]any)
	if !ok || settings["density"] != "compact" {
		t.Fatalf("settings save did not persist provided settings: %#v", saveData["settings"])
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

func TestRealCLIClassifiesCobraUsageErrors(t *testing.T) {
	cases := []struct {
		name    string
		args    []string
		subtype string
	}{
		{
			name:    "unknown nested command",
			args:    []string{"source", "lst"},
			subtype: "unknown_command",
		},
		{
			name:    "unknown flag",
			args:    []string{"source", "add", "--name", "demo", "--path", t.TempDir(), "--dry-rnu"},
			subtype: "unknown_flag",
		},
		{
			name:    "missing required flag",
			args:    []string{"source", "add", "--name", "demo"},
			subtype: "missing_required_flag",
		},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			stdout, stderr, exitCode := runCLIProcess(t, testCase.args...)
			if exitCode != 2 {
				t.Fatalf("exit code = %d, stdout = %s, stderr = %s", exitCode, stdout, stderr)
			}
			if len(bytes.TrimSpace(stdout)) != 0 {
				t.Fatalf("stdout = %s, want empty", stdout)
			}
			var result envelope
			if err := json.Unmarshal(stderr, &result); err != nil {
				t.Fatalf("stderr is not JSON: %v\n%s", err, stderr)
			}
			if result.Error["type"] != "validation" ||
				result.Error["subtype"] != testCase.subtype ||
				result.Error["code"] != testCase.subtype {
				t.Fatalf("unexpected error envelope: %#v", result.Error)
			}
		})
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
	baseEnv := []string{
		"ASSETIWEAVE_ENGINE=" + enginePath(t),
		"ASSETIWEAVE_DB_PATH=" + filepath.Join(t.TempDir(), "app.db"),
		"HOME=" + home,
		"USERPROFILE=" + home,
	}
	if !hasEnvOverride(extraEnv, "ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER") {
		baseEnv = append(baseEnv, "ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER=1")
	}
	cmd.Env = isolatedEnv(append(baseEnv, extraEnv...)...)
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

func hasEnvOverride(values []string, key string) bool {
	for _, value := range values {
		name, _, _ := strings.Cut(value, "=")
		if name == key {
			return true
		}
	}
	return false
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
	return filepath.Clean(filepath.Join(filepath.Dir(file), "..", "..", ".."))
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
