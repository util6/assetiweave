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
		if len(name) > 2 {
			switch name[:2] {
			case "-E", "-C", "-P":
				value = name[2:]
				name = name[:2]
				hasInlineValue = true
			}
		}
		switch name {
		case "--engine", "--plugin-config", "--policy", "-E", "-C", "-P":
			if !hasInlineValue {
				if index+1 >= len(args) || args[index+1] == "--" {
					return bootstrapOptions{}, errs.NewValidationError(errs.SubtypeInvalidArgument, "%s requires a value", name).
						WithCode("validation")
				}
				index++
				value = args[index]
			}
			switch name {
			case "--engine", "-E":
				options.EnginePath = value
			case "--plugin-config", "-C":
				options.PluginConfigPath = value
			case "--policy", "-P":
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
