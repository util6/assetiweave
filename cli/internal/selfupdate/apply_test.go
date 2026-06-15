package selfupdate

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

func TestApplyDownloadsVerifiesAndInstallsTools(t *testing.T) {
	installDir := t.TempDir()
	writeExecutable(t, filepath.Join(installDir, "assetiweave-cli"), "old-cli")
	writeExecutable(t, filepath.Join(installDir, "assetiweave-engine"), "old-engine")
	archive := makeToolsTarGz(t, "new-cli", "new-engine")
	checksum := fmt.Sprintf("%x  assetiweave-tools-v0.2.0-macos-arm64.tar.gz\n", sha256.Sum256(archive))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/tools.tar.gz":
			_, _ = w.Write(archive)
		case "/tools.tar.gz.sha256":
			_, _ = w.Write([]byte(checksum))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(server.Close)

	result, err := Apply(context.Background(), Result{
		Action:        "update_available",
		Latest:        "0.2.0",
		PackageAsset:  "assetiweave-tools-v0.2.0-macos-arm64.tar.gz",
		PackageURL:    server.URL + "/tools.tar.gz",
		ChecksumAsset: "assetiweave-tools-v0.2.0-macos-arm64.tar.gz.sha256",
		ChecksumURL:   server.URL + "/tools.tar.gz.sha256",
	}, ApplyOptions{InstallDir: installDir, GOOS: "darwin"})

	if err != nil {
		t.Fatalf("Apply() error = %v", err)
	}
	if result.Action != "updated" || len(result.Installed) != 2 {
		t.Fatalf("result = %+v", result)
	}
	if got := readFile(t, filepath.Join(installDir, "assetiweave-cli")); got != "new-cli" {
		t.Fatalf("cli = %q", got)
	}
	if got := readFile(t, filepath.Join(installDir, "assetiweave-engine")); got != "new-engine" {
		t.Fatalf("engine = %q", got)
	}
}

func TestApplyInstallsBundledHarvestersPreservingState(t *testing.T) {
	installDir := t.TempDir()
	harvesterRoot := t.TempDir()
	writeExecutable(t, filepath.Join(installDir, "assetiweave-cli"), "old-cli")
	writeExecutable(t, filepath.Join(installDir, "assetiweave-engine"), "old-engine")
	authPath := filepath.Join(harvesterRoot, "tencent-yuanbao-web", "requests", "auth-probe.json")
	if err := os.MkdirAll(filepath.Dir(authPath), 0o700); err != nil {
		t.Fatalf("mkdir auth state: %v", err)
	}
	if err := os.WriteFile(authPath, []byte(`{"secret":"keep"}`+"\n"), 0o600); err != nil {
		t.Fatalf("write auth state: %v", err)
	}
	archive := makeToolsTarGzWithHarvester(t, "new-cli", "new-engine")
	checksum := fmt.Sprintf("%x  assetiweave-tools-v0.3.0-macos-arm64.tar.gz\n", sha256.Sum256(archive))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/tools.tar.gz":
			_, _ = w.Write(archive)
		case "/tools.tar.gz.sha256":
			_, _ = w.Write([]byte(checksum))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(server.Close)

	result, err := Apply(context.Background(), Result{
		Action:        "update_available",
		Latest:        "0.3.0",
		PackageAsset:  "assetiweave-tools-v0.3.0-macos-arm64.tar.gz",
		PackageURL:    server.URL + "/tools.tar.gz",
		ChecksumAsset: "assetiweave-tools-v0.3.0-macos-arm64.tar.gz.sha256",
		ChecksumURL:   server.URL + "/tools.tar.gz.sha256",
	}, ApplyOptions{InstallDir: installDir, HarvesterRoot: harvesterRoot, GOOS: "darwin"})

	if err != nil {
		t.Fatalf("Apply() error = %v", err)
	}
	if len(result.Harvesters) != 1 || result.Harvesters[0].ID != "tencent-yuanbao-web" {
		t.Fatalf("harvester updates = %+v", result.Harvesters)
	}
	if _, err := os.Stat(filepath.Join(harvesterRoot, "tencent-yuanbao-web", "scripts", "harvest.sh")); err != nil {
		t.Fatalf("bundled harvester script missing: %v", err)
	}
	if got := readFile(t, authPath); got != `{"secret":"keep"}`+"\n" {
		t.Fatalf("auth state = %q, want preserved", got)
	}
}

func TestApplyRejectsChecksumMismatchWithoutReplacingTools(t *testing.T) {
	installDir := t.TempDir()
	writeExecutable(t, filepath.Join(installDir, "assetiweave-cli"), "old-cli")
	writeExecutable(t, filepath.Join(installDir, "assetiweave-engine"), "old-engine")
	archive := makeToolsTarGz(t, "new-cli", "new-engine")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/tools.tar.gz":
			_, _ = w.Write(archive)
		case "/tools.tar.gz.sha256":
			_, _ = w.Write([]byte("0000000000000000000000000000000000000000000000000000000000000000  tools.tar.gz\n"))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(server.Close)

	_, err := Apply(context.Background(), Result{
		Action:        "update_available",
		Latest:        "0.2.0",
		PackageAsset:  "assetiweave-tools-v0.2.0-macos-arm64.tar.gz",
		PackageURL:    server.URL + "/tools.tar.gz",
		ChecksumAsset: "assetiweave-tools-v0.2.0-macos-arm64.tar.gz.sha256",
		ChecksumURL:   server.URL + "/tools.tar.gz.sha256",
	}, ApplyOptions{InstallDir: installDir, GOOS: "darwin"})

	if err == nil {
		t.Fatal("Apply() error = nil, want checksum mismatch")
	}
	if got := readFile(t, filepath.Join(installDir, "assetiweave-cli")); got != "old-cli" {
		t.Fatalf("cli = %q, want old-cli", got)
	}
	if got := readFile(t, filepath.Join(installDir, "assetiweave-engine")); got != "old-engine" {
		t.Fatalf("engine = %q, want old-engine", got)
	}
}

func makeToolsTarGz(t *testing.T, cli, engine string) []byte {
	t.Helper()
	return makeTarGz(t, map[string]string{
		"assetiweave-tools/assetiweave-cli":    cli,
		"assetiweave-tools/assetiweave-engine": engine,
	})
}

func makeToolsTarGzWithHarvester(t *testing.T, cli, engine string) []byte {
	t.Helper()
	return makeTarGz(t, map[string]string{
		"assetiweave-tools/assetiweave-cli":    cli,
		"assetiweave-tools/assetiweave-engine": engine,
		"assetiweave-tools/harvesters/tencent-yuanbao-web/harvester.json": `{
  "schema_version": 1,
  "id": "tencent-yuanbao-web",
  "name": "Tencent Yuanbao Web",
  "version": "0.1.0",
  "origin": "official",
  "entrypoint": ["scripts/harvest.sh"],
  "output": {"normalized_dir": "output/normalized", "sessions_file": "sessions.json"},
  "adapter": {"manifest": "conversation-adapter.json"},
  "source": {"id": "tencent-yuanbao-web-export", "kind": "directory", "name": "Tencent Yuanbao Web Export"},
  "update": {"channel": "official"}
}`,
		"assetiweave-tools/harvesters/tencent-yuanbao-web/conversation-adapter.json": `{"schema_version":1,"id":"tencent-yuanbao-web","name":"Tencent Yuanbao Web","version":"0.1.0","protocol_version":1,"command":["adapter.js"],"capabilities":["probe","read_session"],"input_kinds":["directory"]}`,
		"assetiweave-tools/harvesters/tencent-yuanbao-web/adapter.js":                "#!/usr/bin/env node\n",
		"assetiweave-tools/harvesters/tencent-yuanbao-web/scripts/harvest.sh":        "#!/bin/sh\n",
		"assetiweave-tools/harvesters/tencent-yuanbao-web/requests/auth-probe.json":  `{"secret":"overwrite"}` + "\n",
	})
}

func makeTarGz(t *testing.T, files map[string]string) []byte {
	t.Helper()
	var buffer bytes.Buffer
	gzipWriter := gzip.NewWriter(&buffer)
	tarWriter := tar.NewWriter(gzipWriter)
	for name, body := range files {
		header := &tar.Header{Name: name, Mode: 0o755, Size: int64(len(body))}
		if err := tarWriter.WriteHeader(header); err != nil {
			t.Fatalf("write tar header: %v", err)
		}
		if _, err := tarWriter.Write([]byte(body)); err != nil {
			t.Fatalf("write tar body: %v", err)
		}
	}
	if err := tarWriter.Close(); err != nil {
		t.Fatalf("close tar: %v", err)
	}
	if err := gzipWriter.Close(); err != nil {
		t.Fatalf("close gzip: %v", err)
	}
	return buffer.Bytes()
}

func writeExecutable(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o755); err != nil {
		t.Fatalf("write executable: %v", err)
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	bytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	return string(bytes)
}
