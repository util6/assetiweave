package cmd

import (
	"encoding/json"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	engineschema "github.com/util6/assetiweave/internal/schema"
)

type generatedFlag struct {
	paramName   string
	flagName    string
	kind        string
	stringValue string
	boolValue   bool
	intValue    int
}

func newCmdApp(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "app",
		Short: "Call typed App operations generated from the Rust command contract",
	}
	for _, spec := range engineschema.AppCommands() {
		cmd.AddCommand(newCmdGeneratedAppMethod(f, spec))
	}
	return cmd
}

func newCmdGeneratedAppMethod(f *cmdutil.Factory, spec engineschema.CommandSpec) *cobra.Command {
	name := generatedCommandName(spec.Method)
	aliases := []string(nil)
	if name != spec.Method {
		aliases = []string{spec.Method}
	}
	cmd := &cobra.Command{
		Use:     name,
		Aliases: aliases,
		Short:   spec.Description,
		Args:    cobra.NoArgs,
	}
	annotateEngineCommand(cmd, spec, "app")

	propertyNames := make([]string, 0, len(spec.ParamsSchema.Properties))
	for propertyName := range spec.ParamsSchema.Properties {
		propertyNames = append(propertyNames, propertyName)
	}
	sort.Strings(propertyNames)

	bindings := make([]*generatedFlag, 0, len(propertyNames))
	for _, propertyName := range propertyNames {
		property := spec.ParamsSchema.Properties[propertyName]
		binding := &generatedFlag{
			paramName: propertyName,
			flagName:  strings.ReplaceAll(propertyName, "_", "-"),
			kind:      property.BaseType(),
		}
		bindings = append(bindings, binding)
		registerGeneratedFlag(cmd, binding, property)
		if spec.ParamsSchema.IsRequired(propertyName) {
			_ = cmd.MarkFlagRequired(binding.flagName)
		}
	}

	cmd.RunE = func(cmd *cobra.Command, _ []string) error {
		params, err := generatedParams(cmd, f, bindings)
		if err != nil {
			return err
		}
		if spec.ConfirmationRequired {
			yes, _ := params["yes"].(bool)
			if err := requireYes(yes, "app "+spec.Method); err != nil {
				return err
			}
		}
		return callAndPrint(cmd, f, spec.Method, params)
	}
	return cmd
}

func registerGeneratedFlag(cmd *cobra.Command, binding *generatedFlag, property engineschema.PropertySchema) {
	description := property.Description
	switch binding.kind {
	case "boolean":
		cmd.Flags().BoolVar(&binding.boolValue, binding.flagName, false, description)
	case "integer":
		cmd.Flags().IntVar(&binding.intValue, binding.flagName, 0, description)
	case "object", "array":
		cmd.Flags().StringVar(&binding.stringValue, binding.flagName, "", description+" as JSON, @file, or - for stdin")
	default:
		cmd.Flags().StringVar(&binding.stringValue, binding.flagName, "", description)
	}
	if len(property.Enum) > 0 {
		values := append([]string(nil), property.Enum...)
		_ = cmd.RegisterFlagCompletionFunc(binding.flagName, func(_ *cobra.Command, _ []string, _ string) ([]string, cobra.ShellCompDirective) {
			return values, cobra.ShellCompDirectiveNoFileComp
		})
	}
}

func generatedParams(cmd *cobra.Command, f *cmdutil.Factory, bindings []*generatedFlag) (map[string]any, error) {
	params := make(map[string]any)
	stdinUsed := false
	for _, binding := range bindings {
		if !cmd.Flags().Changed(binding.flagName) {
			continue
		}
		switch binding.kind {
		case "boolean":
			params[binding.paramName] = binding.boolValue
		case "integer":
			params[binding.paramName] = binding.intValue
		case "object", "array":
			value, err := readGeneratedJSONValue(f, binding.stringValue, binding.paramName, binding.kind, &stdinUsed)
			if err != nil {
				return nil, err
			}
			params[binding.paramName] = value
		default:
			params[binding.paramName] = binding.stringValue
		}
	}
	return params, nil
}

func readGeneratedJSONValue(f *cmdutil.Factory, value, paramName, expectedKind string, stdinUsed *bool) (any, error) {
	var bytes []byte
	var err error
	switch {
	case value == "-":
		if *stdinUsed {
			return nil, errs.NewValidationError(errs.SubtypeInvalidArgument, "only one generated parameter can read from stdin").
				WithCode("validation")
		}
		*stdinUsed = true
		bytes, err = io.ReadAll(f.IOStreams.In)
	case strings.HasPrefix(value, "@"):
		bytes, err = os.ReadFile(strings.TrimPrefix(value, "@"))
	default:
		bytes = []byte(value)
	}
	if err != nil {
		return nil, errs.NewValidationError(errs.SubtypeInvalidArgument, "failed to read --%s JSON: %v", strings.ReplaceAll(paramName, "_", "-"), err).
			WithCode("validation").
			WithCause(err)
	}

	var decoded any
	if err := json.Unmarshal(bytes, &decoded); err != nil {
		return nil, errs.NewValidationError(errs.SubtypeInvalidJSON, "--%s: %v", strings.ReplaceAll(paramName, "_", "-"), err).
			WithCode("invalid_json").
			WithHint("pass valid JSON, @file, or - for stdin").
			WithCause(err)
	}
	switch expectedKind {
	case "object":
		if _, ok := decoded.(map[string]any); !ok {
			return nil, errs.NewValidationError(errs.SubtypeInvalidJSON, "--%s must be a JSON object", strings.ReplaceAll(paramName, "_", "-")).
				WithCode("invalid_json")
		}
	case "array":
		if _, ok := decoded.([]any); !ok {
			return nil, errs.NewValidationError(errs.SubtypeInvalidJSON, "--%s must be a JSON array", strings.ReplaceAll(paramName, "_", "-")).
				WithCode("invalid_json")
		}
	}
	return decoded, nil
}

func generatedCommandName(method string) string {
	return strings.NewReplacer("_", "-", ".", "-").Replace(method)
}
