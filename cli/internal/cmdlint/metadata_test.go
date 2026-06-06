package cmdlint

import (
	"reflect"
	"testing"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
)

func TestCheckMetadataReportsMissingDirectAnnotations(t *testing.T) {
	root := &cobra.Command{Use: "test"}
	root.AddCommand(&cobra.Command{Use: "run", RunE: func(*cobra.Command, []string) error { return nil }})

	violations := CheckMetadata(root)

	got := summarizeFields(violations)
	want := map[string]int{
		"domain":     1,
		"risk":       1,
		"identities": 1,
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("violations = %#v, want fields %#v", violations, want)
	}
}

func TestCheckMetadataRequiresDirectAnnotationsNotInheritedFallback(t *testing.T) {
	root := &cobra.Command{
		Use: "test",
		Annotations: map[string]string{
			cmdmeta.AnnotationDomain:     "parent",
			cmdmeta.AnnotationRisk:       platform.RiskRead.String(),
			cmdmeta.AnnotationIdentities: platform.IdentityAgent.String(),
		},
	}
	root.AddCommand(&cobra.Command{Use: "run", RunE: func(*cobra.Command, []string) error { return nil }})

	violations := CheckMetadata(root)

	got := summarizeFields(violations)
	want := map[string]int{
		"domain":     1,
		"risk":       1,
		"identities": 1,
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("violations = %#v, want direct annotation fields %#v", violations, want)
	}
}

func TestCheckMetadataReportsInvalidRiskAndIdentity(t *testing.T) {
	root := &cobra.Command{Use: "test"}
	run := &cobra.Command{
		Use:  "run",
		RunE: func(*cobra.Command, []string) error { return nil },
		Annotations: map[string]string{
			cmdmeta.AnnotationDomain:     "system",
			cmdmeta.AnnotationRisk:       "dangerous",
			cmdmeta.AnnotationIdentities: "agent,robot",
		},
	}
	root.AddCommand(run)

	violations := CheckMetadata(root)

	got := summarizeFields(violations)
	want := map[string]int{
		"risk":       1,
		"identities": 1,
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("violations = %#v, want fields %#v", violations, want)
	}
}

func TestCheckMetadataIgnoresNonRunnableParents(t *testing.T) {
	root := &cobra.Command{Use: "test"}
	parent := &cobra.Command{Use: "group"}
	child := &cobra.Command{Use: "run", RunE: func(*cobra.Command, []string) error { return nil }}
	cmdmeta.Set(child, cmdmeta.Metadata{
		Domain:     "system",
		Risk:       platform.RiskRead,
		Identities: []platform.Identity{platform.IdentityAgent},
	})
	parent.AddCommand(child)
	root.AddCommand(parent)

	if violations := CheckMetadata(root); len(violations) != 0 {
		t.Fatalf("violations = %#v, want none", violations)
	}
}

func TestCheckMetadataIgnoresMarkedPureGroups(t *testing.T) {
	root := &cobra.Command{Use: "test"}
	parent := &cobra.Command{
		Use:  "group",
		RunE: func(*cobra.Command, []string) error { return nil },
	}
	cmdmeta.MarkPureGroup(parent)
	child := &cobra.Command{Use: "run", RunE: func(*cobra.Command, []string) error { return nil }}
	cmdmeta.Set(child, cmdmeta.Metadata{
		Domain:     "system",
		Risk:       platform.RiskRead,
		Identities: []platform.Identity{platform.IdentityAgent},
	})
	parent.AddCommand(child)
	root.AddCommand(parent)

	if violations := CheckMetadata(root); len(violations) != 0 {
		t.Fatalf("violations = %#v, want none", violations)
	}
}

func summarizeFields(violations []Violation) map[string]int {
	counts := map[string]int{}
	for _, violation := range violations {
		counts[violation.Field]++
	}
	return counts
}
