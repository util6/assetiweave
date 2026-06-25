package harvesters

import (
	"archive/tar"
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"embed"
	"encoding/json"
	"errors"
	"io"
	"io/fs"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	pathpkg "path"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"time"

	"github.com/util6/assetiweave/errs"
)

const (
	defaultRunTimeout  = 10 * time.Minute
	defaultHTTPTimeout = 60 * time.Second
	maxPackageBytes    = 64 << 20
	maxPackageFiles    = 2048
	homeEnv            = "ASSETIWEAVE_HOME"
	rootEnv            = "ASSETIWEAVE_HARVESTER_ROOT"
)

//go:embed templates
var officialTemplates embed.FS

var validIDPattern = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]*$`)

type Manifest struct {
	SchemaVersion int          `json:"schema_version"`
	ID            string       `json:"id"`
	Name          string       `json:"name"`
	Version       string       `json:"version"`
	Origin        string       `json:"origin"`
	Entrypoint    []string     `json:"entrypoint,omitempty"`
	Runtime       *RuntimeSpec `json:"runtime,omitempty"`
	Output        OutputSpec   `json:"output"`
	Adapter       AdapterSpec  `json:"adapter"`
	Source        SourceSpec   `json:"source"`
	Update        UpdateSpec   `json:"update"`
	Capabilities  []string     `json:"capabilities,omitempty"`
	Description   string       `json:"description,omitempty"`
}

type RuntimeSpec struct {
	Type    string   `json:"type"`
	Entry   string   `json:"entry"`
	Args    []string `json:"args,omitempty"`
	Version string   `json:"version,omitempty"`
}

type OutputSpec struct {
	RawDir        string `json:"raw_dir"`
	NormalizedDir string `json:"normalized_dir"`
	SessionsFile  string `json:"sessions_file"`
}

type AdapterSpec struct {
	Manifest string `json:"manifest"`
}

type SourceSpec struct {
	ID   string `json:"id"`
	Kind string `json:"kind"`
	Name string `json:"name"`
}

type UpdateSpec struct {
	Channel string `json:"channel"`
}

type TemplateSummary struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Version     string `json:"version"`
	Origin      string `json:"origin"`
	Channel     string `json:"channel,omitempty"`
	Installed   bool   `json:"installed,omitempty"`
	Directory   string `json:"directory,omitempty"`
	Description string `json:"description,omitempty"`
}

type InstallOptions struct {
	Root          string
	ID            string
	Force         bool
	PreserveState bool
}

type InstallPackageOptions struct {
	Root          string
	ID            string
	Source        string
	Force         bool
	PreserveState bool
	HTTPClient    *http.Client
}

type InstallResult struct {
	ID              string `json:"id"`
	Name            string `json:"name"`
	Version         string `json:"version"`
	Origin          string `json:"origin"`
	PackageSource   string `json:"package_source,omitempty"`
	Directory       string `json:"directory"`
	ManifestPath    string `json:"manifest_path"`
	AdapterManifest string `json:"adapter_manifest,omitempty"`
	SourceID        string `json:"source_id,omitempty"`
	Updated         bool   `json:"updated"`
	FileCount       int    `json:"file_count"`
}

type RunOptions struct {
	Root    string
	ID      string
	Timeout time.Duration
	Args    []string
}

type RunResult struct {
	ID             string   `json:"id"`
	Directory      string   `json:"directory"`
	Command        []string `json:"command"`
	ExitCode       int      `json:"exit_code"`
	Stdout         string   `json:"stdout,omitempty"`
	Stderr         string   `json:"stderr,omitempty"`
	Result         any      `json:"result,omitempty"`
	NormalizedFile string   `json:"normalized_file,omitempty"`
}

type entrypointInvocation struct {
	Program string
	Args    []string
}

func DefaultRoot() (string, error) {
	if root := strings.TrimSpace(os.Getenv(rootEnv)); root != "" {
		return filepath.Clean(root), nil
	}
	home := strings.TrimSpace(os.Getenv(homeEnv))
	if home == "" {
		var err error
		home, err = os.UserHomeDir()
		if err != nil {
			return "", internalError("resolve home directory: %v", err)
		}
		home = filepath.Join(home, ".assetiweave")
	}
	return filepath.Join(filepath.Clean(home), "harvesters"), nil
}

func ListOfficialTemplates(root string) ([]TemplateSummary, error) {
	root, err := resolveRoot(root)
	if err != nil {
		return nil, err
	}
	entries, err := officialTemplates.ReadDir("templates")
	if err != nil {
		return nil, internalError("read official harvester templates: %v", err)
	}
	summaries := make([]TemplateSummary, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		manifest, err := readOfficialManifest(entry.Name())
		if err != nil {
			return nil, err
		}
		directory := filepath.Join(root, manifest.ID)
		_, statErr := os.Stat(filepath.Join(directory, "harvester.json"))
		summaries = append(summaries, TemplateSummary{
			ID:          manifest.ID,
			Name:        manifest.Name,
			Version:     manifest.Version,
			Origin:      manifest.Origin,
			Channel:     manifest.Update.Channel,
			Installed:   statErr == nil,
			Directory:   directory,
			Description: manifest.Description,
		})
	}
	return summaries, nil
}

func ListInstalled(root string) ([]TemplateSummary, error) {
	root, err := resolveRoot(root)
	if err != nil {
		return nil, err
	}
	entries, err := os.ReadDir(root)
	if os.IsNotExist(err) {
		return []TemplateSummary{}, nil
	}
	if err != nil {
		return nil, internalError("read harvester root %s: %v", root, err)
	}
	summaries := []TemplateSummary{}
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		manifest, err := LoadManifest(filepath.Join(root, entry.Name()))
		if err != nil {
			continue
		}
		summaries = append(summaries, TemplateSummary{
			ID:          manifest.ID,
			Name:        manifest.Name,
			Version:     manifest.Version,
			Origin:      manifest.Origin,
			Channel:     manifest.Update.Channel,
			Installed:   true,
			Directory:   filepath.Join(root, entry.Name()),
			Description: manifest.Description,
		})
	}
	return summaries, nil
}

func InstallOfficialTemplate(options InstallOptions) (InstallResult, error) {
	if err := validateID(options.ID); err != nil {
		return InstallResult{}, err
	}
	root, err := resolveRoot(options.Root)
	if err != nil {
		return InstallResult{}, err
	}
	sourceRoot := filepath.ToSlash(filepath.Join("templates", options.ID))
	return installFromFS(installFSOptions{
		Root:          root,
		ID:            options.ID,
		SourceRoot:    sourceRoot,
		Files:         officialTemplates,
		Force:         options.Force,
		PreserveState: options.PreserveState,
	})
}

func InstallPackage(options InstallPackageOptions) (InstallResult, error) {
	if err := validateID(options.ID); err != nil {
		return InstallResult{}, err
	}
	root, err := resolveRoot(options.Root)
	if err != nil {
		return InstallResult{}, err
	}
	source := strings.TrimSpace(options.Source)
	if source == "" {
		return InstallResult{}, validationError("harvester package source is required")
	}
	tempDir, cleanup, err := preparePackageSource(source, options.HTTPClient)
	if err != nil {
		return InstallResult{}, err
	}
	defer cleanup()
	packageRoot, err := findPackageRoot(tempDir)
	if err != nil {
		return InstallResult{}, err
	}
	manifest, err := LoadManifest(packageRoot)
	if err != nil {
		return InstallResult{}, err
	}
	if manifest.ID != options.ID {
		return InstallResult{}, validationError("harvester package id %q does not match requested id %q", manifest.ID, options.ID)
	}
	result, err := installFromOSDir(installDirOptions{
		Root:          root,
		PackageRoot:   packageRoot,
		Manifest:      manifest,
		Force:         options.Force,
		PreserveState: options.PreserveState,
	})
	if err != nil {
		return InstallResult{}, err
	}
	result.PackageSource = source
	return result, nil
}

type installFSOptions struct {
	Root          string
	ID            string
	SourceRoot    string
	Files         fs.FS
	Force         bool
	PreserveState bool
}

type installDirOptions struct {
	Root          string
	PackageRoot   string
	Manifest      Manifest
	Force         bool
	PreserveState bool
}

func installFromFS(options installFSOptions) (InstallResult, error) {
	manifest, err := readOfficialManifest(options.ID)
	if err != nil {
		return InstallResult{}, err
	}
	target := filepath.Join(options.Root, options.ID)
	exists := pathExists(target)
	if exists && !options.Force {
		return InstallResult{}, validationError("harvester %q already exists at %s", options.ID, target).
			WithHint("rerun with --force to replace it with the packaged template")
	}
	if options.Force && !options.PreserveState {
		if err := os.RemoveAll(target); err != nil {
			return InstallResult{}, internalError("replace harvester %s: %v", target, err)
		}
	}
	fileCount := 0
	if err := fs.WalkDir(options.Files, options.SourceRoot, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		rel, err := filepath.Rel(options.SourceRoot, path)
		if err != nil {
			return err
		}
		if rel == "." {
			return os.MkdirAll(target, 0o700)
		}
		targetPath := filepath.Join(target, filepath.FromSlash(rel))
		if entry.IsDir() {
			return os.MkdirAll(targetPath, 0o700)
		}
		if options.PreserveState && preserveInstalledState(rel) && pathExists(targetPath) {
			return nil
		}
		data, err := fs.ReadFile(options.Files, path)
		if err != nil {
			return err
		}
		mode := modeForInstalledFile(entry.Name(), 0)
		if err := os.MkdirAll(filepath.Dir(targetPath), 0o700); err != nil {
			return err
		}
		if err := os.WriteFile(targetPath, data, mode); err != nil {
			return err
		}
		fileCount++
		return nil
	}); err != nil {
		return InstallResult{}, internalError("install harvester template %s: %v", options.ID, err)
	}
	return installResultFor(manifest, target, exists, fileCount), nil
}

func installFromOSDir(options installDirOptions) (InstallResult, error) {
	if err := validateManifest(options.Manifest); err != nil {
		return InstallResult{}, err
	}
	target := filepath.Join(options.Root, options.Manifest.ID)
	exists := pathExists(target)
	if exists && !options.Force {
		return InstallResult{}, validationError("harvester %q already exists at %s", options.Manifest.ID, target).
			WithHint("rerun with --force to replace it with the package")
	}
	if options.Force && !options.PreserveState {
		if err := os.RemoveAll(target); err != nil {
			return InstallResult{}, internalError("replace harvester %s: %v", target, err)
		}
	}
	fileCount := 0
	if err := filepath.WalkDir(options.PackageRoot, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		rel, err := filepath.Rel(options.PackageRoot, path)
		if err != nil {
			return err
		}
		if rel == "." {
			return os.MkdirAll(target, 0o700)
		}
		if unsafeRelPath(rel) {
			return validationError("unsafe harvester package path: %s", rel)
		}
		targetPath := filepath.Join(target, rel)
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if info.Mode()&fs.ModeSymlink != 0 {
			return validationError("harvester package symlinks are not supported: %s", rel)
		}
		if entry.IsDir() {
			return os.MkdirAll(targetPath, 0o700)
		}
		fileCount++
		if fileCount > maxPackageFiles {
			return validationError("harvester package has too many files: %d", fileCount)
		}
		if options.PreserveState && preserveInstalledState(rel) && pathExists(targetPath) {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		if err := os.MkdirAll(filepath.Dir(targetPath), 0o700); err != nil {
			return err
		}
		if err := os.WriteFile(targetPath, data, modeForInstalledFile(entry.Name(), info.Mode().Perm())); err != nil {
			return err
		}
		return nil
	}); err != nil {
		return InstallResult{}, internalError("install harvester package %s: %v", options.Manifest.ID, err)
	}
	return installResultFor(options.Manifest, target, exists, fileCount), nil
}

func installResultFor(manifest Manifest, target string, updated bool, fileCount int) InstallResult {
	return InstallResult{
		ID:              manifest.ID,
		Name:            manifest.Name,
		Version:         manifest.Version,
		Origin:          manifest.Origin,
		Directory:       target,
		ManifestPath:    filepath.Join(target, "harvester.json"),
		AdapterManifest: resolveRelative(target, manifest.Adapter.Manifest),
		SourceID:        manifest.Source.ID,
		Updated:         updated,
		FileCount:       fileCount,
	}
}

func preparePackageSource(source string, client *http.Client) (string, func(), error) {
	parsed, parseErr := url.Parse(source)
	if parseErr == nil {
		switch parsed.Scheme {
		case "http", "https":
			data, err := fetchPackageBytes(source, client)
			if err != nil {
				return "", func() {}, err
			}
			tempDir, err := os.MkdirTemp("", "assetiweave-harvester-package-*")
			if err != nil {
				return "", func() {}, internalError("create harvester package temp dir: %v", err)
			}
			cleanup := func() { _ = os.RemoveAll(tempDir) }
			if err := extractPackageArchive(data, pathpkg.Base(parsed.Path), tempDir); err != nil {
				cleanup()
				return "", func() {}, err
			}
			return tempDir, cleanup, nil
		case "file":
			source = parsed.Path
		}
	}
	info, err := os.Stat(source)
	if err != nil {
		return "", func() {}, validationError("harvester package source not found: %s", source).WithCause(err)
	}
	if info.IsDir() {
		return filepath.Clean(source), func() {}, nil
	}
	data, err := readPackageFile(source)
	if err != nil {
		return "", func() {}, err
	}
	tempDir, err := os.MkdirTemp("", "assetiweave-harvester-package-*")
	if err != nil {
		return "", func() {}, internalError("create harvester package temp dir: %v", err)
	}
	cleanup := func() { _ = os.RemoveAll(tempDir) }
	if err := extractPackageArchive(data, filepath.Base(source), tempDir); err != nil {
		cleanup()
		return "", func() {}, err
	}
	return tempDir, cleanup, nil
}

func fetchPackageBytes(source string, client *http.Client) ([]byte, error) {
	if client == nil {
		client = &http.Client{Timeout: defaultHTTPTimeout}
	}
	ctx, cancel := context.WithTimeout(context.Background(), defaultHTTPTimeout)
	defer cancel()
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, source, nil)
	if err != nil {
		return nil, validationError("invalid harvester package URL: %s", source).WithCause(err)
	}
	response, err := client.Do(request)
	if err != nil {
		return nil, validationError("download harvester package failed: %s", source).WithCause(err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return nil, validationError("download harvester package returned HTTP %d", response.StatusCode)
	}
	reader := io.LimitReader(response.Body, maxPackageBytes+1)
	data, err := io.ReadAll(reader)
	if err != nil {
		return nil, internalError("read harvester package response: %v", err)
	}
	if int64(len(data)) > maxPackageBytes {
		return nil, validationError("harvester package exceeds %d bytes", maxPackageBytes)
	}
	return data, nil
}

func readPackageFile(path string) ([]byte, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, validationError("open harvester package: %s", path).WithCause(err)
	}
	defer file.Close()
	reader := io.LimitReader(file, maxPackageBytes+1)
	data, err := io.ReadAll(reader)
	if err != nil {
		return nil, internalError("read harvester package %s: %v", path, err)
	}
	if int64(len(data)) > maxPackageBytes {
		return nil, validationError("harvester package exceeds %d bytes", maxPackageBytes)
	}
	return data, nil
}

func extractPackageArchive(data []byte, name, destination string) error {
	switch {
	case strings.HasSuffix(name, ".zip"):
		return extractPackageZip(data, destination)
	case strings.HasSuffix(name, ".tar.gz") || strings.HasSuffix(name, ".tgz"):
		return extractPackageTarGz(data, destination)
	default:
		return validationError("unsupported harvester package archive: %s", name)
	}
}

func extractPackageZip(data []byte, destination string) error {
	reader, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return validationError("read harvester package zip: %v", err).WithCause(err)
	}
	fileCount := 0
	for _, entry := range reader.File {
		info := entry.FileInfo()
		if info.IsDir() {
			continue
		}
		if info.Mode()&fs.ModeSymlink != 0 {
			return validationError("harvester package symlinks are not supported: %s", entry.Name)
		}
		fileCount++
		if fileCount > maxPackageFiles {
			return validationError("harvester package has too many files: %d", fileCount)
		}
		target, err := safeArchivePath(destination, entry.Name)
		if err != nil {
			return err
		}
		source, err := entry.Open()
		if err != nil {
			return internalError("open harvester package zip entry %s: %v", entry.Name, err)
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
			source.Close()
			return err
		}
		file, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, modeForInstalledFile(filepath.Base(entry.Name), info.Mode().Perm()))
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

func extractPackageTarGz(data []byte, destination string) error {
	gzipReader, err := gzip.NewReader(bytes.NewReader(data))
	if err != nil {
		return validationError("read harvester package gzip: %v", err).WithCause(err)
	}
	defer gzipReader.Close()
	tarReader := tar.NewReader(gzipReader)
	fileCount := 0
	for {
		header, err := tarReader.Next()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return validationError("read harvester package tar: %v", err).WithCause(err)
		}
		if header.FileInfo().IsDir() {
			continue
		}
		if header.Typeflag == tar.TypeSymlink || header.Typeflag == tar.TypeLink {
			return validationError("harvester package links are not supported: %s", header.Name)
		}
		fileCount++
		if fileCount > maxPackageFiles {
			return validationError("harvester package has too many files: %d", fileCount)
		}
		target, err := safeArchivePath(destination, header.Name)
		if err != nil {
			return err
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
			return err
		}
		file, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, modeForInstalledFile(filepath.Base(header.Name), os.FileMode(header.Mode)&0o777))
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

func safeArchivePath(destination, name string) (string, error) {
	name = strings.ReplaceAll(name, "\\", "/")
	clean := pathpkg.Clean(name)
	if clean == "." || pathpkg.IsAbs(clean) || strings.HasPrefix(clean, "../") || clean == ".." {
		return "", validationError("unsafe harvester package archive path: %s", name)
	}
	target := filepath.Join(destination, filepath.FromSlash(clean))
	rel, err := filepath.Rel(destination, target)
	if err != nil {
		return "", err
	}
	if unsafeRelPath(rel) {
		return "", validationError("unsafe harvester package archive path: %s", name)
	}
	return target, nil
}

func findPackageRoot(root string) (string, error) {
	if pathExists(filepath.Join(root, "harvester.json")) {
		return filepath.Clean(root), nil
	}
	entries, err := os.ReadDir(root)
	if err != nil {
		return "", internalError("read harvester package root: %v", err)
	}
	candidates := []string{}
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		candidate := filepath.Join(root, entry.Name())
		if pathExists(filepath.Join(candidate, "harvester.json")) {
			candidates = append(candidates, candidate)
		}
	}
	switch len(candidates) {
	case 1:
		return candidates[0], nil
	case 0:
		return "", validationError("harvester package is missing harvester.json")
	default:
		return "", validationError("harvester package contains multiple harvester roots")
	}
}

func unsafeRelPath(rel string) bool {
	clean := filepath.Clean(rel)
	return filepath.IsAbs(clean) || clean == ".." || strings.HasPrefix(clean, ".."+string(filepath.Separator))
}

func modeForInstalledFile(name string, original os.FileMode) os.FileMode {
	if original&0o111 != 0 {
		return 0o700
	}
	switch strings.ToLower(filepath.Ext(name)) {
	case ".js", ".sh", ".bash", ".zsh":
		return 0o700
	default:
		return 0o600
	}
}

func Run(options RunOptions) (RunResult, error) {
	if err := validateID(options.ID); err != nil {
		return RunResult{}, err
	}
	root, err := resolveRoot(options.Root)
	if err != nil {
		return RunResult{}, err
	}
	directory := filepath.Join(root, options.ID)
	manifest, err := LoadManifest(directory)
	if err != nil {
		return RunResult{}, err
	}
	invocation, err := resolveHarvesterInvocation(directory, manifest)
	if err != nil {
		return RunResult{}, err
	}
	timeout := options.Timeout
	if timeout <= 0 {
		timeout = defaultRunTimeout
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	args := append([]string{}, invocation.Args...)
	args = append(args, options.Args...)
	command := exec.CommandContext(ctx, invocation.Program, args...)
	command.Dir = directory
	command.Env = append(os.Environ(),
		"ASSETIWEAVE_HARVESTER_DIR="+directory,
		"ASSETIWEAVE_HARVESTER_ID="+manifest.ID,
	)
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	command.Stdout = &stdout
	command.Stderr = &stderr
	err = command.Run()
	if ctx.Err() == context.DeadlineExceeded {
		return RunResult{}, validationError("harvester %q timed out after %s", options.ID, timeout)
	}
	exitCode := 0
	if err != nil {
		exitCode = exitCodeOf(err)
		stdoutText := cappedString(stdout.String(), 64*1024)
		stderrText := cappedString(stderr.String(), 64*1024)
		return RunResult{
				ID:        manifest.ID,
				Directory: directory,
				Command:   append([]string{invocation.Program}, args...),
				ExitCode:  exitCode,
				Stdout:    stdoutText,
				Stderr:    stderrText,
			}, validationError("harvester %q failed with exit code %d", options.ID, exitCode).
				WithDetails(map[string]any{
					"id":        manifest.ID,
					"directory": directory,
					"exit_code": exitCode,
					"stdout":    stdoutText,
					"stderr":    stderrText,
				})
	}
	result := parseLastJSONObject(stdout.String())
	return RunResult{
		ID:             manifest.ID,
		Directory:      directory,
		Command:        append([]string{invocation.Program}, args...),
		ExitCode:       0,
		Stdout:         cappedString(stdout.String(), 64*1024),
		Stderr:         cappedString(stderr.String(), 64*1024),
		Result:         result,
		NormalizedFile: resolveNormalizedFile(directory, manifest),
	}, nil
}

func LoadManifest(directory string) (Manifest, error) {
	path := filepath.Join(filepath.Clean(directory), "harvester.json")
	data, err := os.ReadFile(path)
	if err != nil {
		return Manifest{}, validationError("harvester manifest not found: %s", path).WithCause(err)
	}
	var manifest Manifest
	if err := json.Unmarshal(data, &manifest); err != nil {
		return Manifest{}, validationError("invalid harvester manifest %s: %v", path, err).WithCause(err)
	}
	if err := validateManifest(manifest); err != nil {
		return Manifest{}, err
	}
	return manifest, nil
}

func readOfficialManifest(id string) (Manifest, error) {
	if err := validateID(id); err != nil {
		return Manifest{}, err
	}
	data, err := officialTemplates.ReadFile(filepath.ToSlash(filepath.Join("templates", id, "harvester.json")))
	if err != nil {
		return Manifest{}, validationError("official harvester template not found: %s", id).WithCause(err)
	}
	var manifest Manifest
	if err := json.Unmarshal(data, &manifest); err != nil {
		return Manifest{}, validationError("invalid official harvester manifest %s: %v", id, err).WithCause(err)
	}
	return manifest, validateManifest(manifest)
}

func validateManifest(manifest Manifest) error {
	if manifest.SchemaVersion != 1 {
		return validationError("harvester schema_version must be 1")
	}
	if err := validateID(manifest.ID); err != nil {
		return err
	}
	if strings.TrimSpace(manifest.Name) == "" {
		return validationError("harvester name is required")
	}
	if strings.TrimSpace(manifest.Version) == "" {
		return validationError("harvester version is required")
	}
	if manifest.Runtime != nil && len(manifest.Entrypoint) > 0 {
		return validationError("harvester %q must not declare both runtime and entrypoint", manifest.ID)
	}
	if manifest.Runtime != nil {
		return validateRuntimeSpec(*manifest.Runtime)
	}
	if len(manifest.Entrypoint) == 0 {
		return validationError("harvester entrypoint is required")
	}
	return nil
}

func resolveRoot(root string) (string, error) {
	root = strings.TrimSpace(root)
	if root != "" {
		return filepath.Clean(root), nil
	}
	return DefaultRoot()
}

func validateID(id string) error {
	if !validIDPattern.MatchString(strings.TrimSpace(id)) || strings.Contains(id, "..") {
		return validationError("invalid harvester id %q", id)
	}
	return nil
}

func resolveHarvesterInvocation(directory string, manifest Manifest) (entrypointInvocation, error) {
	if manifest.Runtime != nil {
		return resolveRuntimeInvocation(directory, *manifest.Runtime)
	}
	return resolveEntrypointInvocation(directory, manifest.Entrypoint)
}

func resolveEntrypointInvocation(directory string, entrypoint []string) (entrypointInvocation, error) {
	if len(entrypoint) == 0 || strings.TrimSpace(entrypoint[0]) == "" {
		return entrypointInvocation{}, validationError("harvester entrypoint is required")
	}
	commandPath, err := resolveCommand(directory, entrypoint[0])
	if err != nil {
		return entrypointInvocation{}, err
	}
	args := append([]string{}, entrypoint[1:]...)
	if isJavaScriptEntrypoint(commandPath) {
		return entrypointInvocation{
			Program: "node",
			Args:    append([]string{commandPath}, args...),
		}, nil
	}
	return entrypointInvocation{Program: commandPath, Args: args}, nil
}

func resolveRuntimeInvocation(directory string, runtimeSpec RuntimeSpec) (entrypointInvocation, error) {
	if err := validateRuntimeSpec(runtimeSpec); err != nil {
		return entrypointInvocation{}, err
	}
	entryPath, err := resolveCommand(directory, runtimeSpec.Entry)
	if err != nil {
		return entrypointInvocation{}, err
	}
	args := append([]string{}, runtimeSpec.Args...)
	switch strings.ToLower(strings.TrimSpace(runtimeSpec.Type)) {
	case "node":
		return entrypointInvocation{Program: "node", Args: append([]string{entryPath}, args...)}, nil
	case "python":
		if runtime.GOOS == "windows" {
			return entrypointInvocation{Program: "py", Args: append([]string{"-3", entryPath}, args...)}, nil
		}
		return entrypointInvocation{Program: "python3", Args: append([]string{entryPath}, args...)}, nil
	case "bash":
		return entrypointInvocation{Program: "bash", Args: append([]string{entryPath}, args...)}, nil
	case "executable":
		return entrypointInvocation{Program: entryPath, Args: args}, nil
	default:
		return entrypointInvocation{}, validationError("unsupported harvester runtime type: %s", runtimeSpec.Type)
	}
}

func validateRuntimeSpec(runtimeSpec RuntimeSpec) error {
	runtimeType := strings.ToLower(strings.TrimSpace(runtimeSpec.Type))
	switch runtimeType {
	case "node", "python", "bash", "executable":
	default:
		return validationError("unsupported harvester runtime type: %s", runtimeSpec.Type)
	}
	if strings.TrimSpace(runtimeSpec.Entry) == "" {
		return validationError("harvester runtime entry is required")
	}
	if strings.TrimSpace(runtimeSpec.Version) != "" && !validRuntimeVersionConstraint(runtimeSpec.Version) {
		return validationError("harvester runtime version constraint must use >=x[.y[.z]]: %s", runtimeSpec.Version)
	}
	return nil
}

func validRuntimeVersionConstraint(version string) bool {
	version = strings.TrimSpace(version)
	if !strings.HasPrefix(version, ">=") {
		return false
	}
	parts := strings.Split(strings.TrimSpace(strings.TrimPrefix(version, ">=")), ".")
	if len(parts) == 0 || len(parts) > 3 {
		return false
	}
	for _, part := range parts {
		if part == "" {
			return false
		}
		for _, character := range part {
			if character < '0' || character > '9' {
				return false
			}
		}
	}
	return true
}

func resolveCommand(directory, command string) (string, error) {
	command = strings.TrimSpace(command)
	if filepath.IsAbs(command) {
		return "", validationError("harvester entrypoint must be relative: %s", command)
	}
	clean := filepath.Clean(command)
	if strings.HasPrefix(clean, ".."+string(filepath.Separator)) || clean == ".." {
		return "", validationError("harvester entrypoint escapes its directory: %s", command)
	}
	path := filepath.Join(directory, clean)
	if runtime.GOOS == "windows" {
		return path, nil
	}
	if info, err := os.Stat(path); err != nil || info.IsDir() {
		if err != nil {
			return "", validationError("harvester entrypoint not found: %s", path).WithCause(err)
		}
		return "", validationError("harvester entrypoint is a directory: %s", path)
	}
	return path, nil
}

func isJavaScriptEntrypoint(path string) bool {
	switch strings.ToLower(filepath.Ext(path)) {
	case ".cjs", ".js", ".mjs":
		return true
	default:
		return false
	}
}

func resolveNormalizedFile(directory string, manifest Manifest) string {
	normalizedDir := manifest.Output.NormalizedDir
	if normalizedDir == "" {
		normalizedDir = "output/normalized"
	}
	file := manifest.Output.SessionsFile
	if file == "" {
		file = "sessions.json"
	}
	return resolveRelative(directory, filepath.Join(normalizedDir, file))
}

func resolveRelative(root, value string) string {
	if value == "" {
		return ""
	}
	if filepath.IsAbs(value) {
		return filepath.Clean(value)
	}
	return filepath.Join(root, filepath.Clean(value))
}

func pathExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func preserveInstalledState(rel string) bool {
	rel = filepath.ToSlash(filepath.Clean(rel))
	return strings.HasPrefix(rel, "requests/") || strings.HasPrefix(rel, "output/")
}

func exitCodeOf(err error) int {
	var exitErr *exec.ExitError
	if err != nil && strings.Contains(err.Error(), "executable file not found") {
		return 127
	}
	if err != nil && strings.Contains(err.Error(), "permission denied") {
		return 126
	}
	if err != nil && err.Error() != "" {
		if errors.As(err, &exitErr) {
			return exitErr.ExitCode()
		}
	}
	return 1
}

func parseLastJSONObject(stdout string) any {
	lines := strings.Split(stdout, "\n")
	for index := len(lines) - 1; index >= 0; index-- {
		line := strings.TrimSpace(lines[index])
		if line == "" || !strings.HasPrefix(line, "{") {
			continue
		}
		var value any
		if err := json.Unmarshal([]byte(line), &value); err == nil {
			return value
		}
	}
	return nil
}

func cappedString(value string, max int) string {
	if len(value) <= max {
		return value
	}
	return value[:max] + "\n... truncated " + strconv.Itoa(len(value)-max) + " bytes"
}

func validationError(format string, args ...any) *errs.ValidationError {
	return errs.NewValidationError(errs.SubtypeInvalidArgument, format, args...).
		WithCode("validation")
}

func internalError(format string, args ...any) error {
	return errs.NewInternalError(errs.SubtypeUnknown, format, args...).
		WithCode("internal")
}
