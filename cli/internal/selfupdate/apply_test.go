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
	var buffer bytes.Buffer
	gzipWriter := gzip.NewWriter(&buffer)
	tarWriter := tar.NewWriter(gzipWriter)
	for name, body := range map[string]string{
		"assetiweave-tools/assetiweave-cli":    cli,
		"assetiweave-tools/assetiweave-engine": engine,
	} {
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
