package cmd

import (
	"context"
	"testing"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
	"github.com/util6/assetiweave/internal/cmdutil"
	engineschema "github.com/util6/assetiweave/internal/schema"
)

func TestRunnableCommandTreeHasRiskMetadata(t *testing.T) {
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: testPluginFactory(&recordingClient{}).IOStreams,
		Client:    &recordingClient{},
	})
	var missing []string
	walkCommands(root, func(command *cobra.Command) {
		if command.HasParent() && command.Runnable() {
			if _, ok := cmdmeta.View(command).Risk(); !ok {
				missing = append(missing, cmdmeta.Path(command))
			}
		}
	})
	if len(missing) > 0 {
		t.Fatalf("runnable commands missing risk metadata: %v", missing)
	}
}

func TestFriendlyCommandRiskComesFromEngineContract(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	for _, spec := range engineschema.MustContract().Commands {
		if spec.CLI == nil {
			continue
		}
		command := findCommand(root, commandPathFromCLI(*spec.CLI))
		if command == nil {
			t.Fatalf("contract CLI path %q not found", *spec.CLI)
		}
		risk, ok := cmdmeta.View(command).Risk()
		if !ok || risk != platform.Risk(spec.Risk) {
			t.Fatalf("%s risk = %q/%v, want %q", *spec.CLI, risk, ok, spec.Risk)
		}
	}
}

func walkCommands(root *cobra.Command, visit func(*cobra.Command)) {
	visit(root)
	for _, child := range root.Commands() {
		walkCommands(child, visit)
	}
}
