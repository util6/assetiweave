package selfupdate

import (
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
)

func TestCheckReportsSupportedReleasePackage(t *testing.T) {
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
		result.Action != "update_available" ||
		result.Current != "0.1.1" ||
		result.Latest != "0.2.0" ||
		result.Target != "macos-arm64" ||
		result.PackageAsset != "assetiweave-tools-v0.2.0-macos-arm64.tar.gz" ||
		result.ChecksumAsset != "assetiweave-tools-v0.2.0-macos-arm64.tar.gz.sha256" ||
		!strings.Contains(result.PackageURL, "/releases/download/v0.2.0/assetiweave-tools-v0.2.0-macos-arm64.tar.gz") ||
		!strings.Contains(result.ChecksumURL, "/releases/download/v0.2.0/assetiweave-tools-v0.2.0-macos-arm64.tar.gz.sha256") {
		t.Fatalf("result = %+v", result)
	}
}

func TestCheckReportsManualReleaseWhenPlatformPackageIsUnavailable(t *testing.T) {
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
		result.Action != "manual_required" ||
		result.PackageURL != "" ||
		!strings.Contains(result.ReleaseURL, "/releases/tag/v0.2.0") {
		t.Fatalf("result = %+v", result)
	}
}
