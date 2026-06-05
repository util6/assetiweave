package errs

import "sort"

type Subtype string

const (
	SubtypeUnknown Subtype = "unknown"
)

const (
	SubtypeInvalidArgument Subtype = "invalid_argument"
	SubtypeInvalidJSON     Subtype = "invalid_json"
)

const (
	SubtypeInvalidConfig       Subtype = "invalid_config"
	SubtypeInvalidPluginConfig Subtype = "invalid_plugin_config"
)

const (
	SubtypeEngineNotFound     Subtype = "engine_not_found"
	SubtypeEngineProcess      Subtype = "engine_process"
	SubtypeEngineProtocol     Subtype = "engine_protocol"
	SubtypeEngineIncompatible Subtype = "engine_incompatible"
)

const (
	SubtypeCommandDenied Subtype = "command_denied"
	SubtypePolicyInvalid Subtype = "policy_invalid"
)

const (
	SubtypeConfirmationRequired Subtype = "confirmation_required"
)

var declaredSubtypes = map[Subtype]struct{}{
	SubtypeUnknown:              {},
	SubtypeInvalidArgument:      {},
	SubtypeInvalidJSON:          {},
	SubtypeInvalidConfig:        {},
	SubtypeInvalidPluginConfig:  {},
	SubtypeEngineNotFound:       {},
	SubtypeEngineProcess:        {},
	SubtypeEngineProtocol:       {},
	SubtypeEngineIncompatible:   {},
	SubtypeCommandDenied:        {},
	SubtypePolicyInvalid:        {},
	SubtypeConfirmationRequired: {},
}

func DeclaredSubtypes() []Subtype {
	subtypes := make([]Subtype, 0, len(declaredSubtypes))
	for subtype := range declaredSubtypes {
		subtypes = append(subtypes, subtype)
	}
	sort.Slice(subtypes, func(i, j int) bool {
		return subtypes[i] < subtypes[j]
	})
	return subtypes
}

func IsDeclaredSubtype(subtype Subtype) bool {
	_, ok := declaredSubtypes[subtype]
	return ok
}
