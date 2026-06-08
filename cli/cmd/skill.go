package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdSkill(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "skill", Short: "Manage skills"}
	cmd.AddCommand(newCmdSkillList(f))
	cmd.AddCommand(newCmdSkillImport(f))
	cmd.AddCommand(newCmdSkillSearch(f))
	cmd.AddCommand(newCmdSkillAcquire(f))
	cmd.AddCommand(newCmdSkillRemote(f))
	cmd.AddCommand(newCmdSkillBackup(f))
	cmd.AddCommand(newCmdSkillMount(f))
	cmd.AddCommand(newCmdSkillUnmount(f))
	cmd.AddCommand(newCmdSkillDelete(f))
	cmd.AddCommand(newCmdSkillGroup(f))
	return cmd
}

func newCmdSkillList(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List skills",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillList, map[string]any{})
		},
	}
}

func newCmdSkillImport(f *cmdutil.Factory) *cobra.Command {
	var from, name string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "import",
		Short: "Import an installed skill directory into the AssetIWeave backup library",
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{"from": from, "name": nil, "dry_run": dryRun}
			if name != "" {
				params["name"] = name
			}
			return callAndPrint(cmd, f, schema.MethodSkillImport, params)
		},
	}
	cmd.Flags().StringVar(&from, "from", "", "installed skill directory containing SKILL.md")
	cmd.Flags().StringVar(&name, "name", "", "imported skill name")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without copying")
	_ = cmd.MarkFlagRequired("from")
	return cmd
}

func newCmdSkillSearch(f *cmdutil.Factory) *cobra.Command {
	var query, provider string
	var limit int
	cmd := &cobra.Command{
		Use:   "search",
		Short: "Search internet providers for skills",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{"query": query}
			if provider != "" {
				params["provider"] = provider
			}
			if cmd.Flags().Changed("limit") {
				params["limit"] = limit
			}
			return callAndPrint(cmd, f, schema.MethodSkillSearch, params)
		},
	}
	cmd.Flags().StringVar(&query, "query", "", "skill search query")
	cmd.Flags().StringVar(&provider, "provider", "github", "search provider")
	cmd.Flags().IntVar(&limit, "limit", 10, "maximum candidate count")
	_ = cmd.MarkFlagRequired("query")
	return cmd
}

func newCmdSkillAcquire(f *cmdutil.Factory) *cobra.Command {
	var url, branch, path, name string
	var dryRun, yes bool
	cmd := &cobra.Command{
		Use:   "acquire",
		Short: "Download and import a skill candidate",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			if !dryRun {
				if err := requireYes(yes, "skill acquire"); err != nil {
					return err
				}
			}
			params := map[string]any{
				"url":     url,
				"dry_run": dryRun,
				"yes":     yes,
			}
			if branch != "" {
				params["branch"] = branch
			}
			if path != "" {
				params["path"] = path
			}
			if name != "" {
				params["name"] = name
			}
			return callAndPrint(cmd, f, schema.MethodSkillAcquire, params)
		},
	}
	cmd.Flags().StringVar(&url, "url", "", "GitHub repository, tree, or blob URL")
	cmd.Flags().StringVar(&branch, "branch", "", "Git branch override")
	cmd.Flags().StringVar(&path, "path", "", "skill directory path inside the repository")
	cmd.Flags().StringVar(&name, "name", "", "imported skill name")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without cloning or importing")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm download and import")
	_ = cmd.MarkFlagRequired("url")
	return cmd
}

func newCmdSkillRemote(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "remote", Short: "Inspect acquired skill remote sources"}
	cmd.AddCommand(newCmdSkillRemoteList(f))
	cmd.AddCommand(newCmdSkillRemoteCheck(f))
	return cmd
}

func newCmdSkillRemoteList(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List acquired skill remote sources",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillRemoteList, map[string]any{})
		},
	}
}

func newCmdSkillRemoteCheck(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "check [asset-id]",
		Short: "Check acquired skill remote sources for drift",
		Args:  cobra.RangeArgs(0, 1),
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{}
			if len(args) == 1 {
				params["asset_id"] = args[0]
			}
			return callAndPrint(cmd, f, schema.MethodSkillRemoteCheck, params)
		},
	}
}

func newCmdSkillBackup(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "backup <asset-id>",
		Short: "Copy a skill into the AssetIWeave backup library",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillBackup, map[string]any{"asset_id": args[0]})
		},
	}
}

func newCmdSkillMount(f *cmdutil.Factory) *cobra.Command {
	var profile string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "mount <asset-ref>",
		Short: "Mount a skill to a target profile",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillMount, map[string]any{"asset_ref": args[0], "profile_id": profile, "dry_run": dryRun})
		},
	}
	cmd.Flags().StringVar(&profile, "profile", "", "target profile id")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without mounting")
	_ = cmd.MarkFlagRequired("profile")
	return cmd
}

func newCmdSkillUnmount(f *cmdutil.Factory) *cobra.Command {
	var profile string
	var dryRun bool
	cmd := &cobra.Command{
		Use:   "unmount <asset-ref>",
		Short: "Unmount a skill from a target profile",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillUnmount, map[string]any{"asset_ref": args[0], "profile_id": profile, "dry_run": dryRun})
		},
	}
	cmd.Flags().StringVar(&profile, "profile", "", "target profile id")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without unmounting")
	_ = cmd.MarkFlagRequired("profile")
	return cmd
}

func newCmdSkillDelete(f *cmdutil.Factory) *cobra.Command {
	var dryRun, yes, unmount bool
	cmd := &cobra.Command{
		Use:   "delete <asset-ref>",
		Short: "Delete an AssetIWeave backup-library skill",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !dryRun {
				if err := requireYes(yes, "skill delete"); err != nil {
					return err
				}
			}
			return callAndPrint(cmd, f, schema.MethodSkillDelete, map[string]any{
				"asset_ref": args[0],
				"dry_run":   dryRun,
				"yes":       yes,
				"unmount":   unmount,
			})
		},
	}
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without deleting")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm deletion")
	cmd.Flags().BoolVar(&unmount, "unmount", false, "unmount enabled managed mounts before deleting")
	return cmd
}

func newCmdSkillGroup(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "group", Short: "Manage skill groups"}
	cmd.AddCommand(newCmdSkillGroupList(f))
	cmd.AddCommand(newCmdSkillGroupShow(f))
	cmd.AddCommand(newCmdSkillGroupCreate(f))
	cmd.AddCommand(newCmdSkillGroupUpdate(f))
	cmd.AddCommand(newCmdSkillGroupDelete(f))
	cmd.AddCommand(newCmdSkillGroupMembers(f))
	cmd.AddCommand(newCmdSkillGroupMount(f, true))
	cmd.AddCommand(newCmdSkillGroupMount(f, false))
	cmd.AddCommand(newCmdSkillGroupExclusive(f))
	return cmd
}

func newCmdSkillGroupList(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List skill groups",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillGroupList, map[string]any{})
		},
	}
}

func newCmdSkillGroupShow(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "show <group-id>",
		Short: "Show a skill group with resolved members",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillGroupGet, map[string]any{"group_id": args[0]})
		},
	}
}

func newCmdSkillGroupCreate(f *cmdutil.Factory) *cobra.Command {
	var id, name, description, color, nameContains, jsonArg string
	var disabled bool
	var sortOrder int
	var sourceIDs, pathGlobs []string
	cmd := &cobra.Command{
		Use:   "create",
		Short: "Create a skill group",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			input, err := skillGroupCreateInput(cmd, f, skillGroupCreateOptions{
				id:           id,
				name:         name,
				description:  description,
				color:        color,
				disabled:     disabled,
				sortOrder:    sortOrder,
				sourceIDs:    sourceIDs,
				pathGlobs:    pathGlobs,
				nameContains: nameContains,
				jsonArg:      jsonArg,
			})
			if err != nil {
				return err
			}
			return callAndPrint(cmd, f, schema.MethodSkillGroupCreate, map[string]any{"input": input})
		},
	}
	cmd.Flags().StringVar(&id, "id", "", "skill group id")
	cmd.Flags().StringVar(&name, "name", "", "skill group name")
	cmd.Flags().StringVar(&description, "description", "", "skill group description")
	cmd.Flags().StringVar(&color, "color", "", "skill group display color")
	cmd.Flags().BoolVar(&disabled, "disabled", false, "create the group disabled")
	cmd.Flags().IntVar(&sortOrder, "sort-order", 0, "skill group sort order")
	cmd.Flags().StringArrayVar(&sourceIDs, "source", nil, "source id rule; repeatable")
	cmd.Flags().StringArrayVar(&pathGlobs, "path-glob", nil, "relative path glob rule; repeatable")
	cmd.Flags().StringVar(&nameContains, "name-contains", "", "name contains rule")
	cmd.Flags().StringVar(&jsonArg, "json", "", "complete AssetGroupInput JSON, @file, or - for stdin")
	return cmd
}

type skillGroupCreateOptions struct {
	id           string
	name         string
	description  string
	color        string
	disabled     bool
	sortOrder    int
	sourceIDs    []string
	pathGlobs    []string
	nameContains string
	jsonArg      string
}

func skillGroupCreateInput(cmd *cobra.Command, f *cmdutil.Factory, options skillGroupCreateOptions) (map[string]any, error) {
	if options.jsonArg != "" {
		return readJSONParams(f, options.jsonArg)
	}
	if options.name == "" {
		return nil, errs.NewValidationError(errs.SubtypeInvalidArgument, "--name is required unless --json is used").
			WithCode("validation")
	}

	input := map[string]any{"name": options.name}
	if options.id != "" {
		input["id"] = options.id
	}
	if options.description != "" {
		input["description"] = options.description
	}
	if options.color != "" {
		input["color"] = options.color
	}
	if options.disabled {
		input["enabled"] = false
	}
	if cmd.Flags().Changed("sort-order") {
		input["sort_order"] = options.sortOrder
	}
	if len(options.sourceIDs) > 0 || len(options.pathGlobs) > 0 || options.nameContains != "" {
		sourceIDs := options.sourceIDs
		if sourceIDs == nil {
			sourceIDs = []string{}
		}
		pathGlobs := options.pathGlobs
		if pathGlobs == nil {
			pathGlobs = []string{}
		}
		var nameContains any
		if options.nameContains != "" {
			nameContains = options.nameContains
		}
		input["rules"] = map[string]any{
			"source_ids":          sourceIDs,
			"relative_path_globs": pathGlobs,
			"name_contains":       nameContains,
		}
	}
	return input, nil
}

func newCmdSkillGroupUpdate(f *cmdutil.Factory) *cobra.Command {
	var jsonArg string
	cmd := &cobra.Command{
		Use:   "update <group-id>",
		Short: "Update a skill group from a complete AssetGroup JSON object",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if jsonArg == "" {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "--json is required").
					WithCode("validation")
			}
			group, err := readJSONParams(f, jsonArg)
			if err != nil {
				return err
			}
			if currentID, ok := group["id"].(string); ok && currentID != "" && currentID != args[0] {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "--json group id %q does not match %q", currentID, args[0]).
					WithCode("validation")
			}
			group["id"] = args[0]
			return callAndPrint(cmd, f, schema.MethodSkillGroupUpdate, map[string]any{"group": group})
		},
	}
	cmd.Flags().StringVar(&jsonArg, "json", "", "complete AssetGroup JSON, @file, or - for stdin")
	return cmd
}

func newCmdSkillGroupDelete(f *cmdutil.Factory) *cobra.Command {
	var yes bool
	cmd := &cobra.Command{
		Use:   "delete <group-id>",
		Short: "Delete a skill group",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := requireYes(yes, "skill group delete"); err != nil {
				return err
			}
			return callAndPrint(cmd, f, schema.MethodSkillGroupDelete, map[string]any{"group_id": args[0], "yes": yes})
		},
	}
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm deletion")
	return cmd
}

func newCmdSkillGroupMembers(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "members", Short: "Manage skill group manual members"}
	cmd.AddCommand(newCmdSkillGroupMembersSet(f))
	return cmd
}

func newCmdSkillGroupMembersSet(f *cmdutil.Factory) *cobra.Command {
	var assetIDs []string
	var clear bool
	cmd := &cobra.Command{
		Use:   "set <group-id>",
		Short: "Replace manual members of a skill group",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if clear && len(assetIDs) > 0 {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "--clear cannot be combined with --asset").
					WithCode("validation")
			}
			if len(assetIDs) == 0 && !clear {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "at least one --asset or --clear is required").
					WithCode("validation")
			}
			if assetIDs == nil {
				assetIDs = []string{}
			}
			return callAndPrint(cmd, f, schema.MethodSkillGroupMembersSet, map[string]any{
				"group_id":  args[0],
				"asset_ids": assetIDs,
			})
		},
	}
	cmd.Flags().StringArrayVar(&assetIDs, "asset", nil, "manual member asset id; repeatable")
	cmd.Flags().BoolVar(&clear, "clear", false, "clear all manual members")
	return cmd
}

func newCmdSkillGroupMount(f *cmdutil.Factory, enabled bool) *cobra.Command {
	var profile string
	var dryRun, yes bool
	use := "mount <group-id>"
	method := schema.MethodSkillGroupMount
	short := "Mount a skill group to a target profile"
	if !enabled {
		use = "unmount <group-id>"
		method = schema.MethodSkillGroupUnmount
		short = "Unmount a skill group from a target profile"
	}
	cmd := &cobra.Command{
		Use:   use,
		Short: short,
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !enabled && !dryRun {
				if err := requireYes(yes, "skill group unmount"); err != nil {
					return err
				}
			}
			return callAndPrint(cmd, f, method, map[string]any{"group_id": args[0], "profile_id": profile, "dry_run": dryRun, "yes": yes})
		},
	}
	cmd.Flags().StringVar(&profile, "profile", "", "target profile id")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "preview without changing mounts")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm destructive operation")
	_ = cmd.MarkFlagRequired("profile")
	return cmd
}

func newCmdSkillGroupExclusive(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "exclusive", Short: "Plan or apply exclusive skill group mounts"}
	cmd.AddCommand(newCmdSkillGroupExclusivePreview(f))
	cmd.AddCommand(newCmdSkillGroupExclusiveApply(f))
	return cmd
}

func newCmdSkillGroupExclusivePreview(f *cmdutil.Factory) *cobra.Command {
	var profile string
	var groupIDs []string
	cmd := &cobra.Command{
		Use:   "preview",
		Short: "Preview an exclusive skill group mount operation",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(groupIDs) == 0 {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "at least one --group is required").
					WithCode("validation")
			}
			return callAndPrint(cmd, f, schema.MethodSkillGroupExclusivePreview, map[string]any{
				"input": skillGroupExclusiveInput(groupIDs, profile, true),
			})
		},
	}
	cmd.Flags().StringArrayVar(&groupIDs, "group", nil, "skill group id; repeatable")
	cmd.Flags().StringVar(&profile, "profile", "", "target profile id")
	_ = cmd.MarkFlagRequired("profile")
	return cmd
}

func newCmdSkillGroupExclusiveApply(f *cmdutil.Factory) *cobra.Command {
	var profile string
	var groupIDs []string
	var yes bool
	cmd := &cobra.Command{
		Use:   "apply",
		Short: "Apply an exclusive skill group mount operation",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(groupIDs) == 0 {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "at least one --group is required").
					WithCode("validation")
			}
			if err := requireYes(yes, "skill group exclusive apply"); err != nil {
				return err
			}
			return callAndPrint(cmd, f, schema.MethodSkillGroupExclusiveApply, map[string]any{
				"input": skillGroupExclusiveInput(groupIDs, profile, false),
				"yes":   yes,
			})
		},
	}
	cmd.Flags().StringArrayVar(&groupIDs, "group", nil, "skill group id; repeatable")
	cmd.Flags().StringVar(&profile, "profile", "", "target profile id")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm exclusive mount changes")
	_ = cmd.MarkFlagRequired("profile")
	return cmd
}

func skillGroupExclusiveInput(groupIDs []string, profile string, dryRun bool) map[string]any {
	return map[string]any{
		"group_ids":      groupIDs,
		"profile_id":     profile,
		"mount_selected": true,
		"dry_run":        dryRun,
	}
}
