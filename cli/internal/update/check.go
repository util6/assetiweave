package update

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"time"
)

const (
	DefaultManifestURL        = "https://github.com/util6/assetiweave/releases/latest/download/latest.json"
	ManifestURLEnv            = "ASSETIWEAVE_UPDATE_MANIFEST_URL"
	NoUpdateNotifierEnv       = "ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER"
	UpdateStatePathEnv        = "ASSETIWEAVE_UPDATE_STATE_PATH"
	UpdateConfigDirEnv        = "ASSETIWEAVE_CLI_CONFIG_DIR"
	requestTimeout            = 3 * time.Second
	cacheTTL                  = 24 * time.Hour
	updateStateFile           = "update-state.json"
	defaultUpdateCommand      = "download the latest AssetIWeave release"
	defaultUpdateConfigSubdir = ".assetiweave-cli"
)

type Report struct {
	Checked   bool   `json:"checked"`
	Available bool   `json:"available"`
	Current   string `json:"current"`
	Latest    string `json:"latest,omitempty"`
	Source    string `json:"source"`
	Command   string `json:"command,omitempty"`
	Error     string `json:"error,omitempty"`
}

type Info struct {
	Current string `json:"current"`
	Latest  string `json:"latest"`
}

func (i *Info) Message() string {
	return fmt.Sprintf("AssetIWeave CLI %s available, current %s", i.Latest, i.Current)
}

var pending atomic.Pointer[Info]

func SetPending(info *Info) {
	pending.Store(info)
}

func GetPending() *Info {
	return pending.Load()
}

type updateState struct {
	LatestVersion string `json:"latest_version"`
	CheckedAt     int64  `json:"checked_at"`
}

func Check(currentVersion, manifestURL string) Report {
	if manifestURL == "" {
		manifestURL = DefaultManifestURL
	}
	report := Report{
		Current: currentVersion,
		Source:  manifestURL,
		Command: defaultUpdateCommand,
	}
	ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
	defer cancel()
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, manifestURL, nil)
	if err != nil {
		report.Error = err.Error()
		return report
	}
	response, err := http.DefaultClient.Do(request)
	if err != nil {
		report.Error = err.Error()
		return report
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		report.Error = fmt.Sprintf("manifest returned HTTP %d", response.StatusCode)
		return report
	}
	var manifest struct {
		Version string `json:"version"`
	}
	if err := json.NewDecoder(response.Body).Decode(&manifest); err != nil {
		report.Error = err.Error()
		return report
	}
	if manifest.Version == "" {
		report.Error = "manifest version is empty"
		return report
	}
	report.Checked = true
	report.Latest = manifest.Version
	report.Available = isNewerVersion(currentVersion, manifest.Version)
	return report
}

func CheckAndCache(currentVersion, manifestURL string) Report {
	report := Check(currentVersion, manifestURL)
	if !report.Checked {
		return report
	}
	_ = saveState(&updateState{
		LatestVersion: report.Latest,
		CheckedAt:     time.Now().Unix(),
	})
	if report.Available {
		SetPending(&Info{Current: currentVersion, Latest: report.Latest})
	}
	return report
}

func CheckCached(currentVersion string) *Info {
	if shouldSkip(currentVersion) {
		return nil
	}
	state, err := loadState()
	if err != nil || state.LatestVersion == "" {
		return nil
	}
	if !isNewerVersion(currentVersion, state.LatestVersion) {
		return nil
	}
	return &Info{Current: currentVersion, Latest: state.LatestVersion}
}

func RefreshCache(currentVersion string) {
	if shouldSkip(currentVersion) {
		return
	}
	state, _ := loadState()
	if state != nil && time.Since(time.Unix(state.CheckedAt, 0)) < cacheTTL {
		return
	}
	report := Check(currentVersion, os.Getenv(ManifestURLEnv))
	if !report.Checked {
		return
	}
	_ = saveState(&updateState{
		LatestVersion: report.Latest,
		CheckedAt:     time.Now().Unix(),
	})
	if report.Available {
		SetPending(&Info{Current: currentVersion, Latest: report.Latest})
	}
}

func shouldSkip(currentVersion string) bool {
	if os.Getenv(NoUpdateNotifierEnv) != "" || isCIEnv() {
		return true
	}
	if currentVersion == "" || strings.EqualFold(currentVersion, "dev") {
		return true
	}
	_, ok := parseVersion(currentVersion)
	return !ok
}

func isCIEnv() bool {
	for _, key := range []string{"CI", "BUILD_NUMBER", "RUN_ID"} {
		if os.Getenv(key) != "" {
			return true
		}
	}
	return false
}

func statePath() string {
	if path := os.Getenv(UpdateStatePathEnv); path != "" {
		return path
	}
	if dir := os.Getenv(UpdateConfigDirEnv); dir != "" {
		return filepath.Join(dir, updateStateFile)
	}
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return ""
	}
	return filepath.Join(home, defaultUpdateConfigSubdir, updateStateFile)
}

func loadState() (*updateState, error) {
	path := statePath()
	if path == "" {
		return nil, fmt.Errorf("update state path is unavailable")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var state updateState
	if err := json.Unmarshal(data, &state); err != nil {
		return nil, err
	}
	return &state, nil
}

func saveState(state *updateState) error {
	path := statePath()
	if path == "" {
		return fmt.Errorf("update state path is unavailable")
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	data, err := json.Marshal(state)
	if err != nil {
		return err
	}
	tempPath := path + ".tmp"
	if err := os.WriteFile(tempPath, data, 0o600); err != nil {
		return err
	}
	return os.Rename(tempPath, path)
}

func isNewerVersion(current, latest string) bool {
	currentVersion, ok := parseVersion(current)
	if !ok {
		return false
	}
	latestVersion, ok := parseVersion(latest)
	if !ok {
		return false
	}
	for index := range currentVersion {
		if latestVersion[index] > currentVersion[index] {
			return true
		}
		if latestVersion[index] < currentVersion[index] {
			return false
		}
	}
	return false
}

func parseVersion(value string) ([3]int, bool) {
	var version [3]int
	value = strings.TrimPrefix(strings.TrimSpace(value), "v")
	parts := strings.Split(value, ".")
	if len(parts) == 0 || len(parts) > len(version) {
		return version, false
	}
	for index, part := range parts {
		if part == "" {
			return version, false
		}
		parsed, err := strconv.Atoi(part)
		if err != nil || parsed < 0 {
			return version, false
		}
		version[index] = parsed
	}
	return version, true
}
