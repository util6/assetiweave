package cmdmeta

import (
	"strings"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
)

const (
	AnnotationMethod       = "assetiweave:method"
	AnnotationDomain       = "assetiweave:domain"
	AnnotationRisk         = "assetiweave:risk"
	AnnotationIdentities   = "assetiweave:identities"
	AnnotationDenialLayer  = "assetiweave:policy-denial-layer"
	AnnotationDenialSource = "assetiweave:policy-denial-source"
	AnnotationPureGroup    = "assetiweave:pure-group"
)

type Metadata struct {
	Method     string
	Domain     string
	Risk       platform.Risk
	Identities []platform.Identity
}

func Set(command *cobra.Command, metadata Metadata) {
	if command.Annotations == nil {
		command.Annotations = map[string]string{}
	}
	setAnnotation(command, AnnotationMethod, metadata.Method)
	setAnnotation(command, AnnotationDomain, metadata.Domain)
	setAnnotation(command, AnnotationRisk, metadata.Risk.String())
	if len(metadata.Identities) == 0 {
		delete(command.Annotations, AnnotationIdentities)
		return
	}
	values := make([]string, len(metadata.Identities))
	for index, identity := range metadata.Identities {
		values[index] = identity.String()
	}
	command.Annotations[AnnotationIdentities] = strings.Join(values, ",")
}

func MarkDenied(command *cobra.Command, layer, source string) {
	if command.Annotations == nil {
		command.Annotations = map[string]string{}
	}
	command.Annotations[AnnotationDenialLayer] = layer
	command.Annotations[AnnotationDenialSource] = source
	delete(command.Annotations, AnnotationPureGroup)
}

func MarkPureGroup(command *cobra.Command) {
	if command == nil {
		return
	}
	if command.Annotations == nil {
		command.Annotations = map[string]string{}
	}
	command.Annotations[AnnotationPureGroup] = "true"
}

func IsPureGroup(command *cobra.Command) bool {
	return command != nil &&
		command.Annotations != nil &&
		command.Annotations[AnnotationPureGroup] == "true"
}

func IsAction(command *cobra.Command) bool {
	return command != nil && command.Runnable() && !IsPureGroup(command)
}

func View(command *cobra.Command) platform.CommandView {
	return commandView{command: command}
}

func Path(command *cobra.Command) string {
	return commandView{command: command}.Path()
}

func setAnnotation(command *cobra.Command, key, value string) {
	if value == "" {
		delete(command.Annotations, key)
		return
	}
	command.Annotations[key] = value
}

type commandView struct {
	command *cobra.Command
}

func (v commandView) Path() string {
	parts := strings.Fields(v.command.CommandPath())
	if len(parts) <= 1 {
		return ""
	}
	return strings.Join(parts[1:], "/")
}

func (v commandView) Domain() string {
	value, _ := v.Annotation(AnnotationDomain)
	return value
}

func (v commandView) Risk() (platform.Risk, bool) {
	value, ok := v.Annotation(AnnotationRisk)
	return platform.Risk(value), ok
}

func (v commandView) Identities() []platform.Identity {
	value, ok := v.Annotation(AnnotationIdentities)
	if !ok || value == "" {
		return nil
	}
	parts := strings.Split(value, ",")
	identities := make([]platform.Identity, 0, len(parts))
	for _, part := range parts {
		identities = append(identities, platform.Identity(part))
	}
	return identities
}

func (v commandView) Annotation(key string) (string, bool) {
	for command := v.command; command != nil; command = command.Parent() {
		if value, ok := command.Annotations[key]; ok {
			return value, true
		}
	}
	return "", false
}
