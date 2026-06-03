package client

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/util6/assetiweave/internal/output"
)

type EngineCaller interface {
	Call(ctx context.Context, method string, params any) (json.RawMessage, error)
}

type EngineClient struct {
	Path string
}

type request struct {
	ID     string `json:"id"`
	Method string `json:"method"`
	Params any    `json:"params,omitempty"`
}

type response struct {
	ID    string            `json:"id,omitempty"`
	OK    bool              `json:"ok"`
	Data  json.RawMessage   `json:"data,omitempty"`
	Error *output.ErrDetail `json:"error,omitempty"`
}

func NewEngineClient(path string) *EngineClient {
	return &EngineClient{Path: path}
}

func (c *EngineClient) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	enginePath, err := c.resolvePath()
	if err != nil {
		return nil, output.ErrWithHint(output.ExitEngine, "engine_not_found", err.Error(), "build the engine with `cargo build -p assetiweave --bin assetiweave-engine`, or set ASSETIWEAVE_ENGINE")
	}

	body, err := json.Marshal(request{ID: "1", Method: method, Params: params})
	if err != nil {
		return nil, output.Errorf(output.ExitValidation, "validation", "failed to encode request: %v", err)
	}

	cmd := exec.CommandContext(ctx, enginePath)
	cmd.Stdin = bytes.NewReader(body)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return nil, output.ErrWithHint(output.ExitEngine, "engine_error", fmt.Sprintf("engine process failed: %v; stderr: %s", err, stderr.String()), "run `assetiweave-cli doctor` for local diagnostics")
	}

	var resp response
	if err := json.Unmarshal(stdout.Bytes(), &resp); err != nil {
		return nil, output.ErrWithHint(output.ExitEngine, "engine_protocol", fmt.Sprintf("engine returned invalid JSON: %v", err), "check that ASSETIWEAVE_ENGINE points to assetiweave-engine")
	}
	if !resp.OK {
		if resp.Error == nil {
			resp.Error = &output.ErrDetail{Type: "engine_error", Code: "engine_error", Message: "engine returned an error without details"}
		}
		return nil, &output.ExitError{Code: output.ExitEngine, Detail: resp.Error}
	}
	return resp.Data, nil
}

func (c *EngineClient) resolvePath() (string, error) {
	if c.Path != "" {
		return c.Path, nil
	}
	if envPath := os.Getenv("ASSETIWEAVE_ENGINE"); envPath != "" {
		return envPath, nil
	}
	if path, err := exec.LookPath("assetiweave-engine"); err == nil {
		return path, nil
	}
	candidates := []string{
		filepath.Join("target", "debug", executableName("assetiweave-engine")),
		filepath.Join("src-tauri", "target", "debug", executableName("assetiweave-engine")),
	}
	if exe, err := os.Executable(); err == nil {
		dir := filepath.Dir(exe)
		candidates = append(candidates,
			filepath.Join(dir, executableName("assetiweave-engine")),
			filepath.Join(dir, "..", "target", "debug", executableName("assetiweave-engine")),
		)
	}
	for _, candidate := range candidates {
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
			return candidate, nil
		}
	}
	return "", fmt.Errorf("assetiweave-engine not found")
}

func executableName(name string) string {
	if filepath.Ext(name) != "" {
		return name
	}
	if os.PathSeparator == '\\' {
		return name + ".exe"
	}
	return name
}
