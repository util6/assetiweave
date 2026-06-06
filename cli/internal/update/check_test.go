package update

import (
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"
	"time"
)

func TestCheckReportsAvailableUpdateFromTauriManifest(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"0.2.0","notes":"new release"}`))
	}))
	t.Cleanup(server.Close)

	report := Check("0.1.1", server.URL)

	if !report.Checked ||
		!report.Available ||
		report.Current != "0.1.1" ||
		report.Latest != "0.2.0" ||
		report.Source != server.URL ||
		report.Error != "" {
		t.Fatalf("report = %+v", report)
	}
}

func TestCheckReportsCurrentVersionWhenManifestIsNotNewer(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"0.1.1"}`))
	}))
	t.Cleanup(server.Close)

	report := Check("0.1.1", server.URL)

	if !report.Checked || report.Available || report.Latest != "0.1.1" || report.Error != "" {
		t.Fatalf("report = %+v", report)
	}
}

func TestCheckReturnsNonBlockingErrorReport(t *testing.T) {
	report := Check("0.1.1", "http://127.0.0.1:1/latest.json")

	if report.Checked ||
		report.Available ||
		report.Current != "0.1.1" ||
		report.Source == "" ||
		report.Error == "" {
		t.Fatalf("report = %+v", report)
	}
}

func TestCheckCachedReturnsPendingInfoWhenStateHasNewerVersion(t *testing.T) {
	allowNotifier(t)
	t.Setenv("ASSETIWEAVE_UPDATE_STATE_PATH", filepath.Join(t.TempDir(), "update-state.json"))
	if err := saveState(&updateState{LatestVersion: "0.2.0", CheckedAt: time.Now().Unix()}); err != nil {
		t.Fatalf("save update state: %v", err)
	}

	info := CheckCached("0.1.1")

	if info == nil ||
		info.Current != "0.1.1" ||
		info.Latest != "0.2.0" ||
		info.Message() == "" {
		t.Fatalf("info = %+v", info)
	}
}

func TestCheckCachedHonorsNotifierOptOut(t *testing.T) {
	allowNotifier(t)
	t.Setenv("ASSETIWEAVE_UPDATE_STATE_PATH", filepath.Join(t.TempDir(), "update-state.json"))
	t.Setenv("ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER", "1")
	if err := saveState(&updateState{LatestVersion: "0.2.0", CheckedAt: time.Now().Unix()}); err != nil {
		t.Fatalf("save update state: %v", err)
	}

	if info := CheckCached("0.1.1"); info != nil {
		t.Fatalf("CheckCached() = %+v, want nil when notifier is disabled", info)
	}
}

func TestRefreshCacheWritesRemoteManifestState(t *testing.T) {
	allowNotifier(t)
	t.Setenv("ASSETIWEAVE_UPDATE_STATE_PATH", filepath.Join(t.TempDir(), "update-state.json"))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"0.2.0"}`))
	}))
	t.Cleanup(server.Close)
	t.Setenv("ASSETIWEAVE_UPDATE_MANIFEST_URL", server.URL)

	RefreshCache("0.1.1")

	state, err := loadState()
	if err != nil {
		t.Fatalf("load update state: %v", err)
	}
	if state.LatestVersion != "0.2.0" || state.CheckedAt == 0 {
		t.Fatalf("state = %+v", state)
	}
}

func TestRefreshCacheSkipsFreshState(t *testing.T) {
	allowNotifier(t)
	t.Setenv("ASSETIWEAVE_UPDATE_STATE_PATH", filepath.Join(t.TempDir(), "update-state.json"))
	if err := saveState(&updateState{LatestVersion: "0.2.0", CheckedAt: time.Now().Unix()}); err != nil {
		t.Fatalf("save update state: %v", err)
	}
	requests := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		requests++
		_, _ = w.Write([]byte(`{"version":"0.3.0"}`))
	}))
	t.Cleanup(server.Close)
	t.Setenv("ASSETIWEAVE_UPDATE_MANIFEST_URL", server.URL)

	RefreshCache("0.1.1")

	if requests != 0 {
		t.Fatalf("requests = %d, want 0 for fresh cache", requests)
	}
}

func allowNotifier(t *testing.T) {
	t.Helper()
	t.Setenv("CI", "")
	t.Setenv("BUILD_NUMBER", "")
	t.Setenv("RUN_ID", "")
	t.Setenv("ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER", "")
}

func TestIsNewerVersionComparesSemanticVersions(t *testing.T) {
	cases := []struct {
		current string
		latest  string
		want    bool
	}{
		{"0.1.1", "0.1.2", true},
		{"0.1.1", "0.2.0", true},
		{"0.1.1", "1.0.0", true},
		{"0.1.1", "0.1.1", false},
		{"0.2.0", "0.1.9", false},
		{"dev", "0.1.1", false},
		{"0.1.1", "", false},
	}
	for _, tt := range cases {
		if got := isNewerVersion(tt.current, tt.latest); got != tt.want {
			t.Fatalf("isNewerVersion(%q, %q) = %v, want %v", tt.current, tt.latest, got, tt.want)
		}
	}
}
