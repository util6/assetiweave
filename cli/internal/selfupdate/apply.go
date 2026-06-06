package selfupdate

import (
	"archive/tar"
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"fmt"
	"io"
	"net/http"
	"os"
	pathpkg "path"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

const (
	maxPackageBytes  = 512 << 20
	maxChecksumBytes = 4 << 10
	downloadTimeout  = 60 * time.Second
)

type ApplyOptions struct {
	InstallDir string
	GOOS       string
	HTTPClient *http.Client
}

func Apply(ctx context.Context, result Result, options ApplyOptions) (Result, error) {
	if result.PackageURL == "" || result.ChecksumURL == "" {
		return result, fmt.Errorf("update package and checksum URLs are required")
	}
	client := options.HTTPClient
	if client == nil {
		client = &http.Client{Timeout: downloadTimeout}
	}
	packageBytes, err := fetchBytes(ctx, client, result.PackageURL, maxPackageBytes)
	if err != nil {
		return result, fmt.Errorf("download update package: %w", err)
	}
	checksumBytes, err := fetchBytes(ctx, client, result.ChecksumURL, maxChecksumBytes)
	if err != nil {
		return result, fmt.Errorf("download update checksum: %w", err)
	}
	if err := verifySHA256(packageBytes, string(checksumBytes)); err != nil {
		return result, err
	}

	tempDir, err := os.MkdirTemp("", "assetiweave-update-*")
	if err != nil {
		return result, fmt.Errorf("create update temp dir: %w", err)
	}
	defer os.RemoveAll(tempDir)
	if err := extractArchive(packageBytes, result.PackageAsset, tempDir); err != nil {
		return result, err
	}

	goos := options.GOOS
	if goos == "" {
		goos = runtime.GOOS
	}
	suffix := ""
	if goos == "windows" {
		suffix = ".exe"
	}
	cli, err := findExtractedBinary(tempDir, "assetiweave-cli"+suffix)
	if err != nil {
		return result, err
	}
	engine, err := findExtractedBinary(tempDir, "assetiweave-engine"+suffix)
	if err != nil {
		return result, err
	}
	installDir, err := resolveInstallDir(options.InstallDir)
	if err != nil {
		return result, err
	}
	installed, err := installBinaries(installDir, []binaryInstall{
		{Source: cli, Name: "assetiweave-cli" + suffix},
		{Source: engine, Name: "assetiweave-engine" + suffix},
	}, goos)
	if err != nil {
		return result, err
	}
	result.Action = "updated"
	result.Installed = installed
	result.Message = fmt.Sprintf("AssetIWeave CLI tools updated to %s", result.Latest)
	return result, nil
}

func fetchBytes(ctx context.Context, client *http.Client, url string, limit int64) ([]byte, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	response, err := client.Do(request)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return nil, fmt.Errorf("HTTP %d", response.StatusCode)
	}
	reader := io.LimitReader(response.Body, limit+1)
	data, err := io.ReadAll(reader)
	if err != nil {
		return nil, err
	}
	if int64(len(data)) > limit {
		return nil, fmt.Errorf("response exceeds %d bytes", limit)
	}
	return data, nil
}

func verifySHA256(data []byte, checksumText string) error {
	fields := strings.Fields(checksumText)
	if len(fields) == 0 {
		return fmt.Errorf("checksum file is empty")
	}
	want := strings.ToLower(fields[0])
	got := fmt.Sprintf("%x", sha256.Sum256(data))
	if got != want {
		return fmt.Errorf("checksum mismatch: got %s, want %s", got, want)
	}
	return nil
}

func extractArchive(data []byte, asset, destination string) error {
	switch {
	case strings.HasSuffix(asset, ".zip"):
		return extractZip(data, destination)
	case strings.HasSuffix(asset, ".tar.gz"):
		return extractTarGz(data, destination)
	default:
		return fmt.Errorf("unsupported update archive: %s", asset)
	}
}

func extractTarGz(data []byte, destination string) error {
	gzipReader, err := gzip.NewReader(bytes.NewReader(data))
	if err != nil {
		return fmt.Errorf("read update archive gzip: %w", err)
	}
	defer gzipReader.Close()
	tarReader := tar.NewReader(gzipReader)
	for {
		header, err := tarReader.Next()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return fmt.Errorf("read update archive tar: %w", err)
		}
		if header.FileInfo().IsDir() {
			continue
		}
		target, err := safeArchivePath(destination, header.Name)
		if err != nil {
			return err
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		file, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, os.FileMode(header.Mode)&0o777)
		if err != nil {
			return err
		}
		_, copyErr := io.Copy(file, tarReader)
		closeErr := file.Close()
		if copyErr != nil {
			return copyErr
		}
		if closeErr != nil {
			return closeErr
		}
	}
}

func extractZip(data []byte, destination string) error {
	reader, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return fmt.Errorf("read update archive zip: %w", err)
	}
	for _, entry := range reader.File {
		if entry.FileInfo().IsDir() {
			continue
		}
		target, err := safeArchivePath(destination, entry.Name)
		if err != nil {
			return err
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		source, err := entry.Open()
		if err != nil {
			return err
		}
		file, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o755)
		if err != nil {
			source.Close()
			return err
		}
		_, copyErr := io.Copy(file, source)
		sourceErr := source.Close()
		closeErr := file.Close()
		if copyErr != nil {
			return copyErr
		}
		if sourceErr != nil {
			return sourceErr
		}
		if closeErr != nil {
			return closeErr
		}
	}
	return nil
}

func safeArchivePath(destination, name string) (string, error) {
	clean := pathpkg.Clean(strings.ReplaceAll(name, "\\", "/"))
	if clean == "." || clean == ".." || strings.HasPrefix(clean, "../") || pathpkg.IsAbs(clean) {
		return "", fmt.Errorf("unsafe update archive path: %s", name)
	}
	target := filepath.Join(destination, filepath.FromSlash(clean))
	relative, err := filepath.Rel(destination, target)
	if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("unsafe update archive path: %s", name)
	}
	return target, nil
}

func findExtractedBinary(root, name string) (string, error) {
	var found string
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil || found != "" {
			return err
		}
		if !entry.IsDir() && entry.Name() == name {
			found = path
		}
		return nil
	})
	if err != nil {
		return "", err
	}
	if found == "" {
		return "", fmt.Errorf("update archive missing %s", name)
	}
	return found, nil
}

func resolveInstallDir(override string) (string, error) {
	if override != "" {
		return override, nil
	}
	executable, err := os.Executable()
	if err != nil {
		return "", fmt.Errorf("resolve running executable: %w", err)
	}
	if resolved, err := filepath.EvalSymlinks(executable); err == nil {
		executable = resolved
	}
	return filepath.Dir(executable), nil
}

type binaryInstall struct {
	Source string
	Name   string
}

type installedBinary struct {
	Destination string
	Backup      string
	BackedUp    bool
}

func installBinaries(installDir string, binaries []binaryInstall, goos string) ([]string, error) {
	if err := os.MkdirAll(installDir, 0o755); err != nil {
		return nil, err
	}
	installed := make([]installedBinary, 0, len(binaries))
	installedPaths := make([]string, 0, len(binaries))
	for _, binary := range binaries {
		destination := filepath.Join(installDir, binary.Name)
		backup := destination + ".old"
		_ = os.Remove(backup)
		item := installedBinary{Destination: destination, Backup: backup}
		if _, err := os.Stat(destination); err == nil {
			if err := os.Rename(destination, backup); err != nil {
				rollbackInstalled(installed)
				return nil, fmt.Errorf("backup %s: %w", binary.Name, err)
			}
			item.BackedUp = true
		} else if !os.IsNotExist(err) {
			rollbackInstalled(installed)
			return nil, err
		}
		if err := os.Rename(binary.Source, destination); err != nil {
			rollbackInstalled(append(installed, item))
			return nil, fmt.Errorf("install %s: %w", binary.Name, err)
		}
		if goos != "windows" {
			if err := os.Chmod(destination, 0o755); err != nil {
				rollbackInstalled(append(installed, item))
				return nil, fmt.Errorf("chmod %s: %w", binary.Name, err)
			}
		}
		installed = append(installed, item)
		installedPaths = append(installedPaths, destination)
	}
	for _, item := range installed {
		if item.BackedUp {
			_ = os.Remove(item.Backup)
		}
	}
	return installedPaths, nil
}

func rollbackInstalled(installed []installedBinary) {
	for index := len(installed) - 1; index >= 0; index-- {
		item := installed[index]
		_ = os.Remove(item.Destination)
		if item.BackedUp {
			_ = os.Rename(item.Backup, item.Destination)
		}
	}
}
