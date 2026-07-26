package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/schema"
)

type memoryScopeFlags struct {
	appID, sourceID, projectPath, sessionID string
	currentProject                          bool
}

func newCmdMemory(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "memory", Short: "Manage progressive Memory, Dreams, and evidence-backed Recall"}
	cmd.AddCommand(newCmdMemoryOverview(f), newCmdMemoryDream(f), newCmdMemoryRecall(f), newCmdMemoryItem(f), newCmdMemoryCandidate(f), newCmdMemoryVerify(f))
	return cmd
}

func newCmdMemoryOverview(f *cmdutil.Factory) *cobra.Command {
	var scope memoryScopeFlags
	cmd := &cobra.Command{Use: "overview", Short: "Show the deterministic local Memory overview", RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodMemoryOverview, map[string]any{"scope": resolved})
	}}
	addMemoryScopeFlags(cmd, &scope)
	return cmd
}

func newCmdMemoryDream(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "dream", Short: "Inspect and run bounded incremental Dreams"}
	for _, spec := range []struct{ use, short, method string }{
		{"status", "Explain Auto-Dream gates", schema.MethodMemoryDreamStatus},
		{"preview", "Preview the stable Dream delta without AI or writes", schema.MethodMemoryDreamPreview},
	} {
		var scope memoryScopeFlags
		method := spec.method
		child := &cobra.Command{Use: spec.use, Short: spec.short, RunE: func(cmd *cobra.Command, _ []string) error {
			resolved, err := scope.params()
			if err != nil {
				return err
			}
			params := map[string]any{"scope": resolved}
			if method == schema.MethodMemoryDreamPreview {
				params["trigger"] = "manual"
			}
			return callAndPrint(cmd, f, method, params)
		}}
		addMemoryScopeFlags(child, &scope)
		cmd.AddCommand(child)
	}
	var runScope memoryScopeFlags
	var dryRun bool
	run := &cobra.Command{Use: "run", Short: "Run a Dream using the configured external AI runtime", RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := runScope.params()
		if err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodMemoryDreamRun, map[string]any{"scope": resolved, "trigger": "manual", "dry_run": dryRun})
	}}
	addMemoryScopeFlags(run, &runScope)
	run.Flags().BoolVar(&dryRun, "dry-run", false, "preview without AI or writes")
	cmd.AddCommand(run)
	cmd.AddCommand(memoryIDCommand(f, "get <note-id>", "Get a Dream Note with evidence", schema.MethodMemoryDreamGet, "note_id"))
	cmd.AddCommand(memoryIDCommand(f, "archive <note-id>", "Archive a Dream Note", schema.MethodMemoryDreamArchive, "note_id"))
	cmd.AddCommand(memoryIDCommand(f, "promote <note-id>", "Copy Dream bullets into review candidates", schema.MethodMemoryDreamPromote, "note_id"))
	var statuses []string
	var scope memoryScopeFlags
	var limit, offset int
	list := &cobra.Command{Use: "list", Short: "List Dream Notes", RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodMemoryDreamList, map[string]any{"statuses": statuses, "scope": resolved, "limit": limit, "offset": offset})
	}}
	addMemoryScopeFlags(list, &scope)
	list.Flags().StringSliceVar(&statuses, "status", nil, "Dream status filter")
	list.Flags().IntVar(&limit, "limit", 50, "maximum notes")
	list.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	cmd.AddCommand(list)
	return cmd
}

func newCmdMemoryRecall(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "recall", Short: "Build local evidence bundles or run two-phase synthesis"}
	cmd.AddCommand(newCmdMemoryRecallAction(f, "preview", schema.MethodMemoryRecallPreview, false))
	cmd.AddCommand(newCmdMemoryRecallAction(f, "run", schema.MethodMemoryRecallRun, true))
	return cmd
}

func newCmdMemoryRecallAction(f *cmdutil.Factory, use, method string, run bool) *cobra.Command {
	var scope memoryScopeFlags
	var query, since, until, format string
	var full, ai, includeUnavailable, dryRun bool
	var limit, offset int
	cmd := &cobra.Command{Use: use, Short: "Preview or execute evidence-backed Recall", RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		mode := "exact"
		if full {
			mode = "full"
		}
		if mode == "exact" && query == "" {
			return fmt.Errorf("--query is required for exact Recall")
		}
		params := map[string]any{"mode": mode, "scope": resolved, "query": nil, "since": nil, "until": nil, "include_unavailable": includeUnavailable, "limit": limit, "offset": offset}
		if query != "" {
			params["query"] = query
		}
		if since != "" {
			params["since"] = since
		}
		if until != "" {
			params["until"] = until
		}
		if run {
			params["synthesize"] = ai
			params["dry_run"] = dryRun
		}
		result, err := callEngine(cmd, f, method, params)
		if err != nil {
			return err
		}
		if format == "json" {
			if result.Meta == nil {
				output.WriteSuccess(f.IOStreams.Out, result.Data)
			} else {
				output.WriteSuccessWithMeta(f.IOStreams.Out, result.Data, result.Meta)
			}
			return nil
		}
		if format != "compact-json" {
			return fmt.Errorf("unsupported Memory Recall format %q: use json or compact-json", format)
		}
		var value map[string]any
		if err := json.Unmarshal(result.Data, &value); err != nil {
			return fmt.Errorf("decode Memory Recall result: %w", err)
		}
		compactMemoryRecall(value)
		if result.Meta == nil {
			output.WriteSuccess(f.IOStreams.Out, value)
		} else {
			output.WriteSuccessWithMeta(f.IOStreams.Out, value, result.Meta)
		}
		return nil
	}}
	addMemoryScopeFlags(cmd, &scope)
	cmd.Flags().StringVar(&query, "query", "", "question for exact Recall")
	cmd.Flags().BoolVar(&full, "full", false, "organize every eligible Question in the explicit scope")
	cmd.Flags().BoolVar(&ai, "ai", false, "use the configured external AI runtime for two-phase synthesis")
	cmd.Flags().BoolVar(&includeUnavailable, "include-unavailable", false, "include bounded retained or missing records")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without writes or AI")
	cmd.Flags().StringVar(&since, "since", "", "inclusive start timestamp")
	cmd.Flags().StringVar(&until, "until", "", "inclusive end timestamp")
	cmd.Flags().IntVar(&limit, "limit", 50, "maximum Questions in this page")
	cmd.Flags().IntVar(&offset, "offset", 0, "Question pagination offset")
	cmd.Flags().StringVar(&format, "format", "json", "output format: json or compact-json")
	return cmd
}

func newCmdMemoryItem(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "item", Short: "Manage formal Memory and candidates"}
	cmd.AddCommand(memoryIDCommand(f, "get <item-id>", "Get a Memory item", schema.MethodMemoryItemGet, "item_id"))
	cmd.AddCommand(memoryIDCommand(f, "archive <item-id>", "Archive a Memory item", schema.MethodMemoryItemArchive, "item_id"))
	var kinds, statuses, origins []string
	var stale bool
	var scope memoryScopeFlags
	var limit, offset int
	list := &cobra.Command{Use: "list", Short: "List Memory items", RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodMemoryItemList, map[string]any{"kinds": kinds, "statuses": statuses, "origins": origins, "scope": resolved, "stale_only": stale, "limit": limit, "offset": offset})
	}}
	addMemoryScopeFlags(list, &scope)
	list.Flags().StringSliceVar(&kinds, "kind", nil, "Memory kind filter")
	list.Flags().StringSliceVar(&statuses, "status", nil, "Memory status filter")
	list.Flags().StringSliceVar(&origins, "origin", nil, "Memory origin filter")
	list.Flags().BoolVar(&stale, "stale", false, "only stale items")
	list.Flags().IntVar(&limit, "limit", 50, "maximum items")
	list.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	cmd.AddCommand(list)
	cmd.AddCommand(newCmdMemoryItemWrite(f, "create", schema.MethodMemoryItemCreate, false), newCmdMemoryItemWrite(f, "update <item-id>", schema.MethodMemoryItemUpdate, true))
	return cmd
}

func newCmdMemoryItemWrite(f *cmdutil.Factory, use, method string, update bool) *cobra.Command {
	var kind, title, content string
	var confidence float64
	var scope memoryScopeFlags
	var evidence []string
	cmd := &cobra.Command{Use: use, Short: "Create or update a Memory item", Args: func(cmd *cobra.Command, args []string) error {
		if update {
			return cobra.ExactArgs(1)(cmd, args)
		}
		return cobra.NoArgs(cmd, args)
	}, RunE: func(cmd *cobra.Command, args []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		params := map[string]any{}
		if !update || cmd.Flags().Changed("kind") {
			params["kind"] = kind
		}
		if !update || cmd.Flags().Changed("title") {
			params["title"] = title
		}
		if !update || cmd.Flags().Changed("content") {
			params["content_markdown"] = content
		}
		if !update || cmd.Flags().Changed("confidence") {
			params["confidence"] = confidence
		}
		if !update || cmd.Flags().Changed("evidence") {
			params["evidence_ids"] = evidence
		}
		if !update || scopeHasValue(scope) {
			params["scope"] = resolved
		}
		if update {
			params["item_id"] = args[0]
		}
		return callAndPrint(cmd, f, method, params)
	}}
	addMemoryScopeFlags(cmd, &scope)
	cmd.Flags().StringVar(&kind, "kind", "context", "Memory kind")
	cmd.Flags().StringVar(&title, "title", "", "Memory title")
	cmd.Flags().StringVar(&content, "content", "", "Memory Markdown content")
	cmd.Flags().Float64Var(&confidence, "confidence", 1, "confidence from 0 to 1")
	cmd.Flags().StringSliceVar(&evidence, "evidence", nil, "evidence snapshot id; repeat or comma-separate")
	if !update {
		_ = cmd.MarkFlagRequired("title")
		_ = cmd.MarkFlagRequired("content")
	}
	return cmd
}

func newCmdMemoryCandidate(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "candidate", Short: "Review Memory candidates"}
	cmd.AddCommand(memoryIDCommand(f, "accept <item-id>", "Accept a candidate", schema.MethodMemoryCandidateAccept, "item_id"), memoryIDCommand(f, "reject <item-id>", "Reject a candidate", schema.MethodMemoryCandidateReject, "item_id"))
	return cmd
}

func newCmdMemoryVerify(f *cmdutil.Factory) *cobra.Command {
	var ids []string
	cmd := &cobra.Command{Use: "verify", Short: "Verify selected Memory evidence freshness", RunE: func(cmd *cobra.Command, _ []string) error {
		if len(ids) == 0 {
			return fmt.Errorf("at least one --item is required")
		}
		return callAndPrint(cmd, f, schema.MethodMemoryVerify, map[string]any{"item_ids": ids})
	}}
	cmd.Flags().StringSliceVar(&ids, "item", nil, "Memory item id; repeat or comma-separate")
	return cmd
}

func memoryIDCommand(f *cmdutil.Factory, use, short, method, key string) *cobra.Command {
	return &cobra.Command{Use: use, Short: short, Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, method, map[string]any{key: args[0]})
	}}
}

func addMemoryScopeFlags(cmd *cobra.Command, scope *memoryScopeFlags) {
	cmd.Flags().StringVar(&scope.appID, "app", "", "app or adapter scope")
	cmd.Flags().StringVar(&scope.sourceID, "source", "", "source scope")
	cmd.Flags().StringVar(&scope.projectPath, "project", "", "project path scope")
	cmd.Flags().StringVar(&scope.sessionID, "session", "", "session scope")
	cmd.Flags().BoolVar(&scope.currentProject, "current-project", false, "use the current working directory as project scope")
}

func (scope memoryScopeFlags) params() (map[string]any, error) {
	project := scope.projectPath
	if scope.currentProject {
		wd, err := os.Getwd()
		if err != nil {
			return nil, err
		}
		project = wd
	}
	value := map[string]any{"app_id": nil, "source_id": nil, "project_path": nil, "session_id": nil}
	if scope.appID != "" {
		value["app_id"] = scope.appID
	}
	if scope.sourceID != "" {
		value["source_id"] = scope.sourceID
	}
	if project != "" {
		value["project_path"] = project
	}
	if scope.sessionID != "" {
		value["session_id"] = scope.sessionID
	}
	return value, nil
}

func scopeHasValue(scope memoryScopeFlags) bool {
	return scope.appID != "" || scope.sourceID != "" || scope.projectPath != "" || scope.sessionID != "" || scope.currentProject
}

func compactMemoryRecall(value map[string]any) {
	preview, _ := value["preview"].(map[string]any)
	if preview == nil {
		preview = value
	}
	delete(preview, "formal_matches")
	delete(preview, "dream_matches")
	if evidence, ok := preview["evidence"].([]any); ok {
		for _, raw := range evidence {
			if item, ok := raw.(map[string]any); ok {
				if snapshot, ok := item["snapshot"].(map[string]any); ok {
					delete(snapshot, "translated_excerpt")
				}
			}
		}
	}
}
