package cmd

import (
	"strings"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
)

type bootstrapOptions struct {
	EnginePath       string
	PluginConfigPath string
	PolicyPath       string
}

func parseBootstrapOptions(args []string) (bootstrapOptions, error) {
	var options bootstrapOptions
	for index := 0; index < len(args); index++ {
		arg := args[index]
		if arg == "--" {
			break
		}
		name, value, hasInlineValue := strings.Cut(arg, "=")
		switch name {
		case "--engine", "--plugin-config", "--policy":
			if !hasInlineValue {
				if index+1 >= len(args) || args[index+1] == "--" {
					return bootstrapOptions{}, errs.NewValidationError(errs.SubtypeInvalidArgument, "%s requires a value", name).
						WithCode("validation")
				}
				index++
				value = args[index]
			}
			switch name {
			case "--engine":
				options.EnginePath = value
			case "--plugin-config":
				options.PluginConfigPath = value
			case "--policy":
				options.PolicyPath = value
			}
		}
	}
	return options, nil
}

func applyBootstrapOptionsToFactory(f *cmdutil.Factory, options bootstrapOptions) {
	if f == nil {
		return
	}
	if options.EnginePath != "" {
		f.EnginePath = options.EnginePath
	}
	if options.PluginConfigPath != "" {
		f.PluginConfigPath = options.PluginConfigPath
	}
	if options.PolicyPath != "" {
		f.PolicyPath = options.PolicyPath
	}
}
