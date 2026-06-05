package cmd

import (
	"strings"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
	engineschema "github.com/util6/assetiweave/internal/schema"
)

var commandIdentities = []platform.Identity{platform.IdentityUser, platform.IdentityAgent}

func annotateCommandTree(root *cobra.Command) {
	for _, spec := range engineschema.MustContract().Commands {
		if spec.CLI == nil {
			continue
		}
		path := commandPathFromCLI(*spec.CLI)
		if len(path) == 0 {
			continue
		}
		command := findCommand(root, path)
		if command != nil {
			annotateEngineCommand(command, spec, firstPathSegment(cmdmeta.Path(command)))
		}
	}
	annotateLocalCommand(root, []string{"api", "call"}, "api", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"completion"}, "system", platform.RiskRead)
	annotateLocalCommand(root, []string{"config", "plugins", "show"}, "system", platform.RiskRead)
}

func annotateEngineCommand(command *cobra.Command, spec engineschema.CommandSpec, domain string) {
	cmdmeta.Set(command, cmdmeta.Metadata{
		Method:     spec.Method,
		Domain:     domain,
		Risk:       platform.Risk(spec.Risk),
		Identities: commandIdentities,
	})
}

func annotateLocalCommand(root *cobra.Command, path []string, domain string, risk platform.Risk) {
	if command := findCommand(root, path); command != nil {
		cmdmeta.Set(command, cmdmeta.Metadata{
			Domain:     domain,
			Risk:       risk,
			Identities: commandIdentities,
		})
	}
}

func commandPathFromCLI(cli string) []string {
	fields := strings.Fields(cli)
	if len(fields) <= 1 {
		return nil
	}
	path := make([]string, 0, len(fields)-1)
	for _, field := range fields[1:] {
		if strings.HasPrefix(field, "-") ||
			strings.HasPrefix(field, "<") ||
			strings.HasPrefix(field, "[") {
			break
		}
		path = append(path, field)
	}
	return path
}

func findCommand(root *cobra.Command, path []string) *cobra.Command {
	current := root
	for _, name := range path {
		var next *cobra.Command
		for _, child := range current.Commands() {
			if child.Name() == name {
				next = child
				break
			}
		}
		if next == nil {
			return nil
		}
		current = next
	}
	return current
}

func firstPathSegment(path string) string {
	domain, _, _ := strings.Cut(path, "/")
	return domain
}
