package cmd

import (
	"encoding/json"
	"fmt"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func newCmdTeam(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "team", Short: "Manage teams and member rosters"}
	cmd.AddCommand(newCmdTeamList(f))
	cmd.AddCommand(newCmdTeamGet(f))
	cmd.AddCommand(newCmdTeamCreate(f))
	cmd.AddCommand(newCmdTeamUpdate(f))
	cmd.AddCommand(newCmdTeamDelete(f))
	return cmd
}

func newCmdTeamList(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all teams",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamList, map[string]any{})
		},
	}
}

func newCmdTeamGet(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "get <team-id>",
		Short: "Get team details and roster",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			params := map[string]any{
				"team_id": args[0],
			}
			return callAndPrint(cmd, f, schema.MethodTeamGet, params)
		},
	}
}

func newCmdTeamCreate(f *cmdutil.Factory) *cobra.Command {
	var id, name, description, membersJSON string
	cmd := &cobra.Command{
		Use:   "create",
		Short: "Create a team with leader and member roster",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			var members []any
			if membersJSON != "" {
				if err := json.Unmarshal([]byte(membersJSON), &members); err != nil {
					return fmt.Errorf("invalid members JSON: %w", err)
				}
			}
			params := map[string]any{
				"name":        name,
				"description": description,
				"members":     members,
			}
			if id != "" {
				params["id"] = id
			}
			return callAndPrint(cmd, f, schema.MethodTeamCreate, params)
		},
	}
	cmd.Flags().StringVar(&id, "id", "", "optional custom team identifier")
	cmd.Flags().StringVar(&name, "name", "", "team name")
	cmd.Flags().StringVar(&description, "description", "", "optional team description")
	cmd.Flags().StringVar(&membersJSON, "members", "[]", "JSON array of team members")
	_ = cmd.MarkFlagRequired("name")
	return cmd
}

func newCmdTeamUpdate(f *cmdutil.Factory) *cobra.Command {
	var name, description, membersJSON string
	cmd := &cobra.Command{
		Use:   "update <team-id>",
		Short: "Update team details and roster",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			var members []any
			if membersJSON != "" {
				if err := json.Unmarshal([]byte(membersJSON), &members); err != nil {
					return fmt.Errorf("invalid members JSON: %w", err)
				}
			}
			params := map[string]any{
				"team_id":     args[0],
				"name":        name,
				"description": description,
				"members":     members,
			}
			return callAndPrint(cmd, f, schema.MethodTeamUpdate, params)
		},
	}
	cmd.Flags().StringVar(&name, "name", "", "team name")
	cmd.Flags().StringVar(&description, "description", "", "optional team description")
	cmd.Flags().StringVar(&membersJSON, "members", "[]", "JSON array of updated team members")
	_ = cmd.MarkFlagRequired("name")
	return cmd
}

func newCmdTeamDelete(f *cmdutil.Factory) *cobra.Command {
	var yes bool
	cmd := &cobra.Command{
		Use:   "delete <team-id>",
		Short: "Delete a team and its roster",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := requireYes(yes, "Team delete"); err != nil {
				return err
			}
			params := map[string]any{
				"team_id": args[0],
				"yes":     true,
			}
			return callAndPrint(cmd, f, schema.MethodTeamDelete, params)
		},
	}
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm deletion of team")
	return cmd
}
