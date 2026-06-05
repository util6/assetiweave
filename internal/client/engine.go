package client

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
)

type EngineCaller interface {
	Call(ctx context.Context, method string, params any) (CallResult, error)
}

type CallResult struct {
	Data json.RawMessage
	Meta *protocol.EngineMeta
}

type EngineClient struct {
	Path string
}

type request struct {
	ID              string `json:"id"`
	Method          string `json:"method"`
	Params          any    `json:"params,omitempty"`
	ProtocolVersion int    `json:"protocol_version"`
	ContractVersion int    `json:"contract_version"`
}

type response struct {
	ID    string               `json:"id,omitempty"`
	OK    bool                 `json:"ok"`
	Data  json.RawMessage      `json:"data,omitempty"`
	Meta  *protocol.EngineMeta `json:"meta,omitempty"`
	Error *output.ErrDetail    `json:"error,omitempty"`
}

func NewEngineClient(path string) *EngineClient {
	return &EngineClient{Path: path}
}

func (c *EngineClient) Call(ctx context.Context, method string, params any) (CallResult, error) {
	enginePath, err := c.resolvePath()
	if err != nil {
		return CallResult{}, errs.NewEngineError(errs.SubtypeEngineNotFound, err.Error()).
			WithCode("engine_not_found").
			WithHint("build the engine with `cargo build -p assetiweave --bin assetiweave-engine`, or set ASSETIWEAVE_ENGINE").
			WithCause(err)
	}

	body, err := encodeRequest(method, params)
	if err != nil {
		return CallResult{}, errs.NewValidationError(errs.SubtypeInvalidArgument, "failed to encode request: %v", err).
			WithCode("validation").
			WithCause(err)
	}

	cmd := exec.CommandContext(ctx, enginePath)
	cmd.Stdin = bytes.NewReader(body)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return CallResult{}, errs.NewEngineError(errs.SubtypeEngineProcess, "engine process failed: %v; stderr: %s", err, stderr.String()).
			WithCode("engine_error").
			WithHint("run `assetiweave-cli doctor` for local diagnostics").
			WithCause(err)
	}

	return decodeResponseForMethod(method, stdout.Bytes())
}

func encodeRequest(method string, params any) ([]byte, error) {
	return json.Marshal(request{
		ID:              "1",
		Method:          method,
		Params:          params,
		ProtocolVersion: protocol.Version,
		ContractVersion: protocol.ContractVersion,
	})
}

func decodeResponse(body []byte) (CallResult, error) {
	return decodeResponseForMethod("", body)
}

func decodeResponseForMethod(method string, body []byte) (CallResult, error) {
	var resp response
	if err := json.Unmarshal(body, &resp); err != nil {
		return CallResult{}, errs.NewEngineError(errs.SubtypeEngineProtocol, "engine returned invalid JSON: %v", err).
			WithCode("engine_protocol").
			WithHint("check that ASSETIWEAVE_ENGINE points to assetiweave-engine").
			WithCause(err)
	}
	if resp.Meta == nil || (method != "system.version" && !protocol.Compatible(resp.Meta.ProtocolVersion, resp.Meta.ContractVersion)) {
		details := map[string]any{
			"expected_protocol_version": protocol.Version,
			"expected_contract_version": protocol.ContractVersion,
		}
		if resp.Meta != nil {
			details["received_protocol_version"] = resp.Meta.ProtocolVersion
			details["received_contract_version"] = resp.Meta.ContractVersion
			details["engine_version"] = resp.Meta.EngineVersion
		}
		return CallResult{}, errs.NewEngineError(errs.SubtypeEngineIncompatible, "Engine protocol or command contract is incompatible with this CLI").
			WithCode("engine_incompatible").
			WithHint("install the CLI and Engine from the same AssetIWeave release").
			WithDetails(details).
			WithMeta(resp.Meta)
	}
	if !resp.OK {
		if resp.Error == nil {
			resp.Error = &output.ErrDetail{Type: "engine_error", Code: "engine_error", Message: "engine returned an error without details"}
		}
		return CallResult{}, &output.ExitError{
			Code:   exitCodeForEngineError(resp.Error.Type),
			Detail: resp.Error,
			Meta:   resp.Meta,
		}
	}
	return CallResult{Data: resp.Data, Meta: resp.Meta}, nil
}

func exitCodeForEngineError(kind string) int {
	switch kind {
	case "confirmation_required":
		return output.ExitConfirmationRequired
	case "command_denied", "policy_invalid":
		return output.ExitPolicy
	case "validation", "invalid_json", "invalid_params", "unknown_method":
		return output.ExitValidation
	default:
		return output.ExitEngine
	}
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
