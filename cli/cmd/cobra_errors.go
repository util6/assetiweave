package cmd

import (
	"errors"
	"fmt"
	"sort"
	"strings"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdmeta"
	"github.com/util6/assetiweave/internal/suggest"
)

func installCobraValidation(root *cobra.Command) {
	if root == nil {
		return
	}
	root.SetFlagErrorFunc(cobraFlagError)
	walkCobraValidation(root)
}

func walkCobraValidation(command *cobra.Command) {
	if command.HasSubCommands() && command.Run == nil && command.RunE == nil {
		cmdmeta.MarkPureGroup(command)
		command.Args = cobra.ArbitraryArgs
		command.RunE = unknownSubcommandRunE
	}
	if cmdmeta.IsAction(command) {
		wrapArgumentValidation(command)
		wrapRequiredFlagValidation(command)
	}
	for _, child := range command.Commands() {
		walkCobraValidation(child)
	}
}

func unknownSubcommandRunE(command *cobra.Command, args []string) error {
	if len(args) == 0 {
		return command.Help()
	}
	unknown := args[0]
	available := availableSubcommandNames(command)
	suggestions := suggest.Closest(unknown, available, 3)
	hint := fmt.Sprintf("run `%s --help` to list available subcommands", command.CommandPath())
	if len(suggestions) > 0 {
		hint = fmt.Sprintf(
			"did you mean %s? (run `%s --help` for all subcommands)",
			strings.Join(suggestions, ", "),
			command.CommandPath(),
		)
	}
	return errs.NewValidationError(
		errs.SubtypeUnknownCommand,
		"unknown command %q for %q",
		unknown,
		command.CommandPath(),
	).
		WithHint(hint).
		WithDetails(map[string]any{
			"unknown":      unknown,
			"command_path": command.CommandPath(),
			"suggestions":  suggestions,
			"available":    available,
		})
}

func cobraFlagError(command *cobra.Command, flagErr error) error {
	var missing *pflag.NotExistError
	if !errors.As(flagErr, &missing) || !strings.HasPrefix(flagErr.Error(), "unknown ") {
		return errs.NewValidationError(errs.SubtypeFlagError, "%s", flagErr).
			WithHint("run `%s --help` for valid flags", command.CommandPath()).
			WithDetails(map[string]any{
				"command_path": command.CommandPath(),
			}).
			WithCause(flagErr)
	}

	name := missing.GetSpecifiedName()
	token := "--" + name
	if missing.GetSpecifiedShortnames() != "" {
		token = "-" + name
	}
	if cmdmeta.IsPureGroup(command) {
		if subcommands := subcommandsDefiningFlag(command, name); len(subcommands) > 0 {
			return errs.NewValidationError(
				errs.SubtypeMissingSubcommand,
				"missing subcommand for %q; flag %s belongs to a subcommand",
				command.CommandPath(),
				token,
			).
				WithHint("run `%s --help` to choose a subcommand", command.CommandPath()).
				WithDetails(map[string]any{
					"flag":         token,
					"command_path": command.CommandPath(),
					"subcommands":  subcommands,
				}).
				WithCause(flagErr)
		}
	}

	valid := visibleFlagNames(command)
	suggestions := suggest.Closest(name, trimFlagPrefixes(valid), 3)
	for index := range suggestions {
		suggestions[index] = "--" + suggestions[index]
	}
	hint := fmt.Sprintf("run `%s --help` to list valid flags", command.CommandPath())
	if len(suggestions) > 0 {
		hint = fmt.Sprintf(
			"did you mean %s? (run `%s --help` for all flags)",
			strings.Join(suggestions, ", "),
			command.CommandPath(),
		)
	}
	return errs.NewValidationError(
		errs.SubtypeUnknownFlag,
		"unknown flag %q for %q",
		token,
		command.CommandPath(),
	).
		WithHint(hint).
		WithDetails(map[string]any{
			"unknown":      token,
			"command_path": command.CommandPath(),
			"suggestions":  suggestions,
			"valid_flags":  valid,
		}).
		WithCause(flagErr)
}

func wrapArgumentValidation(command *cobra.Command) {
	original := command.Args
	if original == nil {
		return
	}
	command.Args = func(current *cobra.Command, args []string) error {
		if err := original(current, args); err != nil {
			if _, typed := errs.ProblemOf(err); typed {
				return err
			}
			return errs.NewValidationError(errs.SubtypeInvalidArgument, "%s", err).
				WithHint("run `%s --help` for usage", current.CommandPath()).
				WithDetails(map[string]any{
					"command_path": current.CommandPath(),
					"arguments":    append([]string(nil), args...),
				}).
				WithCause(err)
		}
		return nil
	}
}

func wrapRequiredFlagValidation(command *cobra.Command) {
	originalE := command.PreRunE
	original := command.PreRun
	command.PreRun = nil
	command.PreRunE = func(current *cobra.Command, args []string) error {
		if missing := missingRequiredFlagNames(current); len(missing) > 0 {
			display := make([]string, len(missing))
			for index, name := range missing {
				display[index] = "--" + name
			}
			return errs.NewValidationError(
				errs.SubtypeMissingRequiredFlag,
				"required flag(s) %s not set",
				strings.Join(display, ", "),
			).
				WithHint("provide the required flags; run `%s --help` for usage", current.CommandPath()).
				WithDetails(map[string]any{
					"command_path": current.CommandPath(),
					"flags":        missing,
				})
		}
		if err := current.ValidateFlagGroups(); err != nil {
			return errs.NewValidationError(errs.SubtypeFlagError, "%s", err).
				WithHint("run `%s --help` for valid flag combinations", current.CommandPath()).
				WithDetails(map[string]any{
					"command_path": current.CommandPath(),
				}).
				WithCause(err)
		}
		if originalE != nil {
			return originalE(current, args)
		}
		if original != nil {
			original(current, args)
		}
		return nil
	}
}

func missingRequiredFlagNames(command *cobra.Command) []string {
	var missing []string
	command.Flags().VisitAll(func(flag *pflag.Flag) {
		required, ok := flag.Annotations[cobra.BashCompOneRequiredFlag]
		if ok && len(required) > 0 && required[0] == "true" && !flag.Changed {
			missing = append(missing, flag.Name)
		}
	})
	sort.Strings(missing)
	return missing
}

func availableSubcommandNames(command *cobra.Command) []string {
	available := make([]string, 0, len(command.Commands()))
	for _, child := range command.Commands() {
		if child.Hidden || !child.IsAvailableCommand() || child.Name() == "help" {
			continue
		}
		available = append(available, child.Name())
	}
	sort.Strings(available)
	return available
}

func visibleFlagNames(command *cobra.Command) []string {
	names := make([]string, 0, command.Flags().NFlag())
	command.Flags().VisitAll(func(flag *pflag.Flag) {
		if !flag.Hidden {
			names = append(names, "--"+flag.Name)
		}
	})
	sort.Strings(names)
	return names
}

func trimFlagPrefixes(flags []string) []string {
	names := make([]string, len(flags))
	for index, flag := range flags {
		names[index] = strings.TrimLeft(flag, "-")
	}
	return names
}

func subcommandsDefiningFlag(command *cobra.Command, name string) []string {
	var subcommands []string
	for _, child := range command.Commands() {
		if child.Hidden || !child.IsAvailableCommand() {
			continue
		}
		if child.Flags().Lookup(name) != nil {
			subcommands = append(subcommands, child.Name())
		}
	}
	sort.Strings(subcommands)
	return subcommands
}
