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
	specsByPath := map[string][]engineschema.CommandSpec{}
	for _, spec := range engineschema.MustContract().Commands {
		if spec.CLI == nil {
			continue
		}
		path := commandPathFromCLI(*spec.CLI)
		if len(path) == 0 {
			continue
		}
		key := strings.Join(path, "/")
		specsByPath[key] = append(specsByPath[key], spec)
	}
	for key, specs := range specsByPath {
		command := findCommand(root, strings.Split(key, "/"))
		if command != nil {
			if len(specs) == 1 {
				annotateEngineCommand(command, specs[0], firstPathSegment(cmdmeta.Path(command)))
			} else {
				annotateSharedEngineCommand(command, specs[0], firstPathSegment(cmdmeta.Path(command)))
			}
		}
	}
	annotateLocalCommand(root, []string{"api", "call"}, "api", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"completion"}, "system", platform.RiskRead)
	annotateLocalCommand(root, []string{"config", "plugins", "show"}, "system", platform.RiskRead)
	annotateLocalCommand(root, []string{"conversation", "web", "auth-check"}, "conversation", platform.RiskRead)
	annotateLocalCommand(root, []string{"conversation", "web", "auth-detect"}, "conversation", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"conversation", "web", "scaffold"}, "conversation", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"conversation", "web", "sync"}, "conversation", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"harvester", "template", "list"}, "conversation", platform.RiskRead)
	annotateLocalCommand(root, []string{"harvester", "install"}, "conversation", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"harvester", "update"}, "conversation", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"harvester", "list"}, "conversation", platform.RiskRead)
	annotateLocalCommand(root, []string{"harvester", "run"}, "conversation", platform.RiskHighRiskWrite)
	annotateLocalCommand(root, []string{"update"}, "system", platform.RiskHighRiskWrite)
}

func annotateEngineCommand(command *cobra.Command, spec engineschema.CommandSpec, domain string) {
	cmdmeta.Set(command, cmdmeta.Metadata{
		Method:     spec.Method,
		Domain:     domain,
		Risk:       platform.Risk(spec.Risk),
		Identities: commandIdentities,
	})
}

func annotateSharedEngineCommand(command *cobra.Command, spec engineschema.CommandSpec, domain string) {
	cmdmeta.Set(command, cmdmeta.Metadata{
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
