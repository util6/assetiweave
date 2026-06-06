package cmd

import (
	"time"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdConversation(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "conversation", Short: "Manage normalized conversation records"}
	cmd.AddCommand(newCmdConversationAdapter(f))
	cmd.AddCommand(newCmdConversationSource(f))
	cmd.AddCommand(newCmdConversationSync(f))
	cmd.AddCommand(newCmdConversationSession(f))
	cmd.AddCommand(newCmdConversationQuestion(f))
	return cmd
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
	cmd.AddCommand(newCmdConversationAdapterRegister(f))
	cmd.AddCommand(newCmdConversationAdapterUnregister(f))
	cmd.AddCommand(newCmdConversationAdapterTryRun(f))
	return cmd
}

func newCmdConversationAdapterScaffold(f *cmdutil.Factory) *cobra.Command {
	var directory, id, name string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "scaffold",
		Short: "Create a language-neutral adapter manifest scaffold",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodConversationAdapterScaffold, map[string]any{
				"directory": directory,
				"id":        id,
				"name":      name,
				"dry_run":   dryRun,
			})
		},
	}
	cmd.Flags().StringVar(&directory, "directory", "", "directory where scaffold files will be created")
	cmd.Flags().StringVar(&id, "id", "", "adapter id")
	cmd.Flags().StringVar(&name, "name", "", "adapter display name")
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

func newCmdConversationSync(f *cmdutil.Factory) *cobra.Command {
	var sourceID, adapterID string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "sync",
		Short: "Synchronize conversation sources",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{"source_id": nil, "adapter_id": nil, "dry_run": dryRun}
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

func newCmdConversationQuestion(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "question", Short: "Manage session question groups"}
	cmd.AddCommand(newCmdConversationQuestionList(f))
	cmd.AddCommand(newCmdConversationQuestionGet(f))
	cmd.AddCommand(newCmdConversationQuestionMerge(f))
	cmd.AddCommand(newCmdConversationQuestionSplit(f))
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

func paginationParams(limit, offset int) map[string]any {
	return map[string]any{
		"limit":  limit,
		"offset": offset,
	}
}
