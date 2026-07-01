package cmd

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/schema"
	"github.com/util6/assetiweave/internal/webharvester"
)

func newCmdConversation(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "conversation", Short: "Manage normalized conversation records"}
	cmd.AddCommand(newCmdConversationAdapter(f))
	cmd.AddCommand(newCmdConversationSource(f))
	cmd.AddCommand(newCmdConversationScript(f))
	cmd.AddCommand(newCmdConversationSync(f))
	cmd.AddCommand(newCmdConversationSearch(f))
	cmd.AddCommand(newCmdConversationSession(f))
	cmd.AddCommand(newCmdConversationWebRecord(f))
	cmd.AddCommand(newCmdConversationQuestion(f))
	cmd.AddCommand(newCmdConversationPart(f))
	cmd.AddCommand(newCmdConversationWeb(f))
	return cmd
}

func newCmdConversationSearch(f *cmdutil.Factory) *cobra.Command {
	var recordKind, adapterID, sourceID, projectPath, query, since, until, format string
	var contentTypes, cardTypes []string
	var currentProject, timeline bool
	var limit, offset int
	cmd := &cobra.Command{
		Use:   "search",
		Short: "Search conversation cards for AI memory retrieval",
		RunE: func(cmd *cobra.Command, args []string) error {
			resolvedProjectPath := projectPath
			if currentProject {
				wd, err := os.Getwd()
				if err != nil {
					return err
				}
				resolvedProjectPath = wd
			}
			resolvedContentTypes := append([]string{}, contentTypes...)
			resolvedContentTypes = append(resolvedContentTypes, cardTypes...)
			params := map[string]any{
				"record_kind":   recordKind,
				"adapter_id":    nil,
				"source_id":     nil,
				"project_path":  nil,
				"query":         query,
				"content_types": resolvedContentTypes,
				"since":         nil,
				"until":         nil,
				"timeline":      timeline,
				"limit":         limit,
				"offset":        offset,
			}
			if adapterID != "" {
				params["adapter_id"] = adapterID
			}
			if sourceID != "" {
				params["source_id"] = sourceID
			}
			if resolvedProjectPath != "" {
				params["project_path"] = resolvedProjectPath
			}
			if since != "" {
				params["since"] = since
			}
			if until != "" {
				params["until"] = until
			}
			switch format {
			case "json":
				return callAndPrint(cmd, f, schema.MethodConversationSearch, params)
			case "compact-json", "markdown", "prompt":
				result, err := callEngine(cmd, f, schema.MethodConversationSearch, params)
				if err != nil {
					return err
				}
				parsed, err := decodeConversationSearchResult(result.Data)
				if err != nil {
					return err
				}
				parsed.ensureScope(params)
				switch format {
				case "compact-json":
					if result.Meta == nil {
						output.WriteSuccess(f.IOStreams.Out, parsed.compact())
					} else {
						output.WriteSuccessWithMeta(f.IOStreams.Out, parsed.compact(), result.Meta)
					}
				case "markdown":
					_, _ = fmt.Fprint(f.IOStreams.Out, parsed.markdown())
				case "prompt":
					_, _ = fmt.Fprint(f.IOStreams.Out, parsed.prompt())
				}
				return nil
			default:
				return fmt.Errorf("unsupported conversation search format %q: use json, compact-json, markdown, or prompt", format)
			}
		},
	}
	cmd.Flags().StringVar(&query, "query", "", "search query")
	cmd.Flags().StringVar(&recordKind, "record-kind", "session", "conversation record kind: session or web")
	cmd.Flags().StringVar(&adapterID, "adapter", "", "adapter id filter")
	cmd.Flags().StringVar(&sourceID, "source", "", "source id filter")
	cmd.Flags().StringVar(&projectPath, "project", "", "project path filter")
	cmd.Flags().BoolVar(&currentProject, "current-project", false, "use the current working directory as the project path filter")
	cmd.Flags().StringArrayVar(&contentTypes, "type", []string{}, "content card type filter; repeat for question, answer, tool, command, code, or result")
	cmd.Flags().StringArrayVar(&cardTypes, "card-type", []string{}, "alias of --type")
	cmd.Flags().StringVar(&since, "since", "", "only include sessions on or after this RFC3339 timestamp or YYYY-MM-DD date")
	cmd.Flags().StringVar(&until, "until", "", "only include sessions on or before this RFC3339 timestamp or YYYY-MM-DD date")
	cmd.Flags().BoolVar(&timeline, "timeline", false, "return hits in chronological session order")
	cmd.Flags().IntVar(&limit, "limit", 50, "maximum hits")
	cmd.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	cmd.Flags().StringVar(&format, "format", "json", "output format: json, compact-json, markdown, or prompt")
	_ = cmd.MarkFlagRequired("query")
	return cmd
}

type conversationSearchScope struct {
	RecordKind   string   `json:"record_kind"`
	AdapterID    *string  `json:"adapter_id"`
	SourceID     *string  `json:"source_id"`
	ProjectPath  *string  `json:"project_path"`
	Query        string   `json:"query"`
	ContentTypes []string `json:"content_types"`
	Since        *string  `json:"since"`
	Until        *string  `json:"until"`
	Timeline     bool     `json:"timeline"`
	Limit        int      `json:"limit"`
	Offset       int      `json:"offset"`
}

type conversationSearchResult struct {
	Query      string                   `json:"query"`
	RecordKind string                   `json:"record_kind"`
	Scope      *conversationSearchScope `json:"scope,omitempty"`
	TotalCount int                      `json:"total_count"`
	Hits       []conversationSearchHit  `json:"hits"`
}

type conversationSearchHit struct {
	Session       conversationSearchSessionItem `json:"session"`
	QuestionID    string                        `json:"question_id"`
	QuestionIndex int                           `json:"question_index"`
	QuestionTitle string                        `json:"question_title"`
	TurnID        *string                       `json:"turn_id"`
	PartID        *string                       `json:"part_id"`
	BlockID       string                        `json:"block_id"`
	CardType      string                        `json:"card_type"`
	Snippet       string                        `json:"snippet"`
	Score         int                           `json:"score"`
}

type conversationSearchSessionItem struct {
	conversationSearchSession
	QuestionCount int `json:"question_count"`
	TurnCount     int `json:"turn_count"`
}

type conversationSearchSession struct {
	ID          string  `json:"id"`
	SourceID    string  `json:"source_id"`
	AdapterID   string  `json:"adapter_id"`
	ExternalID  string  `json:"external_id"`
	Title       string  `json:"title"`
	ProjectPath *string `json:"project_path"`
	StartedAt   *string `json:"started_at"`
	UpdatedAt   *string `json:"updated_at"`
	ImportedAt  string  `json:"imported_at"`
}

type compactConversationSearchResult struct {
	Query      string                         `json:"query"`
	RecordKind string                         `json:"record_kind"`
	Scope      conversationSearchScope        `json:"scope"`
	TotalCount int                            `json:"total_count"`
	Hits       []compactConversationSearchHit `json:"hits"`
}

type compactConversationSearchHit struct {
	EventTime     string  `json:"event_time,omitempty"`
	SessionID     string  `json:"session_id"`
	SessionTitle  string  `json:"session_title"`
	ProjectPath   *string `json:"project_path"`
	QuestionID    string  `json:"question_id"`
	QuestionIndex int     `json:"question_index"`
	QuestionTitle string  `json:"question_title"`
	TurnID        *string `json:"turn_id"`
	PartID        *string `json:"part_id"`
	BlockID       string  `json:"block_id"`
	CardType      string  `json:"card_type"`
	Snippet       string  `json:"snippet"`
	Score         int     `json:"score"`
}

func decodeConversationSearchResult(data json.RawMessage) (*conversationSearchResult, error) {
	var result conversationSearchResult
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, fmt.Errorf("decode conversation search result: %w", err)
	}
	return &result, nil
}

func (r *conversationSearchResult) ensureScope(params map[string]any) {
	if r.Scope != nil {
		return
	}
	r.Scope = &conversationSearchScope{
		RecordKind:   stringParam(params, "record_kind"),
		AdapterID:    optionalStringParam(params, "adapter_id"),
		SourceID:     optionalStringParam(params, "source_id"),
		ProjectPath:  optionalStringParam(params, "project_path"),
		Query:        stringParam(params, "query"),
		ContentTypes: stringSliceParam(params, "content_types"),
		Since:        optionalStringParam(params, "since"),
		Until:        optionalStringParam(params, "until"),
		Timeline:     boolParam(params, "timeline"),
		Limit:        intParam(params, "limit"),
		Offset:       intParam(params, "offset"),
	}
}

func (r conversationSearchResult) compact() compactConversationSearchResult {
	scope := conversationSearchScope{}
	if r.Scope != nil {
		scope = *r.Scope
	}
	hits := make([]compactConversationSearchHit, 0, len(r.Hits))
	for _, hit := range r.Hits {
		hits = append(hits, compactConversationSearchHit{
			EventTime:     hit.eventTime(),
			SessionID:     hit.Session.ID,
			SessionTitle:  hit.Session.Title,
			ProjectPath:   hit.Session.ProjectPath,
			QuestionID:    hit.QuestionID,
			QuestionIndex: hit.QuestionIndex,
			QuestionTitle: hit.QuestionTitle,
			TurnID:        hit.TurnID,
			PartID:        hit.PartID,
			BlockID:       hit.BlockID,
			CardType:      hit.CardType,
			Snippet:       hit.Snippet,
			Score:         hit.Score,
		})
	}
	return compactConversationSearchResult{
		Query:      r.Query,
		RecordKind: r.RecordKind,
		Scope:      scope,
		TotalCount: r.TotalCount,
		Hits:       hits,
	}
}

func (r conversationSearchResult) markdown() string {
	var b strings.Builder
	b.WriteString("# Conversation Search Evidence\n\n")
	b.WriteString("## Search Scope\n")
	if r.Scope != nil {
		writeMarkdownKV(&b, "Query", r.Scope.Query)
		writeMarkdownKV(&b, "Record kind", r.Scope.RecordKind)
		writeMarkdownOptionalKV(&b, "Adapter", r.Scope.AdapterID)
		writeMarkdownOptionalKV(&b, "Source", r.Scope.SourceID)
		writeMarkdownOptionalKV(&b, "Project", r.Scope.ProjectPath)
		if len(r.Scope.ContentTypes) == 0 {
			writeMarkdownKV(&b, "Card types", "all")
		} else {
			writeMarkdownKV(&b, "Card types", strings.Join(r.Scope.ContentTypes, ", "))
		}
		writeMarkdownOptionalKV(&b, "Since", r.Scope.Since)
		writeMarkdownOptionalKV(&b, "Until", r.Scope.Until)
		if r.Scope.Timeline {
			writeMarkdownKV(&b, "Ordering", "timeline, oldest session first")
		} else {
			writeMarkdownKV(&b, "Ordering", "default, newest updated session first")
		}
		writeMarkdownKV(&b, "Returned", fmt.Sprintf("%d of %d", len(r.Hits), r.TotalCount))
	} else {
		writeMarkdownKV(&b, "Query", r.Query)
		writeMarkdownKV(&b, "Record kind", r.RecordKind)
		writeMarkdownKV(&b, "Returned", fmt.Sprintf("%d of %d", len(r.Hits), r.TotalCount))
	}
	b.WriteString("\n## Hits\n")
	if len(r.Hits) == 0 {
		b.WriteString("No matching cards were returned.\n")
		return b.String()
	}
	for index, hit := range r.Hits {
		b.WriteString(fmt.Sprintf("\n### %d. %s - %s\n", index+1, hit.CardType, hit.Session.Title))
		writeMarkdownKV(&b, "Time", hit.eventTime())
		writeMarkdownKV(&b, "Session", hit.Session.ID)
		writeMarkdownOptionalKV(&b, "Project", hit.Session.ProjectPath)
		writeMarkdownKV(&b, "Question", fmt.Sprintf("#%d %s", hit.QuestionIndex, hit.QuestionTitle))
		writeMarkdownKV(&b, "Question ID", hit.QuestionID)
		writeMarkdownOptionalKV(&b, "Turn ID", hit.TurnID)
		writeMarkdownOptionalKV(&b, "Part ID", hit.PartID)
		writeMarkdownKV(&b, "Block ID", hit.BlockID)
		writeMarkdownKV(&b, "Score", fmt.Sprintf("%d", hit.Score))
		b.WriteString("\n")
		b.WriteString(blockquote(hit.Snippet))
		b.WriteString("\n")
	}
	return b.String()
}

func (r conversationSearchResult) prompt() string {
	return "# Prompt\n\n" +
		"Use only the search evidence below to answer the user's question. " +
		"Infer topics, preferences, and constraints yourself; do not assume that the CLI has already clustered them. " +
		"When citing evidence, reference session_id, question_id, and block_id. " +
		"If the evidence is insufficient, state the gap instead of inventing context.\n\n" +
		r.markdown()
}

func (h conversationSearchHit) eventTime() string {
	if h.Session.StartedAt != nil && *h.Session.StartedAt != "" {
		return *h.Session.StartedAt
	}
	if h.Session.UpdatedAt != nil && *h.Session.UpdatedAt != "" {
		return *h.Session.UpdatedAt
	}
	return h.Session.ImportedAt
}

func writeMarkdownKV(b *strings.Builder, key, value string) {
	if strings.TrimSpace(value) == "" {
		value = "all"
	}
	b.WriteString(fmt.Sprintf("- %s: `%s`\n", key, value))
}

func writeMarkdownOptionalKV(b *strings.Builder, key string, value *string) {
	if value == nil || strings.TrimSpace(*value) == "" {
		writeMarkdownKV(b, key, "all")
		return
	}
	writeMarkdownKV(b, key, *value)
}

func blockquote(value string) string {
	lines := strings.Split(strings.TrimSpace(value), "\n")
	for i, line := range lines {
		lines[i] = "> " + line
	}
	return strings.Join(lines, "\n") + "\n"
}

func stringParam(params map[string]any, key string) string {
	value, _ := params[key].(string)
	return value
}

func optionalStringParam(params map[string]any, key string) *string {
	value, ok := params[key].(string)
	if !ok || value == "" {
		return nil
	}
	return &value
}

func stringSliceParam(params map[string]any, key string) []string {
	values, ok := params[key].([]string)
	if !ok {
		return []string{}
	}
	return values
}

func boolParam(params map[string]any, key string) bool {
	value, _ := params[key].(bool)
	return value
}

func intParam(params map[string]any, key string) int {
	value, _ := params[key].(int)
	return value
}

func newCmdConversationAdapter(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "adapter", Short: "Manage conversation adapters"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List conversation adapters",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationAdapterList, map[string]any{})
		},
	})
	cmd.AddCommand(newCmdConversationAdapterScaffold(f))
	cmd.AddCommand(newCmdConversationAdapterValidate(f))
	cmd.AddCommand(newCmdConversationAdapterRuntimeStatus(f))
	cmd.AddCommand(newCmdConversationAdapterRegister(f))
	cmd.AddCommand(newCmdConversationAdapterUnregister(f))
	cmd.AddCommand(newCmdConversationAdapterTryRun(f))
	return cmd
}

func newCmdConversationAdapterScaffold(f *cmdutil.Factory) *cobra.Command {
	var directory, id, name, runtimeType, runtimeEntry, runtimeVersion string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "scaffold",
		Short: "Create a system-runtime adapter manifest scaffold",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{
				"directory": directory,
				"id":        id,
				"name":      name,
				"dry_run":   dryRun,
			}
			if runtimeType != "" {
				params["runtime_type"] = runtimeType
			}
			if runtimeEntry != "" {
				params["runtime_entry"] = runtimeEntry
			}
			if runtimeVersion != "" {
				params["runtime_version"] = runtimeVersion
			}
			return callAndPrint(cmd, f, schema.MethodConversationAdapterScaffold, params)
		},
	}
	cmd.Flags().StringVar(&directory, "directory", "", "directory where scaffold files will be created")
	cmd.Flags().StringVar(&id, "id", "", "adapter id")
	cmd.Flags().StringVar(&name, "name", "", "adapter display name")
	cmd.Flags().StringVar(&runtimeType, "runtime", "", "adapter runtime: node, python, bash, or executable")
	cmd.Flags().StringVar(&runtimeType, "runtime-type", "", "alias of --runtime")
	cmd.Flags().StringVar(&runtimeEntry, "runtime-entry", "", "adapter entry path relative to the adapter directory")
	cmd.Flags().StringVar(&runtimeVersion, "runtime-version", "", "runtime version requirement such as >=20 or >=3.10")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without writing files")
	_ = cmd.MarkFlagRequired("directory")
	_ = cmd.MarkFlagRequired("id")
	_ = cmd.MarkFlagRequired("name")
	return cmd
}

func newCmdConversationAdapterValidate(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "validate <manifest>",
		Short: "Validate an adapter manifest",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationAdapterValidate, map[string]any{"manifest_path": args[0]})
		},
	}
}

func newCmdConversationAdapterRuntimeStatus(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "runtime-status",
		Short: "List detected system runtimes for conversation adapters",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationAdapterRuntimeStatus, map[string]any{})
		},
	}
}

func newCmdConversationAdapterRegister(f *cmdutil.Factory) *cobra.Command {
	var dryRun, yes bool
	cmd := &cobra.Command{
		Use:   "register <manifest>",
		Short: "Register a trusted adapter script",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !dryRun {
				if err := requireYes(yes, "conversation adapter register"); err != nil {
					return err
				}
			}
			return callAndPrint(cmd, f, schema.MethodConversationAdapterRegister, map[string]any{
				"manifest_path": args[0],
				"dry_run":       dryRun,
				"yes":           yes,
			})
		},
	}
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without registering")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm trusting this adapter")
	return cmd
}

func newCmdConversationAdapterUnregister(f *cmdutil.Factory) *cobra.Command {
	var dryRun, yes bool
	cmd := &cobra.Command{
		Use:   "unregister <adapter-id>",
		Short: "Unregister an external adapter",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !dryRun {
				if err := requireYes(yes, "conversation adapter unregister"); err != nil {
					return err
				}
			}
			return callAndPrint(cmd, f, schema.MethodConversationAdapterUnregister, map[string]any{
				"adapter_id": args[0],
				"dry_run":    dryRun,
				"yes":        yes,
			})
		},
	}
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without unregistering")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm unregistering this adapter")
	return cmd
}

func newCmdConversationAdapterTryRun(f *cmdutil.Factory) *cobra.Command {
	var method, location, sessionID string
	var yes bool
	cmd := &cobra.Command{
		Use:   "try-run <manifest>",
		Short: "Execute an adapter manifest once and validate its output",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := requireYes(yes, "conversation adapter try-run"); err != nil {
				return err
			}
			params := map[string]any{
				"manifest_path": args[0],
				"method":        method,
				"location":      nil,
				"session_id":    nil,
				"yes":           yes,
			}
			if location != "" {
				params["location"] = location
			}
			if sessionID != "" {
				params["session_id"] = sessionID
			}
			return callAndPrint(cmd, f, schema.MethodConversationAdapterTryRun, params)
		},
	}
	cmd.Flags().StringVar(&method, "method", "read_session", "adapter method: probe, list_sessions, or read_session")
	cmd.Flags().StringVar(&location, "location", "", "source location passed to the adapter")
	cmd.Flags().StringVar(&sessionID, "session-id", "", "external session id for read_session")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm executing this adapter")
	return cmd
}

func newCmdConversationSource(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "source", Short: "Manage conversation sources"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List conversation sources",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationSourceList, map[string]any{})
		},
	})
	cmd.AddCommand(newCmdConversationSourceUpsert(f, false))
	cmd.AddCommand(newCmdConversationSourceUpsert(f, true))
	cmd.AddCommand(newCmdConversationSourceDisable(f))
	return cmd
}

func newCmdConversationSourceUpsert(f *cmdutil.Factory, update bool) *cobra.Command {
	var id, adapterID, name, kind, location, configJSON string
	var enabled, dryRun bool
	use := "add"
	method := schema.MethodConversationSourceAdd
	short := "Add a conversation source"
	if update {
		use = "update"
		method = schema.MethodConversationSourceUpdate
		short = "Update a conversation source"
	}
	cmd := &cobra.Command{
		Use:   use,
		Short: short,
		RunE: func(cmd *cobra.Command, args []string) error {
			now := time.Now().UTC().Format(time.RFC3339)
			var config any
			if configJSON != "" {
				config = configJSON
			}
			return callAndPrint(cmd, f, method, map[string]any{
				"source": map[string]any{
					"id":               id,
					"adapter_id":       adapterID,
					"name":             name,
					"kind":             kind,
					"location":         location,
					"config_json":      config,
					"enabled":          enabled,
					"last_synced_at":   nil,
					"last_sync_status": nil,
					"created_at":       now,
					"updated_at":       now,
				},
				"dry_run": dryRun,
			})
		},
	}
	cmd.Flags().StringVar(&id, "id", "", "source id")
	cmd.Flags().StringVar(&adapterID, "adapter", "", "adapter id")
	cmd.Flags().StringVar(&name, "name", "", "source display name")
	cmd.Flags().StringVar(&kind, "kind", "live", "source kind: live, file, directory, sqlite, custom")
	cmd.Flags().StringVar(&location, "location", "", "source location")
	cmd.Flags().StringVar(&configJSON, "config-json", "", "optional source adapter config JSON string")
	cmd.Flags().BoolVar(&enabled, "enabled", true, "enable source")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without persisting")
	_ = cmd.MarkFlagRequired("id")
	_ = cmd.MarkFlagRequired("adapter")
	_ = cmd.MarkFlagRequired("name")
	_ = cmd.MarkFlagRequired("location")
	return cmd
}

func newCmdConversationSourceDisable(f *cmdutil.Factory) *cobra.Command {
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "disable <source-id>",
		Short: "Disable a conversation source",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationSourceDisable, map[string]any{"id": args[0], "dry_run": dryRun})
		},
	}
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without changing state")
	return cmd
}

func newCmdConversationScript(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "script", Short: "Manage downloadable conversation parser scripts"}
	cmd.AddCommand(newCmdConversationScriptCatalog(f))
	cmd.AddCommand(newCmdConversationScriptInstall(f))
	return cmd
}

func newCmdConversationScriptCatalog(f *cmdutil.Factory) *cobra.Command {
	var catalogURL string
	cmd := &cobra.Command{
		Use:   "catalog",
		Short: "List downloadable conversation parser scripts",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{"catalog_url": nil}
			if catalogURL != "" {
				params["catalog_url"] = catalogURL
			}
			return callAndPrint(cmd, f, schema.MethodConversationScriptCatalog, params)
		},
	}
	cmd.Flags().StringVar(&catalogURL, "catalog-url", "", "catalog JSON URL or local path")
	return cmd
}

func newCmdConversationScriptInstall(f *cmdutil.Factory) *cobra.Command {
	var catalogURL string
	var dryRun, yes bool
	cmd := &cobra.Command{
		Use:   "install <item-id>",
		Short: "Download and register a trusted conversation parser script",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !dryRun {
				if err := requireYes(yes, "conversation script install"); err != nil {
					return err
				}
			}
			params := map[string]any{
				"catalog_url": nil,
				"item_id":     args[0],
				"dry_run":     dryRun,
				"yes":         yes,
			}
			if catalogURL != "" {
				params["catalog_url"] = catalogURL
			}
			return callAndPrint(cmd, f, schema.MethodConversationScriptInstall, params)
		},
	}
	cmd.Flags().StringVar(&catalogURL, "catalog-url", "", "catalog JSON URL or local path")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview install target without downloading")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm downloading and trusting this script")
	return cmd
}

func newCmdConversationSync(f *cmdutil.Factory) *cobra.Command {
	var sourceID, adapterID, recordKind string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "sync",
		Short: "Synchronize conversation sources",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{
				"source_id":   nil,
				"adapter_id":  nil,
				"record_kind": recordKind,
				"dry_run":     dryRun,
			}
			if sourceID != "" {
				params["source_id"] = sourceID
			}
			if adapterID != "" {
				params["adapter_id"] = adapterID
			}
			return callAndPrint(cmd, f, schema.MethodConversationSync, params)
		},
	}
	cmd.Flags().StringVar(&sourceID, "source", "", "source id filter")
	cmd.Flags().StringVar(&adapterID, "adapter", "", "adapter id filter")
	cmd.Flags().StringVar(&recordKind, "record-kind", "session", "record kind: session or web")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without importing")
	return cmd
}

func newCmdConversationSession(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "session", Short: "Browse and export conversation sessions"}
	cmd.AddCommand(newCmdConversationSessionList(f))
	cmd.AddCommand(newCmdConversationSessionGet(f))
	cmd.AddCommand(newCmdConversationSessionExport(f))
	return cmd
}

func newCmdConversationSessionList(f *cmdutil.Factory) *cobra.Command {
	var adapterID, sourceID, query string
	var limit, offset int
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List imported sessions",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := paginationParams(limit, offset)
			params["adapter_id"] = nil
			params["source_id"] = nil
			params["query"] = nil
			if adapterID != "" {
				params["adapter_id"] = adapterID
			}
			if sourceID != "" {
				params["source_id"] = sourceID
			}
			if query != "" {
				params["query"] = query
			}
			return callAndPrint(cmd, f, schema.MethodConversationSessionList, params)
		},
	}
	cmd.Flags().StringVar(&adapterID, "adapter", "", "adapter id filter")
	cmd.Flags().StringVar(&sourceID, "source", "", "source id filter")
	cmd.Flags().StringVar(&query, "query", "", "search query")
	cmd.Flags().IntVar(&limit, "limit", 50, "maximum sessions")
	cmd.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	return cmd
}

func newCmdConversationSessionGet(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "get <session-id>",
		Short: "Get one session with question groups",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationSessionGet, map[string]any{"session_id": args[0]})
		},
	}
}

func newCmdConversationSessionExport(f *cmdutil.Factory) *cobra.Command {
	var outputRoot string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "export <session-id>",
		Short: "Export one session as Markdown",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationSessionExport, map[string]any{
				"session_id":  args[0],
				"output_root": outputRoot,
				"dry_run":     dryRun,
			})
		},
	}
	cmd.Flags().StringVar(&outputRoot, "output-root", "", "directory for exported Markdown")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without writing")
	_ = cmd.MarkFlagRequired("output-root")
	return cmd
}

func newCmdConversationWebRecord(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "web-record", Short: "Browse and export imported web conversations"}
	cmd.AddCommand(newCmdConversationWebRecordList(f))
	cmd.AddCommand(newCmdConversationWebRecordGet(f))
	cmd.AddCommand(newCmdConversationWebRecordExport(f))
	return cmd
}

func newCmdConversationWebRecordList(f *cmdutil.Factory) *cobra.Command {
	var adapterID, sourceID, query string
	var limit, offset int
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List imported web conversations",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := paginationParams(limit, offset)
			params["adapter_id"] = nil
			params["source_id"] = nil
			params["query"] = nil
			if adapterID != "" {
				params["adapter_id"] = adapterID
			}
			if sourceID != "" {
				params["source_id"] = sourceID
			}
			if query != "" {
				params["query"] = query
			}
			return callAndPrint(cmd, f, schema.MethodConversationWebRecordList, params)
		},
	}
	cmd.Flags().StringVar(&adapterID, "adapter", "", "adapter id filter")
	cmd.Flags().StringVar(&sourceID, "source", "", "source id filter")
	cmd.Flags().StringVar(&query, "query", "", "search query")
	cmd.Flags().IntVar(&limit, "limit", 50, "maximum web conversations")
	cmd.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	return cmd
}

func newCmdConversationWebRecordGet(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "get <record-id>",
		Short: "Get one web conversation with question groups",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationWebRecordGet, map[string]any{"session_id": args[0]})
		},
	}
}

func newCmdConversationWebRecordExport(f *cmdutil.Factory) *cobra.Command {
	var outputRoot string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "export <record-id>",
		Short: "Export one web conversation as Markdown",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationWebRecordExport, map[string]any{
				"session_id":  args[0],
				"output_root": outputRoot,
				"dry_run":     dryRun,
			})
		},
	}
	cmd.Flags().StringVar(&outputRoot, "output-root", "", "directory for exported Markdown")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without writing files")
	_ = cmd.MarkFlagRequired("output-root")
	return cmd
}

func newCmdConversationQuestion(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "question", Short: "Manage session question groups"}
	cmd.AddCommand(newCmdConversationQuestionList(f))
	cmd.AddCommand(newCmdConversationQuestionGet(f))
	cmd.AddCommand(newCmdConversationQuestionMerge(f))
	cmd.AddCommand(newCmdConversationQuestionSplit(f))
	return cmd
}

func newCmdConversationWeb(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "web", Short: "Build and run web conversation harvesters"}
	cmd.AddCommand(newCmdConversationWebScaffold(f))
	cmd.AddCommand(newCmdConversationWebAuthDetect(f))
	cmd.AddCommand(newCmdConversationWebAuthCheck(f))
	cmd.AddCommand(newCmdConversationWebSync(f))
	return cmd
}

func newCmdConversationWebScaffold(f *cmdutil.Factory) *cobra.Command {
	var directory, site, name string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "scaffold",
		Short: "Create a lightweight web conversation harvester scaffold",
		RunE: func(cmd *cobra.Command, args []string) error {
			result, err := webharvester.Scaffold(webharvester.ScaffoldOptions{
				Directory: directory,
				SiteID:    site,
				Name:      name,
				DryRun:    dryRun,
			})
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, result)
			return nil
		},
	}
	cmd.Flags().StringVar(&directory, "directory", "", "directory where web harvester files will be created")
	cmd.Flags().StringVar(&site, "site", "", "site id, for example qwen-web")
	cmd.Flags().StringVar(&name, "name", "", "site display name")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without writing files")
	_ = cmd.MarkFlagRequired("directory")
	_ = cmd.MarkFlagRequired("site")
	return cmd
}

func newCmdConversationWebAuthDetect(f *cmdutil.Factory) *cobra.Command {
	var browser, profile, domain, probeURL, credential string
	cmd := &cobra.Command{
		Use:   "auth-detect <directory>",
		Short: "Create an auth probe request from a local browser login state",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			result, err := webharvester.AuthDetect(webharvester.AuthDetectOptions{
				Directory:  args[0],
				Browser:    browser,
				Profile:    profile,
				Domain:     domain,
				ProbeURL:   probeURL,
				Credential: credential,
			})
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, result)
			return nil
		},
	}
	cmd.Flags().StringVar(&browser, "browser", "auto", "browser profile to inspect: auto, chrome, edge, brave, chromium")
	cmd.Flags().StringVar(&profile, "profile", "Default", "browser profile name")
	cmd.Flags().StringVar(&domain, "domain", "qianwen.com", "cookie domain to extract")
	cmd.Flags().StringVar(&probeURL, "probe-url", "", "auth probe URL; defaults to a site-specific probe when known")
	cmd.Flags().StringVar(&credential, "credential", "auto", "credential source: auto, cookie, token")
	return cmd
}

func newCmdConversationWebAuthCheck(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "auth-check <directory>",
		Short: "Run a configured web harvester login-state probe",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			result, err := webharvester.AuthCheck(webharvester.AuthCheckOptions{Directory: args[0]})
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, result)
			return nil
		},
	}
}

func newCmdConversationWebSync(f *cmdutil.Factory) *cobra.Command {
	var limit int
	cmd := &cobra.Command{
		Use:   "sync <directory>",
		Short: "Download and normalize web conversation data from configured request templates",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			result, err := webharvester.Sync(webharvester.SyncOptions{
				Directory: args[0],
				Limit:     limit,
			})
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, result)
			return nil
		},
	}
	cmd.Flags().IntVar(&limit, "limit", 0, "maximum sessions to download; 0 means all sessions in the list response")
	return cmd
}

func newCmdConversationQuestionList(f *cmdutil.Factory) *cobra.Command {
	var query string
	var limit, offset int
	cmd := &cobra.Command{
		Use:   "list <session-id>",
		Short: "List question groups in a session",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			params := paginationParams(limit, offset)
			params["session_id"] = args[0]
			params["query"] = nil
			if query != "" {
				params["query"] = query
			}
			return callAndPrint(cmd, f, schema.MethodConversationQuestionList, params)
		},
	}
	cmd.Flags().StringVar(&query, "query", "", "search query")
	cmd.Flags().IntVar(&limit, "limit", 100, "maximum questions")
	cmd.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	return cmd
}

func newCmdConversationQuestionGet(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "get <question-id>",
		Short: "Get one question group",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationQuestionGet, map[string]any{"question_id": args[0]})
		},
	}
}

func newCmdConversationQuestionMerge(f *cmdutil.Factory) *cobra.Command {
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "merge <question-id>...",
		Short: "Merge adjacent question groups",
		Args:  cobra.MinimumNArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			ids := make([]string, len(args))
			copy(ids, args)
			return callAndPrint(cmd, f, schema.MethodConversationQuestionMerge, map[string]any{
				"question_ids": ids,
				"dry_run":      dryRun,
			})
		},
	}
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without merging")
	return cmd
}

func newCmdConversationQuestionSplit(f *cmdutil.Factory) *cobra.Command {
	var beforeTurn string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "split <question-id>",
		Short: "Split a question group before a turn",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationQuestionSplit, map[string]any{
				"question_id":    args[0],
				"before_turn_id": beforeTurn,
				"dry_run":        dryRun,
			})
		},
	}
	cmd.Flags().StringVar(&beforeTurn, "before-turn", "", "turn id that starts the new question")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without splitting")
	_ = cmd.MarkFlagRequired("before-turn")
	return cmd
}

func newCmdConversationPart(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "part", Short: "Manage conversation content parts"}
	cmd.AddCommand(newCmdConversationPartTranslation(f))
	return cmd
}

func newCmdConversationPartTranslation(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "translation", Short: "Manage stored part translations"}
	cmd.AddCommand(newCmdConversationPartTranslationUpdate(f))
	return cmd
}

func newCmdConversationPartTranslationUpdate(f *cmdutil.Factory) *cobra.Command {
	var recordKind, text string
	cmd := &cobra.Command{
		Use:   "update <part-id>",
		Short: "Overwrite the stored translation for a content part",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationPartTranslationUpdate, map[string]any{
				"record_kind":     recordKind,
				"part_id":         args[0],
				"translated_text": text,
			})
		},
	}
	cmd.Flags().StringVar(&recordKind, "record-kind", "session", "conversation record kind: session or web")
	cmd.Flags().StringVar(&text, "text", "", "translated content to store")
	_ = cmd.MarkFlagRequired("text")
	return cmd
}

func paginationParams(limit, offset int) map[string]any {
	return map[string]any{
		"limit":  limit,
		"offset": offset,
	}
}
