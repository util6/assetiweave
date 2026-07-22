package harvesters

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
)

func TestListOfficialTemplatesIncludesQwenAndGemini(t *testing.T) {
	templates, err := ListOfficialTemplates(t.TempDir())
	if err != nil {
		t.Fatalf("ListOfficialTemplates() error = %v", err)
	}
	ids := map[string]TemplateSummary{}
	for _, template := range templates {
		ids[template.ID] = template
	}
	for _, id := range []string{"qwen-web", "gemini-web"} {
		if ids[id].Origin != "official" || ids[id].Version == "" {
			t.Fatalf("official template %s missing or incomplete: %#v", id, ids[id])
		}
	}
}

func TestInstallOfficialTemplateWritesUserHarvesterAsset(t *testing.T) {
	root := t.TempDir()
	result, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "gemini-web"})
	if err != nil {
		t.Fatalf("InstallOfficialTemplate() error = %v", err)
	}
	if result.Directory != filepath.Join(root, "gemini-web") {
		t.Fatalf("Directory = %s", result.Directory)
	}
	for _, rel := range []string{
		"harvester.json",
		"conversation-adapter.json",
		"adapter.js",
		"web-harvester.json",
		filepath.Join("requests", "auth-probe.json"),
		filepath.Join("scripts", "harvest.js"),
	} {
		if _, err := os.Stat(filepath.Join(result.Directory, rel)); err != nil {
			t.Fatalf("installed file %s missing: %v", rel, err)
		}
	}
	adapterManifest := readJSONFile(t, filepath.Join(result.Directory, "conversation-adapter.json"))
	runtime, ok := adapterManifest["runtime"].(map[string]any)
	if !ok {
		t.Fatalf("adapter runtime missing: %#v", adapterManifest)
	}
	if runtime["type"] != "node" || runtime["entry"] != "adapter.js" || runtime["version"] != ">=20" {
		t.Fatalf("adapter runtime = %#v", runtime)
	}
	if _, ok := adapterManifest["command"]; ok {
		t.Fatalf("adapter manifest still uses legacy command: %#v", adapterManifest)
	}
	harvesterManifest := readJSONFile(t, filepath.Join(result.Directory, "harvester.json"))
	harvesterRuntime, ok := harvesterManifest["runtime"].(map[string]any)
	if !ok {
		t.Fatalf("harvester runtime missing: %#v", harvesterManifest)
	}
	if harvesterRuntime["type"] != "node" || harvesterRuntime["entry"] != "scripts/harvest.js" || harvesterRuntime["version"] != ">=20" {
		t.Fatalf("harvester runtime = %#v", harvesterRuntime)
	}
	if _, ok := harvesterManifest["entrypoint"]; ok {
		t.Fatalf("harvester manifest still uses legacy entrypoint: %#v", harvesterManifest)
	}
	if _, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "gemini-web"}); err == nil {
		t.Fatal("second install without force succeeded, want overwrite error")
	}
	replaced, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "gemini-web", Force: true})
	if err != nil {
		t.Fatalf("force install error = %v", err)
	}
	if !replaced.Updated {
		t.Fatal("force install Updated = false, want true")
	}
}

func readJSONFile(t *testing.T, path string) map[string]any {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	var value map[string]any
	if err := json.Unmarshal(data, &value); err != nil {
		t.Fatalf("parse %s: %v", path, err)
	}
	return value
}

func TestQwenNormalizerHandlesLegacyAndCurrentResponseSchemas(t *testing.T) {
	node, err := exec.LookPath("node")
	if err != nil {
		t.Skip("node is required to run the Qwen normalizer regression test")
	}
	command := exec.Command(node, "--test", filepath.Join("testdata", "qwen-normalize.test.cjs"))
	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("Qwen normalizer test failed: %v\n%s", err, output)
	}
}

func TestInstallOfficialTemplateCanPreserveStateOnUpdate(t *testing.T) {
	root := t.TempDir()
	result, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "qwen-web"})
	if err != nil {
		t.Fatalf("InstallOfficialTemplate() error = %v", err)
	}
	if _, err := os.Stat(filepath.Join(result.Directory, "scripts", "qwen-normalize.cjs")); err != nil {
		t.Fatalf("installed Qwen normalizer missing: %v", err)
	}
	authPath := filepath.Join(result.Directory, "requests", "auth-probe.json")
	outputPath := filepath.Join(result.Directory, "output", "normalized", "sessions.json")
	if err := os.MkdirAll(filepath.Dir(outputPath), 0o700); err != nil {
		t.Fatalf("mkdir output: %v", err)
	}
	if err := os.WriteFile(authPath, []byte(`{"secret":"keep"}`+"\n"), 0o600); err != nil {
		t.Fatalf("write auth state: %v", err)
	}
	if err := os.WriteFile(outputPath, []byte(`{"sessions":[]}`+"\n"), 0o600); err != nil {
		t.Fatalf("write output state: %v", err)
	}

	_, err = InstallOfficialTemplate(InstallOptions{
		Root:          root,
		ID:            "qwen-web",
		Force:         true,
		PreserveState: true,
	})
	if err != nil {
		t.Fatalf("preserving update error = %v", err)
	}
	authState, err := os.ReadFile(authPath)
	if err != nil {
		t.Fatalf("read auth state: %v", err)
	}
	if string(authState) != `{"secret":"keep"}`+"\n" {
		t.Fatalf("auth state was overwritten: %s", string(authState))
	}
	if _, err := os.Stat(outputPath); err != nil {
		t.Fatalf("output state missing after update: %v", err)
	}
}

func TestPreservingOfficialRepairRejectsInstalledSymlinkEscape(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlink fixture is Unix-specific")
	}
	root := t.TempDir()
	installed, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "qwen-web"})
	if err != nil {
		t.Fatalf("InstallOfficialTemplate() error = %v", err)
	}
	outside := t.TempDir()
	scripts := filepath.Join(installed.Directory, "scripts")
	if err := os.RemoveAll(scripts); err != nil {
		t.Fatalf("remove scripts: %v", err)
	}
	if err := os.Symlink(outside, scripts); err != nil {
		t.Fatalf("symlink scripts: %v", err)
	}

	_, err = InstallOfficialTemplate(InstallOptions{
		Root: root, ID: "qwen-web", Force: true, PreserveState: true,
	})
	if err == nil || !strings.Contains(err.Error(), "symlink") {
		t.Fatalf("preserving repair error = %v, want symlink rejection", err)
	}
	if _, statErr := os.Stat(filepath.Join(outside, "harvest.js")); !os.IsNotExist(statErr) {
		t.Fatalf("preserving repair wrote through symlink: %v", statErr)
	}
}

func TestDoctorAndRepairRestoreOfficialHarvesterWithoutDeletingState(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("runtime fixture is shell-based")
	}
	root := t.TempDir()
	installed, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "qwen-web"})
	if err != nil {
		t.Fatalf("InstallOfficialTemplate() error = %v", err)
	}
	runtimePath := writeFakeRuntime(t, "v22.3.0")
	authPath := filepath.Join(installed.Directory, "requests", "auth-probe.json")
	outputPath := filepath.Join(installed.Directory, "output", "normalized", "sessions.json")
	if err := os.MkdirAll(filepath.Dir(outputPath), 0o700); err != nil {
		t.Fatalf("mkdir output: %v", err)
	}
	if err := os.WriteFile(authPath, []byte(`{"headers":{"Cookie":"keep-private"}}`+"\n"), 0o600); err != nil {
		t.Fatalf("write auth state: %v", err)
	}
	if err := os.WriteFile(outputPath, []byte(`{"sessions":[]}`+"\n"), 0o600); err != nil {
		t.Fatalf("write output state: %v", err)
	}
	tamperedScript := filepath.Join(installed.Directory, "scripts", "harvest.js")
	if err := os.WriteFile(tamperedScript, []byte("tampered\n"), 0o600); err != nil {
		t.Fatalf("tamper script: %v", err)
	}

	diagnosis, err := Doctor(DoctorOptions{
		Root:             root,
		ID:               "qwen-web",
		RuntimeOverrides: RuntimeOverrides{Node: runtimePath},
	})
	if err != nil {
		t.Fatalf("Doctor() error = %v", err)
	}
	if diagnosis.Healthy || !diagnosis.Repairable || !hasDiagnosticCode(diagnosis.Checks, "package_files") {
		t.Fatalf("diagnosis = %#v", diagnosis)
	}

	preview, err := Repair(RepairOptions{
		Root:             root,
		ID:               "qwen-web",
		DryRun:           true,
		RuntimeOverrides: RuntimeOverrides{Node: runtimePath},
	})
	if err != nil {
		t.Fatalf("Repair(dry-run) error = %v", err)
	}
	if !preview.DryRun || preview.Repaired {
		t.Fatalf("preview = %#v", preview)
	}
	if content, _ := os.ReadFile(tamperedScript); string(content) != "tampered\n" {
		t.Fatalf("dry-run changed script: %q", content)
	}

	repaired, err := Repair(RepairOptions{
		Root:             root,
		ID:               "qwen-web",
		RuntimeOverrides: RuntimeOverrides{Node: runtimePath},
	})
	if err != nil {
		t.Fatalf("Repair() error = %v", err)
	}
	if !repaired.Repaired || repaired.After == nil || !repaired.After.Healthy {
		t.Fatalf("repaired = %#v", repaired)
	}
	if content, _ := os.ReadFile(authPath); !strings.Contains(string(content), "keep-private") {
		t.Fatalf("repair overwrote auth state: %s", content)
	}
	if _, err := os.Stat(outputPath); err != nil {
		t.Fatalf("repair deleted normalized output: %v", err)
	}
}

func TestDoctorReportsRuntimeVersionMismatch(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("runtime fixture is shell-based")
	}
	root := t.TempDir()
	if _, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "gemini-web"}); err != nil {
		t.Fatalf("InstallOfficialTemplate() error = %v", err)
	}

	diagnosis, err := Doctor(DoctorOptions{
		Root:             root,
		ID:               "gemini-web",
		RuntimeOverrides: RuntimeOverrides{Node: writeFakeRuntime(t, "v18.20.0")},
	})
	if err != nil {
		t.Fatalf("Doctor() error = %v", err)
	}
	if diagnosis.Healthy || diagnosis.Repairable || !hasDiagnosticCode(diagnosis.Checks, "runtime_version") {
		t.Fatalf("diagnosis = %#v", diagnosis)
	}

	repair, err := Repair(RepairOptions{
		Root: root, ID: "gemini-web", RuntimeOverrides: RuntimeOverrides{Node: writeFakeRuntime(t, "v18.20.0")},
	})
	if err != nil {
		t.Fatalf("Repair() error = %v", err)
	}
	if repair.Repaired || repair.After == nil || repair.After.Healthy {
		t.Fatalf("runtime-only repair must remain an explicit environment action: %#v", repair)
	}
}

func TestDoctorReportsOfficialScriptPermissionDrift(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("executable permission checks are Unix-specific")
	}
	root := t.TempDir()
	installed, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "qwen-web"})
	if err != nil {
		t.Fatalf("InstallOfficialTemplate() error = %v", err)
	}
	script := filepath.Join(installed.Directory, "scripts", "harvest.js")
	if err := os.Chmod(script, 0o600); err != nil {
		t.Fatalf("remove executable permission: %v", err)
	}

	diagnosis, err := Doctor(DoctorOptions{
		Root: root, ID: "qwen-web", RuntimeOverrides: RuntimeOverrides{Node: writeFakeRuntime(t, "v22.3.0")},
	})
	if err != nil {
		t.Fatalf("Doctor() error = %v", err)
	}
	if diagnosis.Healthy || !hasDiagnosticCode(diagnosis.Checks, "package_files") {
		t.Fatalf("diagnosis = %#v", diagnosis)
	}
	repaired, err := Repair(RepairOptions{
		Root: root, ID: "qwen-web", RuntimeOverrides: RuntimeOverrides{Node: writeFakeRuntime(t, "v22.3.0")},
	})
	if err != nil {
		t.Fatalf("Repair() error = %v", err)
	}
	if repaired.After == nil || !repaired.After.Healthy {
		t.Fatalf("repair did not restore executable permission: %#v", repaired)
	}
}

func TestDoctorAndRepairRestoreMissingAuthProbeWithoutReplacingExistingAuth(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("runtime fixture is shell-based")
	}
	root := t.TempDir()
	installed, err := InstallOfficialTemplate(InstallOptions{Root: root, ID: "qwen-web"})
	if err != nil {
		t.Fatalf("InstallOfficialTemplate() error = %v", err)
	}
	authPath := filepath.Join(installed.Directory, "requests", "auth-probe.json")
	if err := os.Remove(authPath); err != nil {
		t.Fatalf("remove auth probe: %v", err)
	}
	overrides := RuntimeOverrides{Node: writeFakeRuntime(t, "v22.3.0")}

	diagnosis, err := Doctor(DoctorOptions{Root: root, ID: "qwen-web", RuntimeOverrides: overrides})
	if err != nil {
		t.Fatalf("Doctor() error = %v", err)
	}
	if !diagnosis.Repairable || !hasDiagnosticCode(diagnosis.Checks, "package_files") {
		t.Fatalf("diagnosis = %#v", diagnosis)
	}
	if _, err := Repair(RepairOptions{Root: root, ID: "qwen-web", RuntimeOverrides: overrides}); err != nil {
		t.Fatalf("Repair() error = %v", err)
	}
	if _, err := os.Stat(authPath); err != nil {
		t.Fatalf("repair did not restore default auth probe: %v", err)
	}
}

func hasDiagnosticCode(checks []DiagnosticCheck, code string) bool {
	for _, check := range checks {
		if check.Code == code && check.Status == DiagnosticFail {
			return true
		}
	}
	return false
}

func writeFakeRuntime(t *testing.T, version string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	if err := os.WriteFile(path, []byte("#!/bin/sh\nprintf '%s\\n' '"+version+"'\n"), 0o700); err != nil {
		t.Fatalf("write fake runtime: %v", err)
	}
	return path
}

func TestInstallPackageFromDirectoryWritesCommunityHarvester(t *testing.T) {
	root := t.TempDir()
	packageDir := writePackage(t, t.TempDir(), "community-web", "community", "0.2.0")

	result, err := InstallPackage(InstallPackageOptions{
		Root:   root,
		ID:     "community-web",
		Source: packageDir,
	})
	if err != nil {
		t.Fatalf("InstallPackage() error = %v", err)
	}
	if result.Origin != "community" || result.Version != "0.2.0" {
		t.Fatalf("result = %#v", result)
	}
	if _, err := os.Stat(filepath.Join(root, "community-web", "scripts", "harvest.sh")); err != nil {
		t.Fatalf("installed script missing: %v", err)
	}
}

func TestInstallPackageFromHTTPZipWritesToRequestedID(t *testing.T) {
	root := t.TempDir()
	archive := makePackageZip(t, "tencent-yuanbao-web", false)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/tencent-yuanbao-web.zip" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "application/zip")
		_, _ = w.Write(archive)
	}))
	t.Cleanup(server.Close)

	result, err := InstallPackage(InstallPackageOptions{
		Root:   root,
		ID:     "tencent-yuanbao-web",
		Source: server.URL + "/tencent-yuanbao-web.zip",
	})
	if err != nil {
		t.Fatalf("InstallPackage() error = %v", err)
	}
	if result.Directory != filepath.Join(root, "tencent-yuanbao-web") {
		t.Fatalf("Directory = %s", result.Directory)
	}
	if _, err := os.Stat(filepath.Join(result.Directory, "harvester.json")); err != nil {
		t.Fatalf("manifest not installed: %v", err)
	}
}

func TestInstallPackageUpdatePreservesState(t *testing.T) {
	root := t.TempDir()
	packageDir := writePackage(t, t.TempDir(), "community-web", "community", "0.1.0")
	result, err := InstallPackage(InstallPackageOptions{Root: root, ID: "community-web", Source: packageDir})
	if err != nil {
		t.Fatalf("InstallPackage() error = %v", err)
	}
	authPath := filepath.Join(result.Directory, "requests", "auth-probe.json")
	outputPath := filepath.Join(result.Directory, "output", "normalized", "sessions.json")
	if err := os.MkdirAll(filepath.Dir(outputPath), 0o700); err != nil {
		t.Fatalf("mkdir output: %v", err)
	}
	if err := os.WriteFile(authPath, []byte(`{"secret":"keep"}`+"\n"), 0o600); err != nil {
		t.Fatalf("write auth state: %v", err)
	}
	if err := os.WriteFile(outputPath, []byte(`{"sessions":[]}`+"\n"), 0o600); err != nil {
		t.Fatalf("write output state: %v", err)
	}
	updatedPackage := writePackage(t, t.TempDir(), "community-web", "community", "0.2.0")

	updated, err := InstallPackage(InstallPackageOptions{
		Root:          root,
		ID:            "community-web",
		Source:        updatedPackage,
		Force:         true,
		PreserveState: true,
	})
	if err != nil {
		t.Fatalf("InstallPackage preserving update error = %v", err)
	}
	if updated.Version != "0.2.0" {
		t.Fatalf("updated version = %s", updated.Version)
	}
	authState, err := os.ReadFile(authPath)
	if err != nil {
		t.Fatalf("read auth state: %v", err)
	}
	if string(authState) != `{"secret":"keep"}`+"\n" {
		t.Fatalf("auth state was overwritten: %s", string(authState))
	}
}

func TestInstallPackageRejectsZipPathTraversal(t *testing.T) {
	root := t.TempDir()
	archive := makePackageZip(t, "bad-web", true)
	archivePath := filepath.Join(t.TempDir(), "bad-web.zip")
	if err := os.WriteFile(archivePath, archive, 0o600); err != nil {
		t.Fatalf("write archive: %v", err)
	}

	if _, err := InstallPackage(InstallPackageOptions{Root: root, ID: "bad-web", Source: archivePath}); err == nil {
		t.Fatal("InstallPackage() error = nil, want unsafe archive path error")
	}
}

func TestExtractPackageZipRejectsWindowsReservedFileNames(t *testing.T) {
	archive := makeZipEntries(t, []zipTestEntry{{Name: "package/CON.txt", Content: "reserved"}})

	err := extractPackageZip(archive, t.TempDir())

	if err == nil || !strings.Contains(err.Error(), "reserved on Windows") {
		t.Fatalf("extractPackageZip() error = %v, want Windows reserved name error", err)
	}
}

func TestExtractPackageZipRejectsCaseInsensitivePathCollisions(t *testing.T) {
	archive := makeZipEntries(t, []zipTestEntry{
		{Name: "Package/Adapter.js", Content: "first"},
		{Name: "package/adapter.js", Content: "second"},
	})

	err := extractPackageZip(archive, t.TempDir())

	if err == nil || !strings.Contains(err.Error(), "colliding paths") {
		t.Fatalf("extractPackageZip() error = %v, want case-insensitive collision error", err)
	}
}

func TestPortableArchiveValidationRejectsTooManyEntries(t *testing.T) {
	names := make([]string, maxPackageFiles+1)
	for index := range names {
		names[index] = fmt.Sprintf("package/file-%04d.txt", index)
	}

	err := validatePortableArchivePathNames(names)

	if err == nil || !strings.Contains(err.Error(), "too many entries") {
		t.Fatalf("validatePortableArchivePathNames() error = %v, want entry limit error", err)
	}
}

func TestRunExecutesRelativeEntrypoint(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "demo-web")
	if err := os.MkdirAll(filepath.Join(dir, "scripts"), 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	manifest := Manifest{
		SchemaVersion: 1,
		ID:            "demo-web",
		Name:          "Demo Web",
		Version:       "0.1.0",
		Origin:        "community",
		Entrypoint:    []string{"scripts/harvest.sh"},
		Output: OutputSpec{
			NormalizedDir: "output/normalized",
			SessionsFile:  "sessions.json",
		},
	}
	data, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		t.Fatalf("marshal manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "harvester.json"), append(data, '\n'), 0o600); err != nil {
		t.Fatalf("write manifest: %v", err)
	}
	script := "#!/bin/sh\nprintf '%s\\n' \"{\\\"ok\\\":true,\\\"id\\\":\\\"$ASSETIWEAVE_HARVESTER_ID\\\"}\"\n"
	if err := os.WriteFile(filepath.Join(dir, "scripts", "harvest.sh"), []byte(script), 0o700); err != nil {
		t.Fatalf("write script: %v", err)
	}

	result, err := Run(RunOptions{Root: root, ID: "demo-web"})
	if err != nil {
		t.Fatalf("Run() error = %v", err)
	}
	if result.ExitCode != 0 || result.ID != "demo-web" {
		t.Fatalf("result = %#v", result)
	}
	resultJSON, ok := result.Result.(map[string]any)
	if !ok || resultJSON["id"] != "demo-web" {
		t.Fatalf("parsed result = %#v", result.Result)
	}
	if result.NormalizedFile != filepath.Join(dir, "output", "normalized", "sessions.json") {
		t.Fatalf("NormalizedFile = %s", result.NormalizedFile)
	}
}

func TestRunExposesCurrentCLIPathToHarvester(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("entrypoint fixture is shell-based")
	}
	root := t.TempDir()
	dir := filepath.Join(root, "repairable-web")
	if err := os.MkdirAll(filepath.Join(dir, "scripts"), 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	manifest := Manifest{
		SchemaVersion: 1,
		ID:            "repairable-web",
		Name:          "Repairable Web",
		Version:       "0.1.0",
		Origin:        "community",
		Entrypoint:    []string{"scripts/harvest.sh"},
	}
	data, _ := json.Marshal(manifest)
	if err := os.WriteFile(filepath.Join(dir, "harvester.json"), data, 0o600); err != nil {
		t.Fatalf("write manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "scripts", "harvest.sh"), []byte("#!/bin/sh\nprintf '{\"cli_path\":\"%s\"}\\n' \"$ASSETIWEAVE_CLI_PATH\"\n"), 0o700); err != nil {
		t.Fatalf("write entrypoint: %v", err)
	}

	result, err := Run(RunOptions{Root: root, ID: "repairable-web", CLIPath: "/Applications/AssetIWeave/assetiweave-cli"})
	if err != nil {
		t.Fatalf("Run() error = %v", err)
	}
	parsed, ok := result.Result.(map[string]any)
	if !ok || parsed["cli_path"] != "/Applications/AssetIWeave/assetiweave-cli" {
		t.Fatalf("Result = %#v", result.Result)
	}
}

func TestResolveEntrypointInvocationRunsJavaScriptThroughNode(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "demo-web")
	script := filepath.Join(dir, "scripts", "harvest.js")
	if err := os.MkdirAll(filepath.Dir(script), 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(script, []byte("\n"), 0o600); err != nil {
		t.Fatalf("write script: %v", err)
	}

	invocation, err := resolveEntrypointInvocation(dir, []string{"scripts/harvest.js", "--once"}, RuntimeOverrides{})
	if err != nil {
		t.Fatalf("resolveEntrypointInvocation() error = %v", err)
	}

	if invocation.Program != "node" {
		t.Fatalf("Program = %q, want node", invocation.Program)
	}
	if got, want := invocation.Args, []string{script, "--once"}; !slices.Equal(got, want) {
		t.Fatalf("Args = %#v, want %#v", got, want)
	}
}

func TestResolveRuntimeInvocationRunsDeclaredNodeRuntime(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "demo-web")
	script := filepath.Join(dir, "scripts", "harvest.mjs")
	if err := os.MkdirAll(filepath.Dir(script), 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(script, []byte("\n"), 0o600); err != nil {
		t.Fatalf("write script: %v", err)
	}

	invocation, err := resolveHarvesterInvocation(dir, Manifest{
		ID: "demo-web",
		Runtime: &RuntimeSpec{
			Type:    "node",
			Entry:   "scripts/harvest.mjs",
			Args:    []string{"--once"},
			Version: ">=20",
		},
	}, RuntimeOverrides{})
	if err != nil {
		t.Fatalf("resolveHarvesterInvocation() error = %v", err)
	}

	if invocation.Program != "node" {
		t.Fatalf("Program = %q, want node", invocation.Program)
	}
	if got, want := invocation.Args, []string{script, "--once"}; !slices.Equal(got, want) {
		t.Fatalf("Args = %#v, want %#v", got, want)
	}
}

func TestResolveRuntimeInvocationUsesAbsoluteRuntimeOverride(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "demo-web")
	script := filepath.Join(dir, "scripts", "harvest.mjs")
	if err := os.MkdirAll(filepath.Dir(script), 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(script, []byte("\n"), 0o600); err != nil {
		t.Fatalf("write script: %v", err)
	}
	nodePath := filepath.Join(root, "bin", "node")

	invocation, err := resolveHarvesterInvocation(dir, Manifest{
		ID:      "demo-web",
		Runtime: &RuntimeSpec{Type: "node", Entry: "scripts/harvest.mjs"},
	}, RuntimeOverrides{Node: nodePath})
	if err != nil {
		t.Fatalf("resolveHarvesterInvocation() error = %v", err)
	}

	if invocation.Program != nodePath {
		t.Fatalf("Program = %q, want %q", invocation.Program, nodePath)
	}
}

func TestResolveRuntimeInvocationIgnoresRelativeRuntimeOverride(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "demo-web")
	script := filepath.Join(dir, "scripts", "harvest.mjs")
	if err := os.MkdirAll(filepath.Dir(script), 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(script, []byte("\n"), 0o600); err != nil {
		t.Fatalf("write script: %v", err)
	}

	invocation, err := resolveHarvesterInvocation(dir, Manifest{
		ID:      "demo-web",
		Runtime: &RuntimeSpec{Type: "node", Entry: "scripts/harvest.mjs"},
	}, RuntimeOverrides{Node: filepath.Join("tools", "node")})
	if err != nil {
		t.Fatalf("resolveHarvesterInvocation() error = %v", err)
	}

	if invocation.Program != "node" {
		t.Fatalf("Program = %q, want node", invocation.Program)
	}
}

func TestResolvePortableRuntimeProgramUsesHostPathRoots(t *testing.T) {
	roots := portableRuntimePathRoots{
		Home:      filepath.Join(string(filepath.Separator), "Users", "alice"),
		Config:    filepath.Join(string(filepath.Separator), "Users", "alice", "AppData", "Roaming"),
		LocalData: filepath.Join(string(filepath.Separator), "Users", "alice", "AppData", "Local"),
		Data:      filepath.Join(string(filepath.Separator), "Users", "alice", "AppData", "Roaming"),
		Cache:     filepath.Join(string(filepath.Separator), "Users", "alice", "AppData", "Local", "Cache"),
	}

	tests := map[string]string{
		"~/bin/node":                          filepath.Join(roots.Home, "bin", "node"),
		"@config/AssetIWeave/node":            filepath.Join(roots.Config, "AssetIWeave", "node"),
		"@local-data/AssetIWeave/python.exe":  filepath.Join(roots.LocalData, "AssetIWeave", "python.exe"),
		"@data/AssetIWeave/bash":              filepath.Join(roots.Data, "AssetIWeave", "bash"),
		"@cache/AssetIWeave/runtime":          filepath.Join(roots.Cache, "AssetIWeave", "runtime"),
		"%USERPROFILE%/bin/node.exe":          filepath.Join(roots.Home, "bin", "node.exe"),
		"%APPDATA%/AssetIWeave/node.exe":      filepath.Join(roots.Config, "AssetIWeave", "node.exe"),
		"%LOCALAPPDATA%/AssetIWeave/node.exe": filepath.Join(roots.LocalData, "AssetIWeave", "node.exe"),
	}

	for stored, want := range tests {
		t.Run(stored, func(t *testing.T) {
			got, ok := resolvePortableRuntimeProgramWithRoots(stored, roots)
			if !ok {
				t.Fatalf("resolvePortableRuntimeProgramWithRoots(%q) was not resolved", stored)
			}
			if got != want {
				t.Fatalf("resolvePortableRuntimeProgramWithRoots(%q) = %q, want %q", stored, got, want)
			}
		})
	}
}

func TestLoadRuntimeOverridesReadsAppSettingsFile(t *testing.T) {
	home := t.TempDir()
	t.Setenv(homeEnv, home)
	nodePath := filepath.Join(home, "bin", "node")
	bashPath := filepath.Join(home, "bin", "bash")
	document := map[string]any{
		"schemaVersion": 1,
		"settings": map[string]any{
			"conversationRuntimeOverrides": map[string]any{
				"node":   nodePath,
				"python": filepath.Join("tools", "python"),
				"bash":   bashPath,
			},
		},
	}
	content, err := json.Marshal(document)
	if err != nil {
		t.Fatalf("marshal settings: %v", err)
	}
	if err := os.WriteFile(filepath.Join(home, configFileName), content, 0o600); err != nil {
		t.Fatalf("write settings: %v", err)
	}

	overrides, err := LoadRuntimeOverrides()
	if err != nil {
		t.Fatalf("LoadRuntimeOverrides() error = %v", err)
	}

	if overrides.Node != nodePath || overrides.Python != "" || overrides.Bash != bashPath {
		t.Fatalf("RuntimeOverrides = %#v", overrides)
	}
}

func TestValidateManifestRejectsRuntimeMixedWithEntrypoint(t *testing.T) {
	err := validateManifest(Manifest{
		SchemaVersion: 1,
		ID:            "demo-web",
		Name:          "Demo Web",
		Version:       "0.1.0",
		Entrypoint:    []string{"scripts/harvest.js"},
		Runtime:       &RuntimeSpec{Type: "node", Entry: "scripts/harvest.js", Version: ">=20"},
	})
	if err == nil {
		t.Fatal("validateManifest() error = nil, want mixed runtime and entrypoint error")
	}
	if !strings.Contains(err.Error(), "must not declare both runtime and entrypoint") {
		t.Fatalf("error = %v", err)
	}
}

type zipTestEntry struct {
	Name    string
	Content string
}

func makeZipEntries(t *testing.T, entries []zipTestEntry) []byte {
	t.Helper()
	var buffer bytes.Buffer
	writer := zip.NewWriter(&buffer)
	for _, entry := range entries {
		file, err := writer.Create(entry.Name)
		if err != nil {
			t.Fatalf("create zip entry %s: %v", entry.Name, err)
		}
		if _, err := file.Write([]byte(entry.Content)); err != nil {
			t.Fatalf("write zip entry %s: %v", entry.Name, err)
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("close zip: %v", err)
	}
	return buffer.Bytes()
}

func writePackage(t *testing.T, parent, id, origin, version string) string {
	t.Helper()
	dir := filepath.Join(parent, id)
	if err := os.MkdirAll(filepath.Join(dir, "scripts"), 0o700); err != nil {
		t.Fatalf("mkdir package scripts: %v", err)
	}
	if err := os.MkdirAll(filepath.Join(dir, "requests"), 0o700); err != nil {
		t.Fatalf("mkdir package requests: %v", err)
	}
	manifest := Manifest{
		SchemaVersion: 1,
		ID:            id,
		Name:          id,
		Version:       version,
		Origin:        origin,
		Entrypoint:    []string{"scripts/harvest.sh"},
		Output: OutputSpec{
			NormalizedDir: "output/normalized",
			SessionsFile:  "sessions.json",
		},
		Adapter: AdapterSpec{Manifest: "conversation-adapter.json"},
		Source: SourceSpec{
			ID:   id + "-export",
			Kind: "directory",
			Name: id + " Export",
		},
		Update: UpdateSpec{Channel: origin},
	}
	data, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		t.Fatalf("marshal manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "harvester.json"), append(data, '\n'), 0o600); err != nil {
		t.Fatalf("write manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "conversation-adapter.json"), []byte(`{"schema_version":1,"id":"`+id+`","name":"`+id+`","version":"`+version+`","protocol_version":1,"command":["adapter.js"],"capabilities":["probe","read_session"],"input_kinds":["directory"]}`+"\n"), 0o600); err != nil {
		t.Fatalf("write adapter manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "adapter.js"), []byte("#!/usr/bin/env node\n"), 0o700); err != nil {
		t.Fatalf("write adapter: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "scripts", "harvest.sh"), []byte("#!/bin/sh\n"), 0o700); err != nil {
		t.Fatalf("write harvest script: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "requests", "auth-probe.json"), []byte("{}\n"), 0o600); err != nil {
		t.Fatalf("write auth probe: %v", err)
	}
	return dir
}

func makePackageZip(t *testing.T, id string, unsafe bool) []byte {
	t.Helper()
	packageDir := writePackage(t, t.TempDir(), id, "official", "1.0.0")
	var buffer bytes.Buffer
	writer := zip.NewWriter(&buffer)
	if unsafe {
		file, err := writer.Create("../escape.txt")
		if err != nil {
			t.Fatalf("create unsafe zip entry: %v", err)
		}
		if _, err := file.Write([]byte("escape")); err != nil {
			t.Fatalf("write unsafe zip entry: %v", err)
		}
	}
	if err := filepath.WalkDir(packageDir, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(filepath.Dir(packageDir), path)
		if err != nil {
			return err
		}
		file, err := writer.Create(filepath.ToSlash(rel))
		if err != nil {
			return err
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		_, err = file.Write(data)
		return err
	}); err != nil {
		t.Fatalf("write package zip: %v", err)
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("close zip: %v", err)
	}
	return buffer.Bytes()
}

func TestRunRejectsEntrypointEscape(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "bad-web")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	manifest := `{
  "schema_version": 1,
  "id": "bad-web",
  "name": "Bad Web",
  "version": "0.1.0",
  "origin": "community",
  "entrypoint": ["../outside.sh"],
  "output": {"normalized_dir": "output/normalized", "sessions_file": "sessions.json"},
  "adapter": {"manifest": "conversation-adapter.json"},
  "source": {"id": "bad-web-export", "kind": "directory", "name": "Bad Web Export"},
  "update": {"channel": "community"}
}`
	if err := os.WriteFile(filepath.Join(dir, "harvester.json"), []byte(manifest), 0o600); err != nil {
		t.Fatalf("write manifest: %v", err)
	}

	if _, err := Run(RunOptions{Root: root, ID: "bad-web"}); err == nil {
		t.Fatal("Run() error = nil, want entrypoint escape validation")
	}
}
