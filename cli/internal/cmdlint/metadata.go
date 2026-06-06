package cmdlint

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
)

type Violation struct {
	Rule    string
	Path    string
	Field   string
	Message string
}

func (v Violation) String() string {
	if v.Path == "" {
		return fmt.Sprintf("%s: %s", v.Field, v.Message)
	}
	return fmt.Sprintf("%s %s: %s", v.Path, v.Field, v.Message)
}

func CheckMetadata(root *cobra.Command) []Violation {
	var violations []Violation
	walk(root, func(command *cobra.Command) {
		if command == nil || !command.HasParent() || !cmdmeta.IsAction(command) {
			return
		}
		path := cmdmeta.Path(command)
		annotations := command.Annotations
		if annotations == nil {
			annotations = map[string]string{}
		}
		if annotations[cmdmeta.AnnotationDomain] == "" {
			violations = append(violations, metadataViolation(path, "domain", "runnable command must declare a direct domain annotation"))
		}
		checkRisk(path, annotations[cmdmeta.AnnotationRisk], &violations)
		checkIdentities(path, annotations[cmdmeta.AnnotationIdentities], &violations)
	})
	return violations
}

func checkRisk(path, value string, violations *[]Violation) {
	if value == "" {
		*violations = append(*violations, metadataViolation(path, "risk", "runnable command must declare a direct risk annotation"))
		return
	}
	if _, err := platform.ParseRisk(value); err != nil {
		*violations = append(*violations, metadataViolation(path, "risk", err.Error()))
	}
}

func checkIdentities(path, value string, violations *[]Violation) {
	if value == "" {
		*violations = append(*violations, metadataViolation(path, "identities", "runnable command must declare direct supported identities"))
		return
	}
	for _, part := range strings.Split(value, ",") {
		part = strings.TrimSpace(part)
		if part == "" {
			*violations = append(*violations, metadataViolation(path, "identities", "identity annotation contains an empty identity"))
			return
		}
		if _, err := platform.ParseIdentity(part); err != nil {
			*violations = append(*violations, metadataViolation(path, "identities", err.Error()))
			return
		}
	}
}

func metadataViolation(path, field, message string) Violation {
	return Violation{
		Rule:    "command_metadata",
		Path:    path,
		Field:   field,
		Message: message,
	}
}

func walk(root *cobra.Command, visit func(*cobra.Command)) {
	if root == nil {
		return
	}
	visit(root)
	for _, child := range root.Commands() {
		walk(child, visit)
	}
}
