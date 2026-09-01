package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

type memoryScopeFlags struct {
	appID, sourceID, projectPath, sessionID string
	currentProject                          bool
}

func newCmdMemory(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "memory", Short: "Manage progressive Memory and persistent Recall"}
	cmd.AddCommand(newCmdMemoryRecent(f), newCmdMemoryContext(f), newCmdMemoryProject(f), newCmdMemoryRebuild(f), newCmdMemoryTasks(f), newCmdMemoryRecall(f))
	return cmd
}

func newCmdMemoryRecent(f *cmdutil.Factory) *cobra.Command {
	var view string
	var limit, offset int
	parent := &cobra.Command{Use: "recent", Short: "Inspect recent conversation work"}
	cmd := &cobra.Command{Use: "list", Short: "List recent conversation work", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, _ []string) error {
		return callAndPrint(cmd, f, schema.MethodMemoryRecentList, map[string]any{"view": view, "limit": limit, "offset": offset})
	}}
	cmd.Flags().StringVar(&view, "view", "project", "ordering: project or time")
	cmd.Flags().IntVar(&limit, "limit", 50, "maximum sessions")
	cmd.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	parent.AddCommand(cmd)
	return parent
}

func newCmdMemoryContext(f *cmdutil.Factory) *cobra.Command {
	var scope memoryScopeFlags
	var query string
	var budget int
	parent := &cobra.Command{Use: "context", Short: "Resolve compiled Memory context"}
	cmd := &cobra.Command{Use: "resolve", Short: "Resolve the compiled Memory context", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		params := map[string]any{"project_path": resolved["project_path"], "query": nil, "token_budget": budget}
		if query != "" {
			params["query"] = query
		}
		return callAndPrint(cmd, f, schema.MethodMemoryContextResolve, params)
	}}
	addMemoryScopeFlags(cmd, &scope)
	cmd.Flags().StringVar(&query, "query", "", "optional relevance query")
	cmd.Flags().IntVar(&budget, "token-budget", 2000, "maximum context tokens")
	parent.AddCommand(cmd)
	return parent
}

func newCmdMemoryProject(f *cmdutil.Factory) *cobra.Command {
	parent := &cobra.Command{Use: "project", Short: "Inspect project Memory"}
	parent.AddCommand(&cobra.Command{Use: "get <project-path>", Short: "Get the project Memory projection", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, schema.MethodMemoryProjectGet, map[string]any{"project_path": args[0]})
	}})
	return parent
}

func newCmdMemoryRebuild(f *cmdutil.Factory) *cobra.Command {
	var scope memoryScopeFlags
	cmd := &cobra.Command{Use: "rebuild", Short: "Queue a Memory rebuild", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodMemoryRebuild, map[string]any{"scope": resolved})
	}}
	addMemoryRebuildScopeFlags(cmd, &scope)
	return cmd
}

func newCmdMemoryTasks(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "task", Short: "Inspect and control Memory background tasks"}
	var activeOnly bool
	list := &cobra.Command{Use: "list", Short: "List Memory tasks", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, _ []string) error {
		return callAndPrint(cmd, f, schema.MethodMemoryTaskList, map[string]any{"active_only": activeOnly})
	}}
	list.Flags().BoolVar(&activeOnly, "active-only", false, "only active tasks")
	cmd.AddCommand(list)
	cmd.AddCommand(memoryIDCommand(f, "get <task-id>", "Get a Memory task", schema.MethodMemoryTaskGet, "task_id"))
	cmd.AddCommand(memoryIDCommand(f, "cancel <task-id>", "Cancel a Memory task", schema.MethodMemoryTaskCancel, "task_id"))
	cmd.AddCommand(memoryIDCommand(f, "retry <task-id>", "Retry a failed Memory task", schema.MethodMemoryTaskRetry, "task_id"))
	return cmd
}

func newCmdMemoryRecall(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "recall", Short: "Search and continue persistent Recall sessions"}
	cmd.AddCommand(newCmdMemoryRecallSearch(f), newCmdMemoryRecallSession(f), newCmdMemoryRecallTurn(f))
	return cmd
}

func newCmdMemoryRecallSearch(f *cmdutil.Factory) *cobra.Command {
	var scope memoryScopeFlags
	var query string
	var limit, offset int
	cmd := &cobra.Command{Use: "search", Short: "Search Memory source content", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, _ []string) error {
		if query == "" {
			return fmt.Errorf("--query is required")
		}
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodMemoryRecallSearch, map[string]any{"query": query, "scope": resolved, "limit": limit, "offset": offset})
	}}
	addMemoryScopeFlags(cmd, &scope)
	cmd.Flags().StringVar(&query, "query", "", "search query")
	cmd.Flags().IntVar(&limit, "limit", 24, "maximum hits")
	cmd.Flags().IntVar(&offset, "offset", 0, "pagination offset")
	return cmd
}

func newCmdMemoryRecallSession(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "session", Short: "Manage persistent Recall sessions"}
	var scope memoryScopeFlags
	create := &cobra.Command{Use: "create", Short: "Create a Recall session", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, _ []string) error {
		resolved, err := scope.params()
		if err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodMemoryRecallSessionCreate, map[string]any{"scope": resolved})
	}}
	addMemoryScopeFlags(create, &scope)
	cmd.AddCommand(create)
	cmd.AddCommand(memoryIDCommand(f, "get <session-id>", "Get a Recall session", schema.MethodMemoryRecallSessionGet, "session_id"))
	return cmd
}

func newCmdMemoryRecallTurn(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "turn", Short: "Send or cancel persistent Recall turns"}
	var query string
	send := &cobra.Command{Use: "send <session-id>", Short: "Queue a Recall turn", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if query == "" {
			return fmt.Errorf("--query is required")
		}
		return callAndPrint(cmd, f, schema.MethodMemoryRecallTurnSend, map[string]any{"session_id": args[0], "query": query})
	}}
	send.Flags().StringVar(&query, "query", "", "Recall question")
	cmd.AddCommand(send)
	cmd.AddCommand(memoryIDCommand(f, "cancel <turn-id>", "Cancel a Recall turn", schema.MethodMemoryRecallTurnCancel, "turn_id"))
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

func addMemoryRebuildScopeFlags(cmd *cobra.Command, scope *memoryScopeFlags) {
	cmd.Flags().StringVar(&scope.projectPath, "project", "", "project path scope")
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
