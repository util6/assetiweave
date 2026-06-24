package selfupdate

import (
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
)

func TestCheckReportsAppManagedUpdate(t *testing.T) {
	t.Setenv("ASSETIWEAVE_UPDATE_STATE_PATH", filepath.Join(t.TempDir(), "update-state.json"))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"0.2.0"}`))
	}))
	t.Cleanup(server.Close)

	result := Check(Options{
		CurrentVersion: "0.1.1",
		ManifestURL:    server.URL,
		GOOS:           "darwin",
		GOARCH:         "arm64",
	})

	if !result.Checked ||
		!result.UpdateAvailable ||
		result.Action != "app_update_required" ||
		result.Current != "0.1.1" ||
		result.Latest != "0.2.0" ||
		result.PackageURL != "" ||
		result.ChecksumURL != "" ||
		!strings.Contains(result.ReleaseURL, "/releases/tag/v0.2.0") ||
		!strings.Contains(result.Message, "desktop app") {
		t.Fatalf("result = %+v", result)
	}
}

func TestCheckReportsAppManagedUpdateForEveryPlatform(t *testing.T) {
	t.Setenv("ASSETIWEAVE_UPDATE_STATE_PATH", filepath.Join(t.TempDir(), "update-state.json"))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"version":"0.2.0"}`))
	}))
	t.Cleanup(server.Close)

	result := Check(Options{
		CurrentVersion: "0.1.1",
		ManifestURL:    server.URL,
		GOOS:           "linux",
		GOARCH:         "arm64",
	})

	if !result.UpdateAvailable ||
		result.Action != "app_update_required" ||
		result.PackageURL != "" ||
		!strings.Contains(result.ReleaseURL, "/releases/tag/v0.2.0") {
		t.Fatalf("result = %+v", result)
	}
}
