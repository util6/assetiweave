package cmd

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdSkill(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "skill", Short: "Manage skills"}
	cmd.AddCommand(newCmdSkillList(f))
	cmd.AddCommand(newCmdSkillImport(f))
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
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List skill groups",
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodSkillGroupList, map[string]any{})
		},
	})
	cmd.AddCommand(newCmdSkillGroupMount(f, true))
	cmd.AddCommand(newCmdSkillGroupMount(f, false))
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
