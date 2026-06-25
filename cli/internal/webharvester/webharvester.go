package webharvester

import (
	"bytes"
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/pbkdf2"
	"crypto/sha1"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/syndtr/goleveldb/leveldb"
	"github.com/util6/assetiweave/errs"
)

const (
	ConfigFileName         = "web-harvester.json"
	DefaultTimeout         = 30 * time.Second
	DefaultKeychainTimeout = 60 * time.Second
)

type Config struct {
	Version          int                    `json:"version"`
	SiteID           string                 `json:"site_id"`
	Name             string                 `json:"name"`
	AuthProbe        AuthProbeConfig        `json:"auth_probe"`
	List             ListConfig             `json:"list"`
	Detail           DetailConfig           `json:"detail"`
	Parser           ParserConfig           `json:"parser"`
	Output           OutputConfig           `json:"output"`
	GeneratedAdapter GeneratedAdapterConfig `json:"generated_adapter"`
}

type AuthProbeConfig struct {
	Request     string    `json:"request"`
	Success     Predicate `json:"success"`
	ExpiredWhen Predicate `json:"expired_when"`
}

type Predicate struct {
	Status       int    `json:"status,omitempty"`
	JSONPath     string `json:"json_path,omitempty"`
	BodyContains string `json:"body_contains,omitempty"`
}

type ListConfig struct {
	Request         string `json:"request"`
	SessionsPath    string `json:"sessions_path"`
	IDPath          string `json:"id_path"`
	TitlePath       string `json:"title_path"`
	UpdatedAtPath   string `json:"updated_at_path"`
	StartedAtPath   string `json:"started_at_path"`
	SourceURLPath   string `json:"source_url_path"`
	FingerprintPath string `json:"fingerprint_path"`
}

type DetailConfig struct {
	Request       string `json:"request"`
	MessagesPath  string `json:"messages_path"`
	IDPath        string `json:"id_path"`
	RolePath      string `json:"role_path"`
	TextPath      string `json:"text_path"`
	CreatedAtPath string `json:"created_at_path"`
}

type ParserConfig struct {
	UserRoles      []string `json:"user_roles"`
	AssistantRoles []string `json:"assistant_roles"`
	SystemRoles    []string `json:"system_roles"`
	ToolRoles      []string `json:"tool_roles"`
}

type OutputConfig struct {
	RawDir        string `json:"raw_dir"`
	NormalizedDir string `json:"normalized_dir"`
	SessionsFile  string `json:"sessions_file"`
}

type GeneratedAdapterConfig struct {
	Manifest string `json:"manifest"`
	Script   string `json:"script"`
}

type HTTPRequestTemplate struct {
	Method  string            `json:"method"`
	URL     string            `json:"url"`
	Headers map[string]string `json:"headers,omitempty"`
	Body    json.RawMessage   `json:"body,omitempty"`
}

type ScaffoldOptions struct {
	Directory string
	SiteID    string
	Name      string
	DryRun    bool
}

type ScaffoldResult struct {
	DryRun       bool     `json:"dry_run"`
	Directory    string   `json:"directory"`
	ConfigPath   string   `json:"config_path"`
	ManifestPath string   `json:"manifest_path"`
	AdapterPath  string   `json:"adapter_path"`
	Created      []string `json:"created,omitempty"`
}

type AuthCheckOptions struct {
	Directory string
	Client    *http.Client
}

type AuthDetectOptions struct {
	Directory  string
	Browser    string
	Profile    string
	Domain     string
	ProbeURL   string
	Credential string

	cookieLoader     func(BrowserProfile, string) ([]BrowserCookie, error)
	tokenLoader      func(BrowserProfile, []string) ([]BrowserToken, error)
	keychainPassword func(string) (string, error)
	profileResolver  func(string, string) ([]BrowserProfile, error)
}

type AuthCheckResult struct {
	SiteID     string `json:"site_id"`
	OK         bool   `json:"ok"`
	StatusCode int    `json:"status_code,omitempty"`
	Request    string `json:"request"`
	Message    string `json:"message"`
}

type AuthDetectResult struct {
	SiteID      string   `json:"site_id"`
	Browser     string   `json:"browser"`
	Profile     string   `json:"profile"`
	ProfilePath string   `json:"profile_path"`
	CookieDB    string   `json:"cookie_db"`
	Domain      string   `json:"domain"`
	ProbeURL    string   `json:"probe_url"`
	Credential  string   `json:"credential"`
	AuthOrigin  string   `json:"auth_origin,omitempty"`
	AuthKey     string   `json:"auth_key,omitempty"`
	CookieCount int      `json:"cookie_count"`
	Wrote       []string `json:"wrote"`
}

type SyncOptions struct {
	Directory string
	Client    *http.Client
	Limit     int
	Now       func() time.Time
}

type SyncResult struct {
	SiteID         string `json:"site_id"`
	RawRunDir      string `json:"raw_run_dir"`
	NormalizedFile string `json:"normalized_file"`
	SessionCount   int    `json:"session_count"`
	TurnCount      int    `json:"turn_count"`
}

type HTTPResponseSnapshot struct {
	StatusCode int               `json:"status_code"`
	Headers    map[string]string `json:"headers,omitempty"`
	Body       string            `json:"body"`
}

type BrowserProfile struct {
	Browser          string
	Profile          string
	ProfilePath      string
	CookieDB         string
	KeychainService  string
	LocalStoragePath string
}

type BrowserCookie struct {
	Host           string
	Name           string
	Value          string
	EncryptedValue []byte
	IsSecure       bool
	ExpiresUTC     int64
}

type BrowserToken struct {
	Origin string
	Key    string
	Value  string
}

type SessionsFile struct {
	Sessions []NormalizedConversationSession `json:"sessions"`
}

type NormalizedConversationSession struct {
	ExternalID        string                       `json:"external_id"`
	Title             *string                      `json:"title"`
	ProjectPath       *string                      `json:"project_path"`
	StartedAt         *string                      `json:"started_at"`
	UpdatedAt         *string                      `json:"updated_at"`
	SourceLocator     *string                      `json:"source_locator"`
	SourceFingerprint *string                      `json:"source_fingerprint"`
	Turns             []NormalizedConversationTurn `json:"turns"`
}

type NormalizedConversationTurn struct {
	ExternalID string                       `json:"external_id"`
	TurnIndex  int64                        `json:"turn_index"`
	UserText   string                       `json:"user_text"`
	Title      *string                      `json:"title"`
	StartedAt  *string                      `json:"started_at"`
	EndedAt    *string                      `json:"ended_at"`
	Parts      []NormalizedConversationPart `json:"parts"`
}

type NormalizedConversationPart struct {
	Role         string  `json:"role"`
	Kind         string  `json:"kind"`
	Text         *string `json:"text"`
	Language     *string `json:"language"`
	Command      *string `json:"command"`
	Cwd          *string `json:"cwd"`
	Status       *string `json:"status"`
	ExitCode     *int    `json:"exit_code"`
	MetadataJSON *string `json:"metadata_json"`
}

func Scaffold(options ScaffoldOptions) (ScaffoldResult, error) {
	if strings.TrimSpace(options.Directory) == "" {
		return ScaffoldResult{}, validationError("directory is required")
	}
	if strings.TrimSpace(options.SiteID) == "" {
		return ScaffoldResult{}, validationError("site id is required")
	}
	name := strings.TrimSpace(options.Name)
	if name == "" {
		name = options.SiteID
	}
	root := filepath.Clean(options.Directory)
	configPath := filepath.Join(root, ConfigFileName)
	manifestPath := filepath.Join(root, "conversation-adapter.json")
	adapterPath := filepath.Join(root, "adapter.js")
	result := ScaffoldResult{
		DryRun:       options.DryRun,
		Directory:    root,
		ConfigPath:   configPath,
		ManifestPath: manifestPath,
		AdapterPath:  adapterPath,
	}
	if options.DryRun {
		return result, nil
	}

	config := defaultConfig(options.SiteID, name)
	writes := map[string][]byte{
		configPath: mustJSON(config),
		filepath.Join(root, "requests", "auth-probe.json"): mustJSON(HTTPRequestTemplate{Method: "GET", URL: "", Headers: map[string]string{}}),
		filepath.Join(root, "requests", "list.json"):       mustJSON(HTTPRequestTemplate{Method: "GET", URL: "", Headers: map[string]string{}}),
		filepath.Join(root, "requests", "detail.json"):     mustJSON(HTTPRequestTemplate{Method: "GET", URL: "", Headers: map[string]string{}}),
		manifestPath: mustJSON(map[string]any{
			"schema_version":   1,
			"id":               options.SiteID,
			"name":             name,
			"version":          "0.1.0",
			"protocol_version": 1,
			"runtime": map[string]any{
				"type":    "node",
				"entry":   "adapter.js",
				"version": ">=20",
			},
			"capabilities": []string{"probe", "read_session", "web_records"},
			"input_kinds":  []string{"directory"},
		}),
		adapterPath: []byte(adapterScript),
	}
	for _, dir := range []string{
		filepath.Join(root, "fixtures"),
		filepath.Join(root, "raw"),
		filepath.Join(root, "normalized"),
		filepath.Join(root, "requests"),
	} {
		if err := os.MkdirAll(dir, 0o700); err != nil {
			return ScaffoldResult{}, internalError("create %s: %v", dir, err)
		}
		result.Created = append(result.Created, dir)
	}
	for path, content := range writes {
		if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
			return ScaffoldResult{}, internalError("create %s: %v", filepath.Dir(path), err)
		}
		mode := os.FileMode(0o600)
		if filepath.Base(path) == "adapter.js" {
			mode = 0o755
		}
		if err := os.WriteFile(path, content, mode); err != nil {
			return ScaffoldResult{}, internalError("write %s: %v", path, err)
		}
		result.Created = append(result.Created, path)
	}
	return result, nil
}

func AuthCheck(options AuthCheckOptions) (AuthCheckResult, error) {
	config, err := LoadConfig(options.Directory)
	if err != nil {
		return AuthCheckResult{}, err
	}
	requestPath := resolvePath(options.Directory, config.AuthProbe.Request)
	template, err := readRequestTemplate(requestPath, nil)
	if err != nil {
		return AuthCheckResult{}, authError("AUTH_NOT_CONFIGURED", "auth probe request is not configured", "fill requests/auth-probe.json with a logged-in web request template", map[string]any{"request": requestPath})
	}
	response, err := executeTemplate(template, options.Client)
	if err != nil {
		return AuthCheckResult{}, authError("NETWORK_BLOCKED", fmt.Sprintf("auth probe request failed: %v", err), "check network, proxy, and request headers", map[string]any{"request": requestPath}, err)
	}
	result := AuthCheckResult{
		SiteID:     config.SiteID,
		StatusCode: response.StatusCode,
		Request:    requestPath,
	}
	if predicateMatches(config.AuthProbe.ExpiredWhen, response) {
		result.Message = "authorization is expired or insufficient"
		return result, authError("AUTH_EXPIRED", result.Message, "refresh the web request template from a logged-in browser session", map[string]any{"status_code": response.StatusCode})
	}
	if !predicateMatches(config.AuthProbe.Success, response) {
		result.Message = "auth probe response did not match the configured success predicate"
		return result, authError("AUTH_PROBE_FAILED", result.Message, "update auth_probe.success in web-harvester.json or refresh the request template", map[string]any{"status_code": response.StatusCode})
	}
	result.OK = true
	result.Message = "authorization probe succeeded"
	return result, nil
}

func AuthDetect(options AuthDetectOptions) (AuthDetectResult, error) {
	config, err := LoadConfig(options.Directory)
	if err != nil {
		return AuthDetectResult{}, err
	}
	domain := strings.TrimSpace(options.Domain)
	if domain == "" {
		domain = "qianwen.com"
	}
	if !validCookieDomain(domain) {
		return AuthDetectResult{}, validationError("invalid cookie domain: %s", domain)
	}
	probeURL := strings.TrimSpace(options.ProbeURL)
	if probeURL == "" {
		probeURL = defaultProbeURLForDomain(domain)
	}
	credentialKinds, err := credentialOrder(options.Credential, domain)
	if err != nil {
		return AuthDetectResult{}, err
	}
	profileResolver := options.profileResolver
	if profileResolver == nil {
		profileResolver = resolveBrowserProfiles
	}
	profiles, err := profileResolver(options.Browser, options.Profile)
	if err != nil {
		return AuthDetectResult{}, err
	}
	tokenLoader := options.tokenLoader
	if tokenLoader == nil {
		tokenLoader = readChromiumLocalStorageTokens
	}
	cookieLoader := options.cookieLoader
	if cookieLoader == nil {
		keychainPassword := options.keychainPassword
		if keychainPassword == nil {
			keychainPassword = macOSKeychainPassword
		}
		cookieLoader = func(profile BrowserProfile, domain string) ([]BrowserCookie, error) {
			return readChromiumCookies(profile, domain, keychainPassword)
		}
	}

	var attempted []string
	var lastErr error
	for _, profile := range profiles {
		attempted = append(attempted, profile.Browser+":"+profile.Profile)
		for _, credentialKind := range credentialKinds {
			switch credentialKind {
			case "token":
				tokens, err := tokenLoader(profile, tokenOriginsForDomain(domain))
				if err != nil {
					lastErr = err
				}
				tokens = filterUsableTokens(tokens)
				if len(tokens) == 0 {
					continue
				}
				requestPath := resolvePath(options.Directory, config.AuthProbe.Request)
				template := authProbeTemplate(probeURL, "token", "Bearer "+tokens[0].Value)
				if err := writeJSON(requestPath, template); err != nil {
					return AuthDetectResult{}, err
				}
				return AuthDetectResult{
					SiteID:      config.SiteID,
					Browser:     profile.Browser,
					Profile:     profile.Profile,
					ProfilePath: profile.ProfilePath,
					CookieDB:    profile.CookieDB,
					Domain:      domain,
					ProbeURL:    probeURL,
					Credential:  "local_storage_token",
					AuthOrigin:  tokens[0].Origin,
					AuthKey:     tokens[0].Key,
					Wrote:       []string{requestPath},
				}, nil
			case "cookie":
				cookies, err := cookieLoader(profile, domain)
				if err != nil {
					lastErr = err
					if problem, ok := errs.ProblemOf(err); ok && problem.Code == "BROWSER_COOKIE_DECRYPT_FAILED" {
						return AuthDetectResult{}, err
					}
					continue
				}
				cookies = filterUsableCookies(cookies)
				if len(cookies) == 0 {
					continue
				}
				requestPath := resolvePath(options.Directory, config.AuthProbe.Request)
				template := authProbeTemplate(probeURL, "cookie", cookieHeader(cookies))
				if err := writeJSON(requestPath, template); err != nil {
					return AuthDetectResult{}, err
				}
				return AuthDetectResult{
					SiteID:      config.SiteID,
					Browser:     profile.Browser,
					Profile:     profile.Profile,
					ProfilePath: profile.ProfilePath,
					CookieDB:    profile.CookieDB,
					Domain:      domain,
					ProbeURL:    probeURL,
					Credential:  "cookie",
					CookieCount: len(cookies),
					Wrote:       []string{requestPath},
				}, nil
			}
		}
	}
	details := map[string]any{"domain": domain, "attempted": attempted}
	if lastErr != nil {
		details["last_error"] = lastErr.Error()
		if problem, ok := errs.ProblemOf(lastErr); ok && problem.Code == "BROWSER_COOKIE_DECRYPT_FAILED" {
			return AuthDetectResult{}, lastErr
		}
	}
	return AuthDetectResult{}, authError(
		"BROWSER_AUTH_NOT_FOUND",
		"browser login state was not found for "+domain,
		"open the target site in Chrome or Edge, sign in, then rerun auth-detect",
		details,
		lastErr,
	)
}

func Sync(options SyncOptions) (SyncResult, error) {
	config, err := LoadConfig(options.Directory)
	if err != nil {
		return SyncResult{}, err
	}
	if _, err := AuthCheck(AuthCheckOptions{Directory: options.Directory, Client: options.Client}); err != nil {
		return SyncResult{}, err
	}
	now := time.Now
	if options.Now != nil {
		now = options.Now
	}
	runID := now().UTC().Format("20060102T150405Z")
	rawRunDir := filepath.Join(resolvePath(options.Directory, config.Output.RawDir), runID)
	if err := os.MkdirAll(filepath.Join(rawRunDir, "details"), 0o700); err != nil {
		return SyncResult{}, internalError("create raw run directory: %v", err)
	}
	normalizedDir := resolvePath(options.Directory, config.Output.NormalizedDir)
	if err := os.MkdirAll(normalizedDir, 0o700); err != nil {
		return SyncResult{}, internalError("create normalized directory: %v", err)
	}

	listTemplate, err := readRequestTemplate(resolvePath(options.Directory, config.List.Request), nil)
	if err != nil {
		return SyncResult{}, authError("AUTH_NOT_CONFIGURED", "list request is not configured", "fill requests/list.json with a logged-in web request template", nil)
	}
	listResponse, err := executeTemplate(listTemplate, options.Client)
	if err != nil {
		return SyncResult{}, authError("NETWORK_BLOCKED", fmt.Sprintf("list request failed: %v", err), "check network, proxy, and request headers", nil, err)
	}
	if err := writeJSON(filepath.Join(rawRunDir, "list.json"), listResponse); err != nil {
		return SyncResult{}, err
	}
	listJSON, err := decodeJSON([]byte(listResponse.Body))
	if err != nil {
		return SyncResult{}, validationError("list response is not JSON: %v", err)
	}
	items, err := selectArray(listJSON, config.List.SessionsPath)
	if err != nil {
		return SyncResult{}, validationError("list sessions path failed: %v", err)
	}
	if options.Limit > 0 && len(items) > options.Limit {
		items = items[:options.Limit]
	}

	sessions := make([]NormalizedConversationSession, 0, len(items))
	for index, item := range items {
		sessionID := valueToText(selectValue(item, config.List.IDPath))
		if strings.TrimSpace(sessionID) == "" {
			return SyncResult{}, validationError("session list item %d has no id at path %q", index, config.List.IDPath)
		}
		replacements := map[string]string{"session_id": sessionID}
		detailTemplate, err := readRequestTemplate(resolvePath(options.Directory, config.Detail.Request), replacements)
		if err != nil {
			return SyncResult{}, err
		}
		detailResponse, err := executeTemplate(detailTemplate, options.Client)
		if err != nil {
			return SyncResult{}, authError("NETWORK_BLOCKED", fmt.Sprintf("detail request failed for %s: %v", sessionID, err), "check network, proxy, and request headers", map[string]any{"session_id": sessionID}, err)
		}
		if err := writeJSON(filepath.Join(rawRunDir, "details", safeFileName(sessionID)+".json"), detailResponse); err != nil {
			return SyncResult{}, err
		}
		detailJSON, err := decodeJSON([]byte(detailResponse.Body))
		if err != nil {
			return SyncResult{}, validationError("detail response for %s is not JSON: %v", sessionID, err)
		}
		session, err := normalizeSession(config, item, detailJSON, sessionID)
		if err != nil {
			return SyncResult{}, err
		}
		if len(session.Turns) > 0 {
			sessions = append(sessions, session)
		}
	}
	sessionsFile := filepath.Join(normalizedDir, config.Output.SessionsFile)
	if err := writeJSON(sessionsFile, SessionsFile{Sessions: sessions}); err != nil {
		return SyncResult{}, err
	}
	turnCount := 0
	for _, session := range sessions {
		turnCount += len(session.Turns)
	}
	return SyncResult{
		SiteID:         config.SiteID,
		RawRunDir:      rawRunDir,
		NormalizedFile: sessionsFile,
		SessionCount:   len(sessions),
		TurnCount:      turnCount,
	}, nil
}

func LoadConfig(directory string) (Config, error) {
	path := filepath.Join(filepath.Clean(directory), ConfigFileName)
	data, err := os.ReadFile(path)
	if err != nil {
		return Config{}, errs.NewConfigError(errs.SubtypeInvalidConfig, "web harvester config not found: %s", path).
			WithCode("invalid_config").
			WithHint("run `assetiweave-cli conversation web scaffold --directory <dir> --site <site>` first").
			WithCause(err)
	}
	var config Config
	if err := json.Unmarshal(data, &config); err != nil {
		return Config{}, errs.NewConfigError(errs.SubtypeInvalidJSON, "invalid web harvester config: %v", err).
			WithCode("invalid_json").
			WithCause(err)
	}
	if config.Version != 1 {
		return Config{}, errs.NewConfigError(errs.SubtypeInvalidConfig, "unsupported web harvester config version: %d", config.Version).
			WithCode("invalid_config")
	}
	return config, nil
}

func normalizeSession(config Config, listItem any, detailJSON any, sessionID string) (NormalizedConversationSession, error) {
	messages, err := selectArray(detailJSON, config.Detail.MessagesPath)
	if err != nil {
		return NormalizedConversationSession{}, validationError("detail messages path failed for %s: %v", sessionID, err)
	}
	title := optionalString(selectValue(listItem, config.List.TitlePath))
	updatedAt := optionalString(selectValue(listItem, config.List.UpdatedAtPath))
	startedAt := optionalString(selectValue(listItem, config.List.StartedAtPath))
	sourceLocator := optionalString(selectValue(listItem, config.List.SourceURLPath))
	fingerprint := optionalString(selectValue(listItem, config.List.FingerprintPath))
	turns := normalizeTurns(config, messages)
	return NormalizedConversationSession{
		ExternalID:        sessionID,
		Title:             title,
		ProjectPath:       nil,
		StartedAt:         startedAt,
		UpdatedAt:         updatedAt,
		SourceLocator:     sourceLocator,
		SourceFingerprint: fingerprint,
		Turns:             turns,
	}, nil
}

func normalizeTurns(config Config, messages []any) []NormalizedConversationTurn {
	var turns []NormalizedConversationTurn
	var current *NormalizedConversationTurn
	for _, message := range messages {
		if turn, ok := normalizeQwenRound(message, int64(len(turns))); ok {
			if current != nil {
				turns = append(turns, *current)
				current = nil
			}
			turns = append(turns, turn)
			continue
		}
		role := normalizeRole(valueToText(selectValue(message, config.Detail.RolePath)), config.Parser)
		text := strings.TrimSpace(valueToText(selectValue(message, config.Detail.TextPath)))
		if role == "" || text == "" {
			continue
		}
		messageID := valueToText(selectValue(message, config.Detail.IDPath))
		if role == "user" {
			if current != nil {
				turns = append(turns, *current)
			}
			if messageID == "" {
				messageID = fmt.Sprintf("turn-%d", len(turns)+1)
			}
			startedAt := optionalString(selectValue(message, config.Detail.CreatedAtPath))
			current = &NormalizedConversationTurn{
				ExternalID: messageID,
				TurnIndex:  int64(len(turns)),
				UserText:   text,
				StartedAt:  startedAt,
				Parts:      []NormalizedConversationPart{},
			}
			continue
		}
		if current == nil {
			continue
		}
		partText := text
		current.Parts = append(current.Parts, NormalizedConversationPart{
			Role: role,
			Kind: "text",
			Text: &partText,
		})
	}
	if current != nil {
		turns = append(turns, *current)
	}
	return turns
}

func normalizeQwenRound(message any, turnIndex int64) (NormalizedConversationTurn, bool) {
	requests, ok := selectValue(message, "request_messages").([]any)
	if !ok || len(requests) == 0 {
		return NormalizedConversationTurn{}, false
	}
	responses, _ := selectValue(message, "response_messages").([]any)
	userText := firstMessageContent(requests)
	if userText == "" {
		return NormalizedConversationTurn{}, false
	}
	externalID := valueToText(selectValue(message, "req_id"))
	if externalID == "" {
		externalID = fmt.Sprintf("turn-%d", turnIndex+1)
	}
	startedAt := optionalString(selectValue(message, "create_time"))
	endedAt := optionalString(selectValue(message, "update_time"))
	turn := NormalizedConversationTurn{
		ExternalID: externalID,
		TurnIndex:  turnIndex,
		UserText:   userText,
		StartedAt:  startedAt,
		EndedAt:    endedAt,
		Parts:      []NormalizedConversationPart{},
	}
	for _, response := range responses {
		text := strings.TrimSpace(valueToText(selectValue(response, "content")))
		if text == "" {
			continue
		}
		partText := text
		turn.Parts = append(turn.Parts, NormalizedConversationPart{
			Role: "assistant",
			Kind: "text",
			Text: &partText,
		})
	}
	return turn, true
}

func firstMessageContent(messages []any) string {
	for _, message := range messages {
		text := strings.TrimSpace(valueToText(selectValue(message, "content")))
		if text != "" {
			return text
		}
	}
	return ""
}

func readRequestTemplate(path string, replacements map[string]string) (HTTPRequestTemplate, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return HTTPRequestTemplate{}, err
	}
	if len(replacements) > 0 {
		text := string(data)
		for key, value := range replacements {
			text = strings.ReplaceAll(text, "{"+key+"}", value)
		}
		data = []byte(text)
	}
	var template HTTPRequestTemplate
	if err := json.Unmarshal(data, &template); err != nil {
		return HTTPRequestTemplate{}, err
	}
	if strings.TrimSpace(template.URL) == "" {
		return HTTPRequestTemplate{}, fmt.Errorf("request URL is empty")
	}
	if template.Method == "" {
		template.Method = http.MethodGet
	}
	return template, nil
}

func executeTemplate(template HTTPRequestTemplate, client *http.Client) (HTTPResponseSnapshot, error) {
	if client == nil {
		client = &http.Client{Timeout: DefaultTimeout}
	}
	body := io.Reader(nil)
	if len(template.Body) > 0 && string(template.Body) != "null" {
		body = bytes.NewReader(template.Body)
	}
	request, err := http.NewRequest(strings.ToUpper(template.Method), template.URL, body)
	if err != nil {
		return HTTPResponseSnapshot{}, err
	}
	for key, value := range template.Headers {
		request.Header.Set(key, value)
	}
	response, err := client.Do(request)
	if err != nil {
		return HTTPResponseSnapshot{}, err
	}
	defer response.Body.Close()
	bodyBytes, err := io.ReadAll(io.LimitReader(response.Body, 64*1024*1024))
	if err != nil {
		return HTTPResponseSnapshot{}, err
	}
	headers := map[string]string{}
	for key, values := range response.Header {
		headers[key] = strings.Join(values, ", ")
	}
	return HTTPResponseSnapshot{
		StatusCode: response.StatusCode,
		Headers:    headers,
		Body:       string(bodyBytes),
	}, nil
}

func resolveBrowserProfiles(browser, profile string) ([]BrowserProfile, error) {
	if runtime.GOOS != "darwin" {
		return nil, authError("BROWSER_AUTH_UNSUPPORTED", "browser auth detection is only supported on macOS in this build", "provide request templates manually on this platform", map[string]any{"goos": runtime.GOOS})
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return nil, internalError("resolve home directory: %v", err)
	}
	profile = strings.TrimSpace(profile)
	if profile == "" {
		profile = "Default"
	}
	browser = strings.ToLower(strings.TrimSpace(browser))
	if browser == "" {
		browser = "auto"
	}
	appSupport := filepath.Join(home, "Library", "Application Support")
	candidates := []struct {
		id      string
		root    string
		service string
	}{
		{id: "edge", root: filepath.Join(appSupport, "Microsoft Edge"), service: "Microsoft Edge Safe Storage"},
		{id: "chrome", root: filepath.Join(appSupport, "Google", "Chrome"), service: "Chrome Safe Storage"},
		{id: "brave", root: filepath.Join(appSupport, "BraveSoftware", "Brave-Browser"), service: "Brave Safe Storage"},
		{id: "chromium", root: filepath.Join(appSupport, "Chromium"), service: "Chromium Safe Storage"},
	}
	var profiles []BrowserProfile
	for _, candidate := range candidates {
		if browser != "auto" && browser != candidate.id {
			continue
		}
		profilePath := filepath.Join(candidate.root, profile)
		cookieDB := chromiumCookieDB(profilePath)
		if cookieDB == "" {
			if browser != "auto" {
				return nil, authError("BROWSER_PROFILE_NOT_FOUND", "browser profile cookie database was not found", "check --browser and --profile", map[string]any{"browser": browser, "profile": profile, "profile_path": profilePath})
			}
			continue
		}
		profiles = append(profiles, BrowserProfile{
			Browser:          candidate.id,
			Profile:          profile,
			ProfilePath:      profilePath,
			CookieDB:         cookieDB,
			KeychainService:  candidate.service,
			LocalStoragePath: filepath.Join(profilePath, "Local Storage", "leveldb"),
		})
	}
	if len(profiles) == 0 {
		return nil, authError("BROWSER_PROFILE_NOT_FOUND", "no supported browser profile was found", "open the target site in Chrome or Edge, then rerun auth-detect", map[string]any{"browser": browser, "profile": profile})
	}
	return profiles, nil
}

func chromiumCookieDB(profilePath string) string {
	for _, candidate := range []string{
		filepath.Join(profilePath, "Network", "Cookies"),
		filepath.Join(profilePath, "Cookies"),
	} {
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
			return candidate
		}
	}
	return ""
}

func readChromiumLocalStorageTokens(profile BrowserProfile, origins []string) ([]BrowserToken, error) {
	if profile.LocalStoragePath == "" {
		return nil, nil
	}
	info, err := os.Stat(profile.LocalStoragePath)
	if err != nil || !info.IsDir() {
		return nil, nil
	}
	tempDir, err := os.MkdirTemp("", "assetiweave-browser-local-storage-*")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(tempDir)
	tempLevelDB := filepath.Join(tempDir, "leveldb")
	if err := copyLevelDBDirectory(profile.LocalStoragePath, tempLevelDB); err != nil {
		return nil, err
	}
	db, err := leveldb.OpenFile(tempLevelDB, nil)
	if err != nil {
		return nil, err
	}
	defer db.Close()

	originBytes := make([][]byte, 0, len(origins))
	for _, origin := range origins {
		if strings.TrimSpace(origin) != "" {
			originBytes = append(originBytes, []byte(origin))
		}
	}
	var tokens []BrowserToken
	iterator := db.NewIterator(nil, nil)
	defer iterator.Release()
	for iterator.Next() {
		key := append([]byte(nil), iterator.Key()...)
		value := append([]byte(nil), iterator.Value()...)
		origin := matchedOrigin(key, value, originBytes)
		if origin == "" {
			continue
		}
		for _, candidate := range tokenCandidatesFromStorageRecord(key, value) {
			tokens = append(tokens, BrowserToken{
				Origin: origin,
				Key:    storageKeyName(key),
				Value:  candidate,
			})
		}
	}
	if err := iterator.Error(); err != nil {
		return nil, err
	}
	return tokens, nil
}

func copyLevelDBDirectory(source, target string) error {
	if err := os.MkdirAll(target, 0o700); err != nil {
		return err
	}
	entries, err := os.ReadDir(source)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		if name == "LOCK" || strings.HasSuffix(name, ".tmp") {
			continue
		}
		if !(name == "CURRENT" || name == "MANIFEST" || strings.HasPrefix(name, "MANIFEST-") || strings.HasSuffix(name, ".ldb") || strings.HasSuffix(name, ".log")) {
			continue
		}
		data, err := os.ReadFile(filepath.Join(source, name))
		if err != nil {
			continue
		}
		if err := os.WriteFile(filepath.Join(target, name), data, 0o600); err != nil {
			return err
		}
	}
	return nil
}

func matchedOrigin(key, value []byte, origins [][]byte) string {
	for _, origin := range origins {
		if bytes.Contains(key, origin) || bytes.Contains(value, origin) {
			return string(origin)
		}
	}
	return ""
}

var tokenJSONPattern = regexp.MustCompile(`(?i)"(?:access_)?token"\s*:\s*"([^"]+)"`)

func tokenCandidatesFromStorageRecord(key, value []byte) []string {
	text := storageText(value)
	keyText := strings.ToLower(storageText(key))
	var candidates []string
	for _, match := range tokenJSONPattern.FindAllStringSubmatch(text, -1) {
		candidates = append(candidates, match[1])
	}
	if strings.Contains(keyText, "token") {
		candidates = append(candidates, text)
	}
	result := make([]string, 0, len(candidates))
	seen := map[string]bool{}
	for _, candidate := range candidates {
		candidate = strings.Trim(strings.TrimSpace(candidate), "\"'")
		if !plausibleBearerToken(candidate) || seen[candidate] {
			continue
		}
		seen[candidate] = true
		result = append(result, candidate)
	}
	return result
}

func storageText(value []byte) string {
	if len(value) == 0 {
		return ""
	}
	zeroCount := 0
	for _, ch := range value {
		if ch == 0 {
			zeroCount++
		}
	}
	if zeroCount > len(value)/4 {
		withoutZero := make([]byte, 0, len(value)-zeroCount)
		for _, ch := range value {
			if ch != 0 {
				withoutZero = append(withoutZero, ch)
			}
		}
		value = withoutZero
	}
	text := string(value)
	return strings.Map(func(ch rune) rune {
		if ch == '\t' || ch == '\n' || ch == '\r' || (ch >= 32 && ch != 127) {
			return ch
		}
		return -1
	}, text)
}

func storageKeyName(key []byte) string {
	parts := bytes.Split(key, []byte{0})
	text := ""
	if len(parts) > 0 {
		text = storageText(parts[len(parts)-1])
	}
	if strings.TrimSpace(text) == "" {
		text = storageText(key)
	}
	text = strings.TrimSpace(text)
	if len(text) > 80 {
		return text[:80]
	}
	return text
}

func plausibleBearerToken(value string) bool {
	if len(value) < 10 || len(value) > 4096 {
		return false
	}
	if strings.ContainsAny(value, " \t\r\n") {
		return false
	}
	return true
}

func filterUsableTokens(tokens []BrowserToken) []BrowserToken {
	result := make([]BrowserToken, 0, len(tokens))
	seen := map[string]bool{}
	for _, token := range tokens {
		if !plausibleBearerToken(token.Value) || seen[token.Value] {
			continue
		}
		seen[token.Value] = true
		result = append(result, token)
	}
	sort.SliceStable(result, func(left, right int) bool {
		leftPriority := tokenPriority(result[left])
		rightPriority := tokenPriority(result[right])
		if leftPriority != rightPriority {
			return leftPriority < rightPriority
		}
		return result[left].Origin < result[right].Origin
	})
	return result
}

func tokenPriority(token BrowserToken) int {
	key := strings.ToLower(strings.TrimSpace(token.Key))
	origin := strings.ToLower(strings.TrimSpace(token.Origin))
	switch {
	case origin == "https://chatgpt.com" && (key == "access_token" || key == "accesstoken" || key == "token"):
		return 0
	case key == "token" && origin == "https://chat.qwen.ai":
		return 0
	case key == "token":
		return 1
	case strings.Contains(key, "access_token"):
		return 2
	case strings.Contains(key, "token"):
		return 3
	default:
		return 4
	}
}

func readChromiumCookies(profile BrowserProfile, domain string, keychainPassword func(string) (string, error)) ([]BrowserCookie, error) {
	if profile.CookieDB == "" {
		return nil, fmt.Errorf("cookie database path is empty")
	}
	tempDir, err := os.MkdirTemp("", "assetiweave-browser-cookies-*")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(tempDir)
	tempDB := filepath.Join(tempDir, "Cookies")
	data, err := os.ReadFile(profile.CookieDB)
	if err != nil {
		return nil, err
	}
	if err := os.WriteFile(tempDB, data, 0o600); err != nil {
		return nil, err
	}
	rows, err := queryChromiumCookieRows(tempDB, domain)
	if err != nil {
		return nil, err
	}
	requiresPassword := false
	for _, row := range rows {
		if row.Value == "" && row.EncryptedHex != "" {
			requiresPassword = true
			break
		}
	}
	password := ""
	if requiresPassword {
		if keychainPassword == nil {
			return nil, fmt.Errorf("encrypted cookies require a keychain password")
		}
		password, err = keychainPassword(profile.KeychainService)
		if err != nil {
			return nil, authError("BROWSER_COOKIE_DECRYPT_FAILED", "failed to read browser cookie encryption key", "grant Keychain access for the browser safe storage item or provide request templates manually", map[string]any{"browser": profile.Browser, "service": profile.KeychainService}, err)
		}
	}
	cookies := make([]BrowserCookie, 0, len(rows))
	for _, row := range rows {
		value := row.Value
		encryptedValue := []byte(nil)
		if value == "" && row.EncryptedHex != "" {
			encryptedValue, err = hexDecode(row.EncryptedHex)
			if err != nil {
				return nil, err
			}
			value, err = decryptChromiumMacCookie(encryptedValue, password, row.Host)
			if err != nil {
				return nil, authError("BROWSER_COOKIE_DECRYPT_FAILED", "failed to decrypt browser cookie", "grant Keychain access for the browser safe storage item or provide request templates manually", map[string]any{"browser": profile.Browser, "cookie": row.Name}, err)
			}
		}
		cookies = append(cookies, BrowserCookie{
			Host:           row.Host,
			Name:           row.Name,
			Value:          value,
			EncryptedValue: encryptedValue,
			IsSecure:       row.IsSecure != 0,
			ExpiresUTC:     row.ExpiresUTC,
		})
	}
	return cookies, nil
}

type chromiumCookieRow struct {
	Host         string `json:"host"`
	Name         string `json:"name"`
	Value        string `json:"value"`
	EncryptedHex string `json:"encrypted_hex"`
	IsSecure     int    `json:"is_secure"`
	ExpiresUTC   int64  `json:"expires_utc"`
}

func queryChromiumCookieRows(cookieDB, domain string) ([]chromiumCookieRow, error) {
	if !validCookieDomain(domain) {
		return nil, fmt.Errorf("invalid cookie domain: %s", domain)
	}
	domainLiteral := strings.ReplaceAll(domain, "'", "''")
	query := fmt.Sprintf(`
select
  host_key as host,
  name,
  value,
  hex(encrypted_value) as encrypted_hex,
  is_secure,
  expires_utc
from cookies
where host_key = '%[1]s'
   or host_key = '.%[1]s'
   or host_key like '%%.%[1]s'
order by host_key, name;
`, domainLiteral)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	output, err := exec.CommandContext(ctx, "sqlite3", "-json", cookieDB, query).Output()
	if err != nil {
		return nil, err
	}
	var rows []chromiumCookieRow
	if len(bytes.TrimSpace(output)) == 0 {
		return rows, nil
	}
	if err := json.Unmarshal(output, &rows); err != nil {
		return nil, err
	}
	return rows, nil
}

func macOSKeychainPassword(service string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), DefaultKeychainTimeout)
	defer cancel()
	output, err := exec.CommandContext(ctx, "security", "find-generic-password", "-w", "-s", service).Output()
	if ctx.Err() == context.DeadlineExceeded {
		return "", fmt.Errorf("keychain access timed out for %s", service)
	}
	if err != nil {
		return "", err
	}
	password := strings.TrimRight(string(output), "\r\n")
	if password == "" {
		return "", fmt.Errorf("empty keychain password for %s", service)
	}
	return password, nil
}

func decryptChromiumMacCookie(encryptedValue []byte, password, hostKey string) (string, error) {
	if len(encryptedValue) == 0 {
		return "", nil
	}
	ciphertext := encryptedValue
	if bytes.HasPrefix(ciphertext, []byte("v10")) || bytes.HasPrefix(ciphertext, []byte("v11")) {
		ciphertext = ciphertext[3:]
	}
	if len(ciphertext)%aes.BlockSize != 0 {
		return "", fmt.Errorf("encrypted cookie length is not a multiple of AES block size")
	}
	key, err := pbkdf2.Key(sha1.New, password, []byte("saltysalt"), 1003, 16)
	if err != nil {
		return "", err
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return "", err
	}
	plaintext := make([]byte, len(ciphertext))
	iv := bytes.Repeat([]byte(" "), aes.BlockSize)
	cipher.NewCBCDecrypter(block, iv).CryptBlocks(plaintext, ciphertext)
	plaintext, err = removePKCS7Padding(plaintext, aes.BlockSize)
	if err != nil {
		return "", err
	}
	plaintext = removeChromiumHostKeyPrefix(plaintext, hostKey)
	return string(plaintext), nil
}

func removeChromiumHostKeyPrefix(plaintext []byte, hostKey string) []byte {
	if len(plaintext) < sha256.Size || hostKey == "" {
		return plaintext
	}
	hash := sha256.Sum256([]byte(hostKey))
	if bytes.Equal(plaintext[:sha256.Size], hash[:]) {
		return plaintext[sha256.Size:]
	}
	return plaintext
}

func removePKCS7Padding(value []byte, blockSize int) ([]byte, error) {
	if len(value) == 0 || len(value)%blockSize != 0 {
		return nil, fmt.Errorf("invalid padded value length")
	}
	padding := int(value[len(value)-1])
	if padding == 0 || padding > blockSize || padding > len(value) {
		return nil, fmt.Errorf("invalid PKCS7 padding")
	}
	for _, ch := range value[len(value)-padding:] {
		if int(ch) != padding {
			return nil, fmt.Errorf("invalid PKCS7 padding bytes")
		}
	}
	return value[:len(value)-padding], nil
}

func hexDecode(value string) ([]byte, error) {
	if len(value)%2 != 0 {
		return nil, fmt.Errorf("invalid hex length")
	}
	result := make([]byte, len(value)/2)
	for index := 0; index < len(result); index++ {
		high, ok := hexNibble(value[index*2])
		if !ok {
			return nil, fmt.Errorf("invalid hex")
		}
		low, ok := hexNibble(value[index*2+1])
		if !ok {
			return nil, fmt.Errorf("invalid hex")
		}
		result[index] = high<<4 | low
	}
	return result, nil
}

func hexNibble(ch byte) (byte, bool) {
	switch {
	case ch >= '0' && ch <= '9':
		return ch - '0', true
	case ch >= 'a' && ch <= 'f':
		return ch - 'a' + 10, true
	case ch >= 'A' && ch <= 'F':
		return ch - 'A' + 10, true
	default:
		return 0, false
	}
}

func filterUsableCookies(cookies []BrowserCookie) []BrowserCookie {
	now := time.Now()
	result := make([]BrowserCookie, 0, len(cookies))
	for _, cookie := range cookies {
		if strings.TrimSpace(cookie.Name) == "" || cookie.Value == "" {
			continue
		}
		if !validCookiePair(cookie.Name, cookie.Value) {
			continue
		}
		if chromiumCookieExpired(cookie.ExpiresUTC, now) {
			continue
		}
		result = append(result, cookie)
	}
	return result
}

func chromiumCookieExpired(expiresUTC int64, now time.Time) bool {
	if expiresUTC <= 0 {
		return false
	}
	const unixEpochOffsetMicros = int64(11644473600000000)
	unixMicros := expiresUTC - unixEpochOffsetMicros
	if unixMicros <= 0 {
		return false
	}
	return time.UnixMicro(unixMicros).Before(now)
}

func cookieHeader(cookies []BrowserCookie) string {
	cookies = append([]BrowserCookie(nil), cookies...)
	sort.SliceStable(cookies, func(left, right int) bool {
		if cookies[left].Host != cookies[right].Host {
			return len(cookies[left].Host) > len(cookies[right].Host)
		}
		return cookies[left].Name < cookies[right].Name
	})
	parts := make([]string, 0, len(cookies))
	seen := map[string]bool{}
	for _, cookie := range cookies {
		if seen[cookie.Name] {
			continue
		}
		seen[cookie.Name] = true
		parts = append(parts, cookie.Name+"="+cookie.Value)
	}
	return strings.Join(parts, "; ")
}

func validCookiePair(name, value string) bool {
	if strings.ContainsAny(name, "=;,\r\n\t ") {
		return false
	}
	for _, ch := range name {
		if ch < 33 || ch == 127 {
			return false
		}
	}
	for _, ch := range value {
		if ch < 32 || ch == 127 || ch == ';' || ch == '\r' || ch == '\n' {
			return false
		}
	}
	return true
}

func validCookieDomain(domain string) bool {
	if domain == "" || strings.HasPrefix(domain, ".") || strings.Contains(domain, "/") {
		return false
	}
	for _, ch := range domain {
		if (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9') || ch == '-' || ch == '.' {
			continue
		}
		return false
	}
	return strings.Contains(domain, ".")
}

func credentialOrder(credential, domain string) ([]string, error) {
	credential = strings.ToLower(strings.TrimSpace(credential))
	if credential == "" {
		credential = "auto"
	}
	switch credential {
	case "cookie":
		return []string{"cookie"}, nil
	case "token":
		return []string{"token"}, nil
	case "auto":
		if isQwenDomain(domain) || isChatGPTDomain(domain) {
			return []string{"cookie", "token"}, nil
		}
		return []string{"token", "cookie"}, nil
	default:
		return nil, validationError("unsupported credential mode: %s", credential)
	}
}

func authProbeTemplate(probeURL, credentialKind, credentialValue string) HTTPRequestTemplate {
	template := HTTPRequestTemplate{
		Method: http.MethodGet,
		URL:    probeURL,
		Headers: map[string]string{
			"Accept":     "application/json, text/plain, */*",
			"Origin":     originForURL(probeURL),
			"Referer":    refererForURL(probeURL),
			"User-Agent": defaultBrowserUserAgent(),
		},
	}
	if credentialKind == "token" {
		template.Headers["Authorization"] = credentialValue
	} else {
		template.Headers["Cookie"] = credentialValue
	}
	if strings.Contains(probeURL, "/growth/user/benefit/user/member/info") {
		template.Method = http.MethodPost
		template.Headers["Content-Type"] = "application/json"
		template.Body = json.RawMessage(`{"clientChannel":"PC"}`)
	}
	return template
}

func defaultProbeURLForDomain(domain string) string {
	if isQwenDomain(domain) {
		return "https://api.qianwen.com/growth/user/benefit/user/member/info"
	}
	if domain == "qwen.ai" || strings.HasSuffix(domain, ".qwen.ai") {
		return "https://chat.qwen.ai/api/v1/auths/"
	}
	if isChatGPTDomain(domain) {
		return "https://chatgpt.com/api/auth/session"
	}
	return "https://" + domain + "/"
}

func tokenOriginsForDomain(domain string) []string {
	if isQwenDomain(domain) {
		return []string{
			"https://chat.qwen.ai",
			"https://www.qianwen.com",
			"https://qianwen.com",
		}
	}
	if isChatGPTDomain(domain) {
		return []string{
			"https://chatgpt.com",
			"https://chat.openai.com",
		}
	}
	return []string{"https://" + domain}
}

func isQwenDomain(domain string) bool {
	return domain == "qianwen.com" || strings.HasSuffix(domain, ".qianwen.com")
}

func isChatGPTDomain(domain string) bool {
	return domain == "chatgpt.com" || strings.HasSuffix(domain, ".chatgpt.com")
}

func originForURL(value string) string {
	parsed, err := url.Parse(value)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return ""
	}
	return parsed.Scheme + "://" + parsed.Host
}

func refererForURL(value string) string {
	origin := originForURL(value)
	if origin == "" {
		return ""
	}
	return origin + "/"
}

func defaultBrowserUserAgent() string {
	return "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
}

func predicateMatches(predicate Predicate, response HTTPResponseSnapshot) bool {
	if predicate.Status != 0 && predicate.Status != response.StatusCode {
		return false
	}
	if predicate.BodyContains != "" && !strings.Contains(string(response.Body), predicate.BodyContains) {
		return false
	}
	if predicate.JSONPath != "" {
		value, err := decodeJSON([]byte(response.Body))
		if err != nil {
			return false
		}
		if selectValue(value, predicate.JSONPath) == nil {
			return false
		}
	}
	if predicate.Status == 0 && predicate.BodyContains == "" && predicate.JSONPath == "" {
		return false
	}
	return true
}

func decodeJSON(data []byte) (any, error) {
	var value any
	if err := json.Unmarshal(data, &value); err != nil {
		return nil, err
	}
	return value, nil
}

func selectArray(value any, path string) ([]any, error) {
	selected := selectValue(value, path)
	array, ok := selected.([]any)
	if !ok {
		return nil, fmt.Errorf("path %q did not resolve to an array", path)
	}
	return array, nil
}

func selectValue(value any, path string) any {
	if strings.TrimSpace(path) == "" {
		return value
	}
	current := value
	for _, segment := range strings.Split(path, ".") {
		if segment == "" {
			continue
		}
		switch typed := current.(type) {
		case map[string]any:
			current = typed[segment]
		case []any:
			index, err := strconv.Atoi(segment)
			if err != nil || index < 0 || index >= len(typed) {
				return nil
			}
			current = typed[index]
		default:
			return nil
		}
	}
	return current
}

func valueToText(value any) string {
	switch typed := value.(type) {
	case nil:
		return ""
	case string:
		return typed
	case float64:
		return strconv.FormatFloat(typed, 'f', -1, 64)
	case bool:
		return strconv.FormatBool(typed)
	default:
		data, err := json.Marshal(typed)
		if err != nil {
			return ""
		}
		return string(data)
	}
}

func optionalString(value any) *string {
	text := strings.TrimSpace(valueToText(value))
	if text == "" {
		return nil
	}
	return &text
}

func normalizeRole(role string, parser ParserConfig) string {
	role = strings.ToLower(strings.TrimSpace(role))
	if containsRole(parser.UserRoles, role) {
		return "user"
	}
	if containsRole(parser.AssistantRoles, role) {
		return "assistant"
	}
	if containsRole(parser.SystemRoles, role) {
		return "system"
	}
	if containsRole(parser.ToolRoles, role) {
		return "tool"
	}
	return ""
}

func containsRole(roles []string, role string) bool {
	for _, candidate := range roles {
		if strings.ToLower(strings.TrimSpace(candidate)) == role {
			return true
		}
	}
	return false
}

func defaultConfig(siteID, name string) Config {
	return Config{
		Version: 1,
		SiteID:  siteID,
		Name:    name,
		AuthProbe: AuthProbeConfig{
			Request: "requests/auth-probe.json",
			Success: Predicate{
				Status: 200,
			},
			ExpiredWhen: Predicate{
				Status: 401,
			},
		},
		List: ListConfig{
			Request:       "requests/list.json",
			SessionsPath:  "data.sessions",
			IDPath:        "id",
			TitlePath:     "title",
			UpdatedAtPath: "updated_at",
		},
		Detail: DetailConfig{
			Request:       "requests/detail.json",
			MessagesPath:  "data.messages",
			IDPath:        "id",
			RolePath:      "role",
			TextPath:      "content",
			CreatedAtPath: "created_at",
		},
		Parser: ParserConfig{
			UserRoles:      []string{"user"},
			AssistantRoles: []string{"assistant", "model"},
			SystemRoles:    []string{"system"},
			ToolRoles:      []string{"tool"},
		},
		Output: OutputConfig{
			RawDir:        "raw",
			NormalizedDir: "normalized",
			SessionsFile:  "sessions.json",
		},
		GeneratedAdapter: GeneratedAdapterConfig{
			Manifest: "conversation-adapter.json",
			Script:   "adapter.js",
		},
	}
}

func resolvePath(root, path string) string {
	if filepath.IsAbs(path) {
		return path
	}
	return filepath.Join(filepath.Clean(root), path)
}

func writeJSON(path string, value any) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return internalError("create %s: %v", filepath.Dir(path), err)
	}
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return internalError("encode %s: %v", path, err)
	}
	data = append(data, '\n')
	if err := os.WriteFile(path, data, 0o600); err != nil {
		return internalError("write %s: %v", path, err)
	}
	return nil
}

func mustJSON(value any) []byte {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		panic(err)
	}
	return append(data, '\n')
}

func safeFileName(value string) string {
	var builder strings.Builder
	for _, ch := range value {
		if (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9') || ch == '-' || ch == '_' || ch == '.' {
			builder.WriteRune(ch)
		} else {
			builder.WriteRune('_')
		}
	}
	if builder.Len() == 0 {
		return "session"
	}
	return builder.String()
}

func validationError(format string, args ...any) error {
	return errs.NewValidationError(errs.SubtypeInvalidArgument, format, args...).
		WithCode("validation")
}

func authError(code, message, hint string, details any, causes ...error) error {
	err := errs.NewConfigError(errs.SubtypeInvalidConfig, message).
		WithCode(code).
		WithHint(hint)
	if details != nil {
		err.WithDetails(details)
	}
	if len(causes) > 0 && causes[0] != nil {
		err.WithCause(causes[0])
	}
	return err
}

func internalError(format string, args ...any) error {
	return errs.NewInternalError(errs.SubtypeUnknown, format, args...).
		WithCode("internal")
}

const adapterScript = `#!/usr/bin/env node
const fs = require("fs");
const path = require("path");

const CONTENT_CARD_SCHEMA = "web-content-cards-v1";

function emit(value) {
  process.stdout.write(JSON.stringify(value) + "\n");
}

let request = {};
try {
  const input = fs.readFileSync(0, "utf8").trim();
  request = input ? JSON.parse(input) : {};
} catch (error) {
  emit({ type: "error", message: "failed to read adapter request: " + error.message });
  process.exit(0);
}

if (request.method === "probe") {
  emit({ type: "complete", item: { ok: true } });
  process.exit(0);
}

const location = request.source && request.source.location ? request.source.location : ".";
const sessionsPath = resolveSessionsPath(location);
let payload;
try {
  payload = JSON.parse(fs.readFileSync(sessionsPath, "utf8"));
} catch (error) {
  emit({ type: "error", message: "failed to read normalized sessions: " + sessionsPath + ": " + error.message });
  process.exit(0);
}

const sessions = Array.isArray(payload.sessions) ? payload.sessions.map(normalizeSessionCards) : [];
for (const session of sessions) {
  emit({ type: "item", item: { kind: "session", session } });
}
emit({ type: "complete", item: { session_count: sessions.length } });

function normalizeSessionCards(session) {
  if (!session || typeof session !== "object") return session;
  let changed = false;
  const turns = Array.isArray(session.turns) ? session.turns : [];
  for (const turn of turns) {
    const parts = Array.isArray(turn && turn.parts) ? turn.parts : [];
    for (const part of parts) {
      if (ensurePartContentCard(part)) {
        changed = true;
      }
    }
  }
  if (changed && typeof session.source_fingerprint === "string" && session.source_fingerprint.trim()) {
    if (!session.source_fingerprint.includes(CONTENT_CARD_SCHEMA)) {
      session.source_fingerprint = session.source_fingerprint + ":" + CONTENT_CARD_SCHEMA;
    }
  }
  return session;
}

function ensurePartContentCard(part) {
  if (!part || typeof part !== "object") return false;
  const metadata = metadataObject(part.metadata_json);
  const existing = metadata.content_card || metadata.contentCard;
  if (existing && typeof existing === "object" && typeof existing.type === "string") {
    return false;
  }
  const contentCard = inferContentCard(part);
  if (!contentCard) return false;
  part.metadata_json = JSON.stringify({ ...metadata, content_card: contentCard });
  return true;
}

function metadataObject(value) {
  if (!value || typeof value !== "string" || !value.trim()) return {};
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function inferContentCard(part) {
  const kind = text(part.kind || "text");
  const role = text(part.role || "assistant");
  const language = text(part.language);
  if (kind === "code_block") {
    return compactObject({ type: "code", language });
  }
  if (kind === "command") {
    return { type: "command" };
  }
  if (kind === "tool" || kind === "file_change" || kind === "subagent") {
    return { type: "result", format: "markdown" };
  }
  if (kind === "metadata") {
    return { type: "tool", format: "markdown" };
  }
  if (role === "tool") {
    return { type: "result", format: "markdown" };
  }
  if (role === "assistant") {
    return { type: "answer", format: "markdown" };
  }
  return null;
}

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== "")
  );
}

function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

function resolveSessionsPath(location) {
  const candidates = [
    path.join(location, "sessions.json"),
    path.join(location, "normalized", "sessions.json"),
    path.join(location, "output", "normalized", "sessions.json"),
  ];
  if (/[\\/]normalized$/i.test(location) && !/[\\/]output[\\/]normalized$/i.test(location)) {
    candidates.push(path.join(path.dirname(location), "output", "normalized", "sessions.json"));
  }
  if (/[\\/]output[\\/]normalized$/i.test(location)) {
    candidates.push(path.join(path.dirname(path.dirname(location)), "normalized", "sessions.json"));
  }
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return candidates[0];
}
`
