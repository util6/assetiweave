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
	cmd.AddCommand(newCmdTeamMember(f))
	cmd.AddCommand(newCmdTeamLeader(f))
	cmd.AddCommand(newCmdTeamRun(f))
	cmd.AddCommand(newCmdTeamTask(f))
	cmd.AddCommand(newCmdTeamMailbox(f))
	cmd.AddCommand(newCmdTeamTool(f))
	return cmd
}

func newCmdTeamMember(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "member", Short: "Run and inspect a member Session"}
	cmd.AddCommand(newCmdTeamMemberTurn(f))
	cmd.AddCommand(newCmdTeamMemberReplay(f))
	cmd.AddCommand(newCmdTeamMemberStream(f))
	cmd.AddCommand(newCmdTeamMemberTask(f))
	cmd.AddCommand(newCmdTeamMemberCancel(f))
	return cmd
}

func newCmdTeamMemberTurn(f *cmdutil.Factory) *cobra.Command {
	var memberID, message string
	cmd := &cobra.Command{
		Use:   "turn <team-id>",
		Short: "Start a background turn for a Team member",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamMemberTurnStart, map[string]any{
				"team_id": args[0], "member_id": memberID, "message": message, "replay": false,
			})
		},
	}
	cmd.Flags().StringVar(&memberID, "member-id", "", "Team member identifier")
	cmd.Flags().StringVar(&message, "message", "", "message for the Team member")
	_ = cmd.MarkFlagRequired("member-id")
	_ = cmd.MarkFlagRequired("message")
	return cmd
}

func newCmdTeamMemberReplay(f *cmdutil.Factory) *cobra.Command {
	var memberID string
	cmd := &cobra.Command{
		Use:   "replay <team-id>",
		Short: "Replay a Team member's provider history",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamMemberReplayStart, map[string]any{
				"team_id": args[0], "member_id": memberID,
			})
		},
	}
	cmd.Flags().StringVar(&memberID, "member-id", "", "Team member identifier")
	_ = cmd.MarkFlagRequired("member-id")
	return cmd
}

func newCmdTeamMemberStream(f *cmdutil.Factory) *cobra.Command {
	var memberID, executionID string
	cmd := &cobra.Command{
		Use:   "stream <team-id>",
		Short: "Get a Team member Session stream snapshot",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamMemberStreamSnapshot, map[string]any{
				"team_id": args[0], "member_id": memberID, "execution_id": executionID,
			})
		},
	}
	cmd.Flags().StringVar(&memberID, "member-id", "", "Team member identifier")
	cmd.Flags().StringVar(&executionID, "execution-id", "", "member execution identifier")
	_ = cmd.MarkFlagRequired("member-id")
	_ = cmd.MarkFlagRequired("execution-id")
	return cmd
}

func newCmdTeamMemberTask(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "task", Short: "Inspect Team member turn tasks"}
	cmd.AddCommand(newCmdTeamMemberTaskGet(f))
	cmd.AddCommand(newCmdTeamMemberTaskList(f))
	return cmd
}

func newCmdTeamMemberTaskGet(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "get <task-id>",
		Short: "Get a Team member turn task snapshot",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamMemberTaskGet, map[string]any{"task_id": args[0]})
		},
	}
}

func newCmdTeamMemberTaskList(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List Team member turn tasks",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamMemberTasksList, map[string]any{})
		},
	}
}

func newCmdTeamMemberCancel(f *cmdutil.Factory) *cobra.Command {
	var memberID, executionID string
	cmd := &cobra.Command{
		Use:   "cancel <team-id>",
		Short: "Cancel a Team member background turn",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamMemberTurnCancel, map[string]any{
				"team_id": args[0], "member_id": memberID, "execution_id": executionID,
			})
		},
	}
	cmd.Flags().StringVar(&memberID, "member-id", "", "Team member identifier")
	cmd.Flags().StringVar(&executionID, "execution-id", "", "member execution identifier")
	_ = cmd.MarkFlagRequired("member-id")
	_ = cmd.MarkFlagRequired("execution-id")
	return cmd
}

func newCmdTeamLeader(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "leader", Short: "Chat with the Team leader"}
	cmd.AddCommand(newCmdTeamLeaderChat(f))
	return cmd
}

func newCmdTeamLeaderChat(f *cmdutil.Factory) *cobra.Command {
	var message string
	var replay bool
	cmd := &cobra.Command{
		Use:   "chat <team-id>",
		Short: "Chat with or replay the persistent Team leader",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return callAndPrint(cmd, f, schema.MethodTeamLeaderChat, map[string]any{
				"team_id": args[0], "message": message, "replay": replay,
			})
		},
	}
	cmd.Flags().StringVar(&message, "message", "", "message for the Team leader")
	cmd.Flags().BoolVar(&replay, "replay", false, "replay the saved leader history")
	return cmd
}

func newCmdTeamRun(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "run", Short: "Draft, review, inspect, and confirm Team runs"}
	cmd.AddCommand(newCmdTeamRunDraft(f))
	cmd.AddCommand(newCmdTeamRunGet(f))
	cmd.AddCommand(newCmdTeamRunReview(f))
	cmd.AddCommand(newCmdTeamRunConfirm(f))
	cmd.AddCommand(newCmdTeamRunRestore(f))
	return cmd
}

func newCmdTeamRunRestore(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{Use: "restore <run-id>", Short: "Restore Team history and member readiness", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, schema.MethodTeamRunRestore, map[string]any{"run_id": args[0]})
	}}
}

func newCmdTeamRunDraft(f *cmdutil.Factory) *cobra.Command {
	var message string
	cmd := &cobra.Command{Use: "draft <team-id>", Short: "Create a structured Team task draft", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, schema.MethodTeamRunDraft, map[string]any{"team_id": args[0], "leader_message": message})
	}}
	cmd.Flags().StringVar(&message, "message", "", "request for the Team leader")
	_ = cmd.MarkFlagRequired("message")
	return cmd
}

func newCmdTeamRunGet(f *cmdutil.Factory) *cobra.Command {
	return &cobra.Command{Use: "get <run-id>", Short: "Get a Team run snapshot", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, schema.MethodTeamRunGet, map[string]any{"run_id": args[0]})
	}}
}

func newCmdTeamRunReview(f *cmdutil.Factory) *cobra.Command {
	var revision int64
	var tasksJSON string
	cmd := &cobra.Command{Use: "review <run-id>", Short: "Apply human Team task assignments", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		var tasks []any
		if err := json.Unmarshal([]byte(tasksJSON), &tasks); err != nil {
			return fmt.Errorf("invalid tasks JSON: %w", err)
		}
		return callAndPrint(cmd, f, schema.MethodTeamRunReview, map[string]any{"run_id": args[0], "revision": revision, "tasks": tasks})
	}}
	cmd.Flags().Int64Var(&revision, "revision", 0, "run revision")
	cmd.Flags().StringVar(&tasksJSON, "tasks", "[]", "JSON array of reviewed task assignments")
	_ = cmd.MarkFlagRequired("revision")
	return cmd
}

func newCmdTeamRunConfirm(f *cmdutil.Factory) *cobra.Command {
	var revision int64
	var yes bool
	cmd := &cobra.Command{Use: "confirm <run-id>", Short: "Confirm and execute a reviewed Team run", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if err := requireYes(yes, "Team run confirm"); err != nil {
			return err
		}
		return callAndPrint(cmd, f, schema.MethodTeamRunConfirm, map[string]any{"run_id": args[0], "revision": revision})
	}}
	cmd.Flags().Int64Var(&revision, "revision", 0, "run revision")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm execution")
	_ = cmd.MarkFlagRequired("revision")
	return cmd
}

func newCmdTeamTask(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "task", Short: "Update Team tasks"}
	cmd.AddCommand(newCmdTeamTaskUpdate(f))
	return cmd
}

func newCmdTeamTaskUpdate(f *cmdutil.Factory) *cobra.Command {
	var credential, teamID, runID, memberID, state, result, errorCode string
	cmd := &cobra.Command{Use: "update <task-id>", Short: "Update an assigned Team task", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		params := map[string]any{"credential": credential, "task_id": args[0], "team_id": teamID, "run_id": runID, "member_id": memberID, "state": state, "result": result, "error_code": errorCode}
		return callAndPrint(cmd, f, schema.MethodTeamToolTaskUpdate, params)
	}}
	cmd.Flags().StringVar(&credential, "credential", "", "scoped Team tool credential")
	cmd.Flags().StringVar(&teamID, "team-id", "", "Team identifier")
	cmd.Flags().StringVar(&runID, "run-id", "", "run identifier")
	cmd.Flags().StringVar(&memberID, "member-id", "", "assigned member identifier")
	cmd.Flags().StringVar(&state, "state", "", "queued, running, succeeded, failed, or canceled")
	cmd.Flags().StringVar(&result, "result", "", "task result")
	cmd.Flags().StringVar(&errorCode, "error-code", "", "terminal error code")
	for _, name := range []string{"credential", "team-id", "run-id", "member-id", "state"} {
		_ = cmd.MarkFlagRequired(name)
	}
	return cmd
}

func newCmdTeamMailbox(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "mailbox", Short: "Exchange Team mailbox messages"}
	cmd.AddCommand(newCmdTeamMailboxSend(f))
	cmd.AddCommand(newCmdTeamMailboxRead(f))
	return cmd
}

func newCmdTeamMailboxSend(f *cmdutil.Factory) *cobra.Command {
	var credential, teamID, senderID, recipientID, messageType, body, idempotency, taskID string
	cmd := &cobra.Command{Use: "send <run-id>", Short: "Send a mailbox message", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, schema.MethodTeamToolMailboxSend, map[string]any{"credential": credential, "team_id": teamID, "run_id": args[0], "task_id": taskID, "sender_member_id": senderID, "recipient_member_id": recipientID, "message_type": messageType, "body": body, "idempotency_key": idempotency})
	}}
	cmd.Flags().StringVar(&credential, "credential", "", "scoped Team tool credential")
	cmd.Flags().StringVar(&teamID, "team-id", "", "Team identifier")
	cmd.Flags().StringVar(&taskID, "task-id", "", "optional task identifier")
	cmd.Flags().StringVar(&senderID, "sender", "", "sender member identifier")
	cmd.Flags().StringVar(&recipientID, "recipient", "", "recipient member identifier")
	cmd.Flags().StringVar(&messageType, "type", "note", "message type")
	cmd.Flags().StringVar(&body, "body", "", "message body")
	cmd.Flags().StringVar(&idempotency, "idempotency-key", "", "idempotency key")
	for _, name := range []string{"credential", "team-id", "sender", "recipient", "body", "idempotency-key"} {
		_ = cmd.MarkFlagRequired(name)
	}
	return cmd
}

func newCmdTeamMailboxRead(f *cmdutil.Factory) *cobra.Command {
	var credential, teamID, memberID string
	var ack bool
	cmd := &cobra.Command{Use: "read <run-id>", Short: "Read a mailbox", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, schema.MethodTeamToolMailboxRead, map[string]any{"credential": credential, "team_id": teamID, "run_id": args[0], "recipient_member_id": memberID, "ack": ack})
	}}
	cmd.Flags().StringVar(&credential, "credential", "", "scoped Team tool credential")
	cmd.Flags().StringVar(&teamID, "team-id", "", "Team identifier")
	cmd.Flags().StringVar(&memberID, "member", "", "recipient member identifier")
	cmd.Flags().BoolVar(&ack, "ack", false, "acknowledge messages")
	for _, name := range []string{"credential", "team-id", "member"} {
		_ = cmd.MarkFlagRequired(name)
	}
	return cmd
}

func newCmdTeamTool(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "tool", Short: "Use scoped Team tools through Engine"}
	cmd.AddCommand(newCmdTeamToolTasks(f))
	return cmd
}

func newCmdTeamToolTasks(f *cmdutil.Factory) *cobra.Command {
	var credential, teamID, runID, memberID string
	cmd := &cobra.Command{Use: "tasks", Short: "List tasks owned by a Team member", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		return callAndPrint(cmd, f, schema.MethodTeamToolTasks, map[string]any{"credential": credential, "team_id": teamID, "run_id": runID, "member_id": memberID})
	}}
	cmd.Flags().StringVar(&credential, "credential", "", "scoped Team tool credential")
	cmd.Flags().StringVar(&teamID, "team-id", "", "Team identifier")
	cmd.Flags().StringVar(&runID, "run-id", "", "run identifier")
	cmd.Flags().StringVar(&memberID, "member-id", "", "authenticated member identifier")
	for _, name := range []string{"credential", "team-id", "run-id", "member-id"} {
		_ = cmd.MarkFlagRequired(name)
	}
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
