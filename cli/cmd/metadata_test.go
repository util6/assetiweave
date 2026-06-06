package cmd

import (
	"context"
	"strings"
	"testing"

	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdlint"
	"github.com/util6/assetiweave/internal/cmdmeta"
	"github.com/util6/assetiweave/internal/cmdutil"
	engineschema "github.com/util6/assetiweave/internal/schema"
)

func TestRunnableCommandTreePassesMetadataLint(t *testing.T) {
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: testPluginFactory(&recordingClient{}).IOStreams,
		Client:    &recordingClient{},
	})
	if violations := cmdlint.CheckMetadata(root); len(violations) > 0 {
		t.Fatalf("metadata lint violations: %v", violations)
	}
}

func TestProfileCommandVisibilityPolicy(t *testing.T) {
	visibleRoot := Build(context.Background(), testPluginFactory(&recordingClient{}))
	visibleProfile := findCommand(visibleRoot, []string{"profile"})
	if visibleProfile == nil {
		t.Fatal("profile command not found")
	}
	if visibleProfile.Hidden {
		t.Fatal("profile command should be visible by default")
	}

	hiddenRoot, _ := buildInternalWithOptions(context.Background(), testPluginFactory(&recordingClient{}), buildOptions{HideProfiles: true})
	hiddenProfile := findCommand(hiddenRoot, []string{"profile"})
	if hiddenProfile == nil {
		t.Fatal("profile command not found with HideProfiles")
	}
	if !hiddenProfile.Hidden {
		t.Fatal("profile command should be hidden with HideProfiles")
	}
}

func TestHiddenProfileCommandRemainsExecutable(t *testing.T) {
	client := &recordingClient{}
	factory := testPluginFactory(client)
	root, _ := buildInternalWithOptions(context.Background(), factory, buildOptions{HideProfiles: true})
	root.SetArgs([]string{"profile", "list"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "profile.list" {
		t.Fatalf("method = %q, want profile.list", client.method)
	}
}

func TestFriendlyCommandRiskComesFromEngineContract(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	specsByPath := map[string][]engineschema.CommandSpec{}
	for _, spec := range engineschema.MustContract().Commands {
		if spec.CLI == nil {
			continue
		}
		path := commandPathFromCLI(*spec.CLI)
		specsByPath[strings.Join(path, "/")] = append(specsByPath[strings.Join(path, "/")], spec)
	}
	for path, specs := range specsByPath {
		command := findCommand(root, strings.Split(path, "/"))
		if command == nil {
			t.Fatalf("contract CLI path %q not found", path)
		}
		risk, ok := cmdmeta.View(command).Risk()
		wantRisk := platform.Risk(specs[0].Risk)
		for _, spec := range specs[1:] {
			if platform.Risk(spec.Risk) != wantRisk {
				t.Fatalf("shared CLI path %q has mixed risks: %v", path, commandMethods(specs))
			}
		}
		if !ok || risk != wantRisk {
			t.Fatalf("%s risk = %q/%v, want %q", path, risk, ok, wantRisk)
		}
		method, hasMethod := command.Annotations[cmdmeta.AnnotationMethod]
		if len(specs) == 1 {
			if !hasMethod || method != specs[0].Method {
				t.Fatalf("%s method = %q/%v, want %q", path, method, hasMethod, specs[0].Method)
			}
		} else if hasMethod {
			t.Fatalf("%s method = %q, want no single method annotation for shared path %v", path, method, commandMethods(specs))
		}
	}
}

func commandMethods(specs []engineschema.CommandSpec) []string {
	methods := make([]string, 0, len(specs))
	for _, spec := range specs {
		methods = append(methods, spec.Method)
	}
	return methods
}
