package webharvester

import (
	"crypto/sha256"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/util6/assetiweave/errs"
)

func TestScaffoldCreatesHarvesterAndAdapterFiles(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")

	result, err := Scaffold(ScaffoldOptions{
		Directory: root,
		SiteID:    "qwen-web",
		Name:      "Qwen Web",
	})

	if err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}
	for _, path := range []string{
		result.ConfigPath,
		result.ManifestPath,
		result.AdapterPath,
		filepath.Join(root, "requests", "auth-probe.json"),
		filepath.Join(root, "requests", "list.json"),
		filepath.Join(root, "requests", "detail.json"),
		filepath.Join(root, "normalized"),
		filepath.Join(root, "raw"),
	} {
		if _, err := os.Stat(path); err != nil {
			t.Fatalf("expected scaffold path %s: %v", path, err)
		}
	}
	config, err := LoadConfig(root)
	if err != nil {
		t.Fatalf("LoadConfig() error = %v", err)
	}
	if config.SiteID != "qwen-web" || config.GeneratedAdapter.Script != "adapter.js" {
		t.Fatalf("unexpected config = %#v", config)
	}
}

func TestAuthCheckReportsExpiredLogin(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = w.Write([]byte(`{"error":"login required"}`))
	}))
	t.Cleanup(server.Close)
	writeRequest(t, filepath.Join(root, "requests", "auth-probe.json"), HTTPRequestTemplate{
		Method: "GET",
		URL:    server.URL + "/me",
	})

	_, err := AuthCheck(AuthCheckOptions{Directory: root})

	if err == nil {
		t.Fatal("AuthCheck() error = nil, want expired auth error")
	}
	problem, ok := errs.ProblemOf(err)
	if !ok || problem.Code != "AUTH_EXPIRED" {
		t.Fatalf("AuthCheck() error = %#v, want AUTH_EXPIRED", err)
	}
}

func TestSyncDownloadsAndNormalizesSessions(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web", Name: "Qwen Web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		switch r.URL.Path {
		case "/me":
			_, _ = w.Write([]byte(`{"data":{"user":"ok"}}`))
		case "/sessions":
			_, _ = w.Write([]byte(`{"data":{"sessions":[{"id":"s1","title":"First web chat","updated_at":"2026-06-09T01:00:00Z"}]}}`))
		case "/sessions/s1":
			_, _ = w.Write([]byte(`{"data":{"messages":[{"id":"m1","role":"user","content":"hello","created_at":"2026-06-09T00:59:00Z"},{"id":"m2","role":"assistant","content":"answer"}]}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(server.Close)
	writeRequest(t, filepath.Join(root, "requests", "auth-probe.json"), HTTPRequestTemplate{Method: "GET", URL: server.URL + "/me"})
	writeRequest(t, filepath.Join(root, "requests", "list.json"), HTTPRequestTemplate{Method: "GET", URL: server.URL + "/sessions"})
	writeRequest(t, filepath.Join(root, "requests", "detail.json"), HTTPRequestTemplate{Method: "GET", URL: server.URL + "/sessions/{session_id}"})

	result, err := Sync(SyncOptions{
		Directory: root,
		Now:       func() time.Time { return time.Date(2026, 6, 9, 1, 2, 3, 0, time.UTC) },
	})

	if err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	if result.SessionCount != 1 || result.TurnCount != 1 {
		t.Fatalf("Sync() counts = sessions %d turns %d", result.SessionCount, result.TurnCount)
	}
	if _, err := os.Stat(filepath.Join(result.RawRunDir, "list.json")); err != nil {
		t.Fatalf("missing raw list snapshot: %v", err)
	}
	var sessionsFile SessionsFile
	data, err := os.ReadFile(result.NormalizedFile)
	if err != nil {
		t.Fatalf("read normalized file: %v", err)
	}
	if err := json.Unmarshal(data, &sessionsFile); err != nil {
		t.Fatalf("normalized file is not JSON: %v\n%s", err, string(data))
	}
	session := sessionsFile.Sessions[0]
	if session.ExternalID != "s1" || *session.Title != "First web chat" {
		t.Fatalf("normalized session = %#v", session)
	}
	if session.Turns[0].UserText != "hello" ||
		len(session.Turns[0].Parts) != 1 ||
		*sessionsFile.Sessions[0].Turns[0].Parts[0].Text != "answer" {
		t.Fatalf("normalized turns = %#v", session.Turns)
	}
}

func TestSyncStopsBeforeListWhenAuthExpired(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}
	listHits := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/me":
			w.WriteHeader(http.StatusUnauthorized)
			_, _ = w.Write([]byte(`{"error":"login required"}`))
		case "/sessions":
			listHits++
			_, _ = w.Write([]byte(`{"data":{"sessions":[]}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(server.Close)
	writeRequest(t, filepath.Join(root, "requests", "auth-probe.json"), HTTPRequestTemplate{Method: "GET", URL: server.URL + "/me"})
	writeRequest(t, filepath.Join(root, "requests", "list.json"), HTTPRequestTemplate{Method: "GET", URL: server.URL + "/sessions"})

	_, err := Sync(SyncOptions{Directory: root})

	if err == nil {
		t.Fatal("Sync() error = nil, want auth error")
	}
	problem, ok := errs.ProblemOf(err)
	if !ok || problem.Code != "AUTH_EXPIRED" {
		t.Fatalf("Sync() error = %#v, want AUTH_EXPIRED", err)
	}
	if listHits != 0 {
		t.Fatalf("list endpoint was called %d times, want 0", listHits)
	}
}

func TestSyncStoresNonJSONListSnapshotBeforeParseError(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/me":
			_, _ = w.Write([]byte(`{"data":{"user":"ok"}}`))
		case "/sessions":
			w.Header().Set("Content-Type", "text/html")
			_, _ = w.Write([]byte(`<html>login page</html>`))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(server.Close)
	writeRequest(t, filepath.Join(root, "requests", "auth-probe.json"), HTTPRequestTemplate{Method: "GET", URL: server.URL + "/me"})
	writeRequest(t, filepath.Join(root, "requests", "list.json"), HTTPRequestTemplate{Method: "GET", URL: server.URL + "/sessions"})

	_, err := Sync(SyncOptions{
		Directory: root,
		Now:       func() time.Time { return time.Date(2026, 6, 9, 1, 2, 3, 0, time.UTC) },
	})

	if err == nil {
		t.Fatal("Sync() error = nil, want validation error")
	}
	rawPath := filepath.Join(root, "raw", "20260609T010203Z", "list.json")
	data, readErr := os.ReadFile(rawPath)
	if readErr != nil {
		t.Fatalf("read raw snapshot: %v", readErr)
	}
	var snapshot HTTPResponseSnapshot
	if err := json.Unmarshal(data, &snapshot); err != nil {
		t.Fatalf("raw snapshot is not JSON: %v\n%s", err, string(data))
	}
	if snapshot.Body != `<html>login page</html>` {
		t.Fatalf("raw snapshot body = %q, want html response", snapshot.Body)
	}
}

func TestNormalizeTurnsSupportsQwenRoundShape(t *testing.T) {
	messages := []any{
		map[string]any{
			"req_id":      "req-1",
			"create_time": "2026-06-09T01:00:00Z",
			"update_time": "2026-06-09T01:00:03Z",
			"request_messages": []any{
				map[string]any{"mime_type": "text/plain", "content": "hello"},
			},
			"response_messages": []any{
				map[string]any{"mime_type": "signal/post"},
				map[string]any{"mime_type": "multi_load/iframe", "content": "answer"},
			},
		},
	}

	turns := normalizeTurns(defaultConfig("qwen-web", "Qwen Web"), messages)

	if len(turns) != 1 {
		t.Fatalf("turn count = %d", len(turns))
	}
	if turns[0].ExternalID != "req-1" || turns[0].UserText != "hello" {
		t.Fatalf("turn = %#v", turns[0])
	}
	if len(turns[0].Parts) != 1 || *turns[0].Parts[0].Text != "answer" {
		t.Fatalf("parts = %#v", turns[0].Parts)
	}
}

func TestAuthDetectWritesAuthProbeFromBrowserCookies(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}

	result, err := AuthDetect(AuthDetectOptions{
		Directory: root,
		Browser:   "edge",
		Profile:   "Default",
		Domain:    "qianwen.com",
		tokenLoader: func(profile BrowserProfile, origins []string) ([]BrowserToken, error) {
			return nil, nil
		},
		cookieLoader: func(profile BrowserProfile, domain string) ([]BrowserCookie, error) {
			if profile.Browser != "edge" || domain != "qianwen.com" {
				t.Fatalf("cookie loader profile=%#v domain=%s", profile, domain)
			}
			return []BrowserCookie{
				{Host: ".qianwen.com", Name: "tongyi_sso_ticket", Value: "secret-ticket"},
				{Host: "www.qianwen.com", Name: "XSRF-TOKEN", Value: "secret-xsrf"},
			}, nil
		},
	})

	if err != nil {
		t.Fatalf("AuthDetect() error = %v", err)
	}
	if result.CookieCount != 2 {
		t.Fatalf("CookieCount = %d, want 2", result.CookieCount)
	}
	encoded, err := json.Marshal(result)
	if err != nil {
		t.Fatalf("marshal result: %v", err)
	}
	if strings.Contains(string(encoded), "secret-ticket") || strings.Contains(string(encoded), "secret-xsrf") {
		t.Fatalf("AuthDetect result leaked cookie values: %s", string(encoded))
	}
	template, err := readRequestTemplate(filepath.Join(root, "requests", "auth-probe.json"), nil)
	if err != nil {
		t.Fatalf("read auth probe template: %v", err)
	}
	if template.URL != "https://api.qianwen.com/growth/user/benefit/user/member/info" {
		t.Fatalf("template URL = %s", template.URL)
	}
	if template.Method != http.MethodPost {
		t.Fatalf("template method = %s", template.Method)
	}
	var body map[string]string
	if err := json.Unmarshal(template.Body, &body); err != nil {
		t.Fatalf("template body is not JSON: %v", err)
	}
	if body["clientChannel"] != "PC" {
		t.Fatalf("template body = %s", string(template.Body))
	}
	if !strings.Contains(template.Headers["Cookie"], "tongyi_sso_ticket=secret-ticket") ||
		!strings.Contains(template.Headers["Cookie"], "XSRF-TOKEN=secret-xsrf") {
		t.Fatalf("template cookie header = %q", template.Headers["Cookie"])
	}
}

func TestAuthDetectPrefersLocalStorageToken(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}
	cookieLoaderCalled := false

	result, err := AuthDetect(AuthDetectOptions{
		Directory:  root,
		Browser:    "edge",
		Profile:    "Default",
		Domain:     "qianwen.com",
		ProbeURL:   "https://chat.qwen.ai/api/v1/auths/",
		Credential: "token",
		tokenLoader: func(profile BrowserProfile, origins []string) ([]BrowserToken, error) {
			if !slicesContain(origins, "https://chat.qwen.ai") {
				t.Fatalf("origins = %#v, want chat.qwen.ai", origins)
			}
			return []BrowserToken{{Origin: "https://chat.qwen.ai", Key: "token", Value: "secret-local-storage-token"}}, nil
		},
		cookieLoader: func(profile BrowserProfile, domain string) ([]BrowserCookie, error) {
			cookieLoaderCalled = true
			return []BrowserCookie{{Host: ".qianwen.com", Name: "cookie", Value: "secret-cookie"}}, nil
		},
	})

	if err != nil {
		t.Fatalf("AuthDetect() error = %v", err)
	}
	if result.Credential != "local_storage_token" {
		t.Fatalf("Credential = %s, want local_storage_token", result.Credential)
	}
	if result.AuthOrigin != "https://chat.qwen.ai" || result.AuthKey != "token" {
		t.Fatalf("auth source = %s %s", result.AuthOrigin, result.AuthKey)
	}
	if cookieLoaderCalled {
		t.Fatal("cookie loader was called even though localStorage token was available")
	}
	encoded, err := json.Marshal(result)
	if err != nil {
		t.Fatalf("marshal result: %v", err)
	}
	if strings.Contains(string(encoded), "secret-local-storage-token") {
		t.Fatalf("AuthDetect result leaked token value: %s", string(encoded))
	}
	template, err := readRequestTemplate(filepath.Join(root, "requests", "auth-probe.json"), nil)
	if err != nil {
		t.Fatalf("read auth probe template: %v", err)
	}
	if template.Headers["Authorization"] != "Bearer secret-local-storage-token" {
		t.Fatalf("Authorization header = %q", template.Headers["Authorization"])
	}
	if template.URL != "https://chat.qwen.ai/api/v1/auths/" {
		t.Fatalf("template URL = %s", template.URL)
	}
}

func TestAuthDetectReportsMissingBrowserLogin(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}

	_, err := AuthDetect(AuthDetectOptions{
		Directory: root,
		Browser:   "chrome",
		Domain:    "qianwen.com",
		tokenLoader: func(profile BrowserProfile, origins []string) ([]BrowserToken, error) {
			return nil, nil
		},
		cookieLoader: func(profile BrowserProfile, domain string) ([]BrowserCookie, error) {
			return nil, nil
		},
	})

	if err == nil {
		t.Fatal("AuthDetect() error = nil, want missing browser auth")
	}
	problem, ok := errs.ProblemOf(err)
	if !ok || problem.Code != "BROWSER_AUTH_NOT_FOUND" {
		t.Fatalf("AuthDetect() error = %#v, want BROWSER_AUTH_NOT_FOUND", err)
	}
}

func TestAuthDetectPreservesCookieDecryptFailure(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}

	_, err := AuthDetect(AuthDetectOptions{
		Directory: root,
		Browser:   "edge",
		Domain:    "qianwen.com",
		tokenLoader: func(profile BrowserProfile, origins []string) ([]BrowserToken, error) {
			return nil, nil
		},
		cookieLoader: func(profile BrowserProfile, domain string) ([]BrowserCookie, error) {
			return nil, authError("BROWSER_COOKIE_DECRYPT_FAILED", "decrypt failed", "grant Keychain access", nil)
		},
	})

	if err == nil {
		t.Fatal("AuthDetect() error = nil, want decrypt failure")
	}
	problem, ok := errs.ProblemOf(err)
	if !ok || problem.Code != "BROWSER_COOKIE_DECRYPT_FAILED" {
		t.Fatalf("AuthDetect() error = %#v, want BROWSER_COOKIE_DECRYPT_FAILED", err)
	}
}

func TestAuthDetectFiltersInvalidCookieHeaderValues(t *testing.T) {
	root := filepath.Join(t.TempDir(), "qwen-web")
	if _, err := Scaffold(ScaffoldOptions{Directory: root, SiteID: "qwen-web"}); err != nil {
		t.Fatalf("Scaffold() error = %v", err)
	}

	result, err := AuthDetect(AuthDetectOptions{
		Directory: root,
		Browser:   "edge",
		Domain:    "qianwen.com",
		tokenLoader: func(profile BrowserProfile, origins []string) ([]BrowserToken, error) {
			return nil, nil
		},
		cookieLoader: func(profile BrowserProfile, domain string) ([]BrowserCookie, error) {
			return []BrowserCookie{
				{Host: ".qianwen.com", Name: "ok", Value: "value"},
				{Host: ".qianwen.com", Name: "bad", Value: "line\nbreak"},
			}, nil
		},
	})

	if err != nil {
		t.Fatalf("AuthDetect() error = %v", err)
	}
	if result.CookieCount != 1 {
		t.Fatalf("CookieCount = %d, want 1", result.CookieCount)
	}
	template, err := readRequestTemplate(filepath.Join(root, "requests", "auth-probe.json"), nil)
	if err != nil {
		t.Fatalf("read auth probe template: %v", err)
	}
	if template.Headers["Cookie"] != "ok=value" {
		t.Fatalf("cookie header = %q", template.Headers["Cookie"])
	}
}

func TestRemoveChromiumHostKeyPrefix(t *testing.T) {
	hash := sha256.Sum256([]byte(".qianwen.com"))
	plaintext := append(hash[:], []byte("cookie-value")...)

	trimmed := removeChromiumHostKeyPrefix(plaintext, ".qianwen.com")

	if string(trimmed) != "cookie-value" {
		t.Fatalf("trimmed = %q", string(trimmed))
	}
}

func slicesContain(values []string, needle string) bool {
	for _, value := range values {
		if value == needle {
			return true
		}
	}
	return false
}

func writeRequest(t *testing.T, path string, request HTTPRequestTemplate) {
	t.Helper()
	data, err := json.MarshalIndent(request, "", "  ")
	if err != nil {
		t.Fatalf("encode request: %v", err)
	}
	if err := os.WriteFile(path, append(data, '\n'), 0o600); err != nil {
		t.Fatalf("write request %s: %v", path, err)
	}
}
