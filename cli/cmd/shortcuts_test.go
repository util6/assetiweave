package cmd

import (
	"bytes"
	"context"
	"strings"
	"testing"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
	"github.com/util6/assetiweave/internal/cmdutil"
)

func TestCommonCommandAliasesResolveToCanonicalCommands(t *testing.T) {
	root := buildShortcutTestRoot()
	tests := []struct {
		args []string
		want string
	}{
		{args: []string{"ov"}, want: "assetiweave-cli overview"},
		{args: []string{"src", "ls"}, want: "assetiweave-cli source list"},
		{args: []string{"sk", "acq"}, want: "assetiweave-cli skill acquire"},
		{args: []string{"c", "ses", "g"}, want: "assetiweave-cli conversation session get"},
		{args: []string{"c", "ad", "up"}, want: "assetiweave-cli conversation adapter upgrade"},
		{args: []string{"m", "rec", "s"}, want: "assetiweave-cli memory recall search"},
		{args: []string{"hv", "tpl", "ls"}, want: "assetiweave-cli harvester template list"},
	}

	for _, test := range tests {
		command, _, err := root.Find(test.args)
		if err != nil {
			t.Fatalf("Find(%v) error = %v", test.args, err)
		}
		if command.CommandPath() != test.want {
			t.Fatalf("Find(%v) path = %q, want %q", test.args, command.CommandPath(), test.want)
		}
	}
}

func TestHandwrittenCommandsExposeAliases(t *testing.T) {
	root := buildShortcutTestRoot()
	walkCommands(root, func(command *cobra.Command) {
		if command == root || command.Name() == "help" || (command.Parent() != nil && command.Parent().Name() == "app") {
			return
		}
		if len(command.Aliases) == 0 {
			t.Errorf("%s has no command alias", command.CommandPath())
		}
	})
}

func TestVisibleFlagsHaveUniqueShorthands(t *testing.T) {
	root := buildShortcutTestRoot()
	walkCommands(root, func(command *cobra.Command) {
		seen := map[string]string{}
		command.InitDefaultHelpFlag()
		for _, flags := range []*pflag.FlagSet{command.LocalFlags(), command.InheritedFlags()} {
			flags.VisitAll(func(flag *pflag.Flag) {
				if flag.Hidden {
					return
				}
				if flag.Shorthand == "" {
					t.Errorf("%s --%s has no shorthand", command.CommandPath(), flag.Name)
					return
				}
				if previous, exists := seen[flag.Shorthand]; exists {
					t.Errorf("%s -%s is shared by --%s and --%s", command.CommandPath(), flag.Shorthand, previous, flag.Name)
				}
				seen[flag.Shorthand] = flag.Name
			})
		}
	})
}

func TestCommonFlagShorthandsAreStable(t *testing.T) {
	root := buildShortcutTestRoot()
	tests := []struct {
		path      []string
		shorthand string
		want      string
	}{
		{path: []string{"source", "add"}, shorthand: "n", want: "name"},
		{path: []string{"source", "add"}, shorthand: "p", want: "path"},
		{path: []string{"source", "add"}, shorthand: "d", want: "dry-run"},
		{path: []string{"skill", "search"}, shorthand: "q", want: "query"},
		{path: []string{"skill", "mount"}, shorthand: "p", want: "profile"},
		{path: []string{"skill", "delete"}, shorthand: "y", want: "yes"},
		{path: []string{"conversation", "search"}, shorthand: "l", want: "limit"},
		{path: []string{"conversation", "adapter", "upgrade"}, shorthand: "d", want: "developer"},
		{path: []string{"conversation", "adapter", "upgrade"}, shorthand: "r", want: "dry-run"},
	}

	for _, test := range tests {
		command, _, err := root.Find(test.path)
		if err != nil {
			t.Fatalf("Find(%v) error = %v", test.path, err)
		}
		flag := command.Flags().Lookup(test.want)
		if flag == nil || flag.Shorthand != test.shorthand {
			t.Fatalf("%s --%s = %#v, want -%s", command.CommandPath(), test.want, flag, test.shorthand)
		}
		if command.Flags().ShorthandLookup(test.shorthand) == nil {
			t.Fatalf("%s -%s is not registered", command.CommandPath(), test.shorthand)
		}
	}
}

func TestAliasesAndShortFlagsExecuteCanonicalCommand(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(
		t,
		client,
		"src", "a",
		"-n", "LocalSkills",
		"-p", "/tmp/skills",
		"-d",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "source.add" {
		t.Fatalf("method = %q, want source.add", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["name"] != "LocalSkills" || params["root_path"] != "/tmp/skills" || params["dry_run"] != true {
		t.Fatalf("params = %#v", params)
	}
}

func TestCompletionAliasUsesCompletionBootstrapPath(t *testing.T) {
	if !isCompletionCommandArgs([]string{"comp", "zsh"}) {
		t.Fatal("completion alias should skip normal runtime bootstrap")
	}
}

func TestCompletionRegistersCanonicalAndShortExecutableNames(t *testing.T) {
	stdout := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"comp", "zsh"})
	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	output := stdout.String()
	if !strings.Contains(output, "#compdef assetiweave-cli") || !strings.Contains(output, "#compdef aiwc") {
		t.Fatalf("completion script does not register both executable names:\n%s", output)
	}
}

func TestGeneratedAppRequiredFlagAcceptsShorthand(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "ap", "create-profile", "-i", `{}`)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "create_profile" {
		t.Fatalf("method = %q, want create_profile", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if _, ok := params["input"].(map[string]any); !ok {
		t.Fatalf("input = %#v, want decoded JSON object", params["input"])
	}
}

func buildShortcutTestRoot() *cobra.Command {
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	}
	root, _ := buildInternalWithOptions(context.Background(), factory, buildOptions{SkipRuntime: true})
	return root
}

func walkCommands(command *cobra.Command, visit func(*cobra.Command)) {
	visit(command)
	for _, child := range command.Commands() {
		walkCommands(child, visit)
	}
}
